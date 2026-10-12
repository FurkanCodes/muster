//! Changing the person's overrides of a project's settings from the UI,
//! keeping the rest of their file as they wrote it, comments included; and
//! telling which file sets a value. Text in, text out: the daemon reads and
//! writes the files.

use crate::settings::SettingSource;

/// What a new overrides file starts with, so a person who finds it knows
/// what it is.
pub const NEW_FILE: &str = "# Your own settings for this project, over the muster.toml in its \
                            repository.\n# Muster's Settings screen writes here, and so can you.\n";

/// `text`, the overrides as they are (`None` when there is no file yet),
/// with `[table] key` set to `value`. A table written inline or with
/// dotted keys is changed where it is; a value already there keeps its
/// comment.
pub fn set_flag(text: Option<&str>, table: &str, key: &str, value: bool) -> Result<String, String> {
    let mut document: toml_edit::DocumentMut = text
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| format!("is not valid TOML: {}", error.message()))?;
    let item = document.entry(table).or_insert_with(toml_edit::table);
    let Some(entries) = item.as_table_like_mut() else {
        return Err(format!("{table} is not a table"));
    };
    match entries.get_mut(key) {
        Some(existing) => {
            let Some(written) = existing.as_value_mut() else {
                return Err(format!("{table}.{key} is not a value"));
            };
            let decor = written.decor().clone();
            *written = toml_edit::Value::from(value);
            *written.decor_mut() = decor;
        }
        None => {
            entries.insert(key, toml_edit::value(value));
            // A table written inline is spaced afresh, or its last value's
            // spacing would come before the new one's comma.
            if let Some(inline) = item.as_inline_table_mut() {
                inline.fmt();
            }
        }
    }
    Ok(match text {
        Some(_) => document.to_string(),
        None => format!("{NEW_FILE}\n{document}"),
    })
}

/// Where `[table] key` gets its value: the person's overrides (`local`)
/// over the repository's `muster.toml`, over Muster's default.
pub fn source_of(
    repository: Option<&str>,
    local: Option<&str>,
    (table, key): (&str, &str),
) -> SettingSource {
    let sets = |text: Option<&str>| {
        text.and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|written| {
                written
                    .get(table)
                    .and_then(toml::Value::as_table)
                    .map(|entries| entries.contains_key(key))
            })
            .unwrap_or(false)
    };
    if sets(local) {
        SettingSource::Yours
    } else if sets(repository) {
        SettingSource::Project
    } else {
        SettingSource::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mine(text: Option<&str>, value: bool) -> String {
        set_flag(text, "tickets", "mine", value).unwrap()
    }

    fn reads_mine(text: &str) -> Option<bool> {
        text.parse::<toml::Table>().unwrap()["tickets"]
            .get("mine")
            .and_then(toml::Value::as_bool)
    }

    #[test]
    fn a_new_file_says_what_it_is_and_holds_the_value() {
        let text = mine(None, true);
        assert!(text.starts_with(NEW_FILE), "{text}");
        assert!(text.ends_with("[tickets]\nmine = true\n"), "{text}");
        assert_eq!(reads_mine(&text), Some(true));
    }

    #[test]
    fn the_rest_of_the_file_is_kept_as_written() {
        let written = "# mine, not Muster's\n[project]\nmemory = \"8g\" # for Gradle\n\n\
                       [tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\n\
                       project = \"APP\"\n\n[pull_requests]\nhost = \"gitlab\"\n";
        let text = mine(Some(written), true);
        assert_eq!(
            text,
            "# mine, not Muster's\n[project]\nmemory = \"8g\" # for Gradle\n\n\
             [tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\n\
             project = \"APP\"\nmine = true\n\n[pull_requests]\nhost = \"gitlab\"\n"
        );
        // Turned off again, the line stays where it is, and its comment too.
        let commented = text.replace("mine = true", "mine = true # just mine");
        assert_eq!(
            mine(Some(&commented), false),
            commented.replace("mine = true", "mine = false")
        );
    }

    #[test]
    fn inline_and_dotted_tables_are_changed_where_they_are() {
        assert_eq!(
            mine(Some("tickets = { tracker = \"github\" }\n"), true),
            "tickets = { tracker = \"github\", mine = true }\n"
        );
        assert_eq!(
            mine(Some("tickets.mine = true\n"), false),
            "tickets.mine = false\n"
        );
    }

    #[test]
    fn what_cannot_hold_the_value_is_refused_not_rewritten() {
        assert!(
            set_flag(Some("tickets = 3\n"), "tickets", "mine", true)
                .unwrap_err()
                .contains("tickets is not a table")
        );
        assert!(
            set_flag(Some("[tickets]\nmine = ["), "tickets", "mine", true)
                .unwrap_err()
                .contains("not valid TOML")
        );
    }

    #[test]
    fn the_person_s_file_wins_then_the_repository_s() {
        let key = ("tickets", "mine");
        let set = Some("[tickets]\nmine = false\n");
        let unset = Some("[tickets]\ntracker = \"github\"\n");
        assert_eq!(source_of(set, set, key), SettingSource::Yours);
        assert_eq!(source_of(set, unset, key), SettingSource::Project);
        assert_eq!(source_of(set, None, key), SettingSource::Project);
        assert_eq!(source_of(unset, None, key), SettingSource::Default);
        assert_eq!(source_of(None, None, key), SettingSource::Default);
        assert_eq!(
            source_of(None, Some("tickets.mine = true"), key),
            SettingSource::Yours
        );
    }
}
