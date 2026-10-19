//! Changing the person's overrides of a project's settings from the UI,
//! keeping the rest of their file as they wrote it, comments included; and
//! telling which file sets a value. Text in, text out: the daemon reads and
//! writes the files.

use crate::settings::SettingSource;

/// Why the overrides could not be changed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("is not valid TOML: {0}")]
    Syntax(String),
    #[error("{0} is not a table")]
    NotATable(String),
    #[error("{0} is not a value")]
    NotAValue(String),
}

/// What a new overrides file starts with, so a person who finds it knows
/// what it is.
pub const NEW_FILE: &str = "# Your own settings for this project, over the muster.toml in its \
                            repository.\n# Muster's Settings screen writes here, and so can you.\n";

/// `text`, the overrides as they are (`None` when there is no file yet),
/// with `[table] key` set to `value`. A table written inline or with
/// dotted keys is changed where it is; a value already there keeps its
/// comment.
pub fn set_flag(
    text: Option<&str>,
    table: &str,
    key: &str,
    value: bool,
) -> Result<String, EditError> {
    set_flag_in(text, NEW_FILE, table, key, value)
}

/// [`set_flag`] in a file of another kind: a new one starts with
/// `new_file`, saying what it is.
pub fn set_flag_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    value: bool,
) -> Result<String, EditError> {
    set_value_in(text, new_file, table, key, toml_edit::Value::from(value))
}

/// [`set_flag_in`] for a whole number.
pub fn set_number_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    value: i64,
) -> Result<String, EditError> {
    set_value_in(text, new_file, table, key, toml_edit::Value::from(value))
}

/// [`set_flag_in`] for a text.
pub fn set_text_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    value: &str,
) -> Result<String, EditError> {
    set_value_in(text, new_file, table, key, toml_edit::Value::from(value))
}

/// `[table] key` set to `value` in `text`, as [`set_flag_in`] says.
fn set_value_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    value: toml_edit::Value,
) -> Result<String, EditError> {
    let mut document: toml_edit::DocumentMut = text
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| EditError::Syntax(error.message().to_owned()))?;
    let item = document.entry(table).or_insert_with(toml_edit::table);
    let Some(entries) = item.as_table_like_mut() else {
        return Err(EditError::NotATable(table.to_owned()));
    };
    if let Some(existing) = entries.get_mut(key) {
        let Some(written) = existing.as_value_mut() else {
            return Err(EditError::NotAValue(format!("{table}.{key}")));
        };
        let decor = written.decor().clone();
        *written = value;
        *written.decor_mut() = decor;
    } else {
        entries.insert(key, toml_edit::Item::Value(value));
        // A table written inline is spaced afresh, or its last value's
        // spacing would come before the new one's comma.
        if let Some(inline) = item.as_inline_table_mut() {
            inline.fmt();
        }
    }
    Ok(match text {
        Some(_) => document.to_string(),
        None => format!("{new_file}\n{document}"),
    })
}

/// `text`, the overrides as they are (`None` when there is no file yet),
/// with each `[table] key` of `values` set to its text, or taken out where
/// it has none. Values already there keep their comments; a table left
/// empty is kept, as the person may have written it.
pub fn set_texts(
    text: Option<&str>,
    table: &str,
    values: &[(&str, Option<&str>)],
) -> Result<String, EditError> {
    set_texts_in(text, NEW_FILE, table, values)
}

/// [`set_texts`] in a file that starts with `new_file` when there is none
/// yet, as Muster's own settings do.
pub fn set_texts_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    values: &[(&str, Option<&str>)],
) -> Result<String, EditError> {
    let mut document: toml_edit::DocumentMut = text
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| EditError::Syntax(error.message().to_owned()))?;
    let item = document.entry(table).or_insert_with(toml_edit::table);
    let Some(entries) = item.as_table_like_mut() else {
        return Err(EditError::NotATable(table.to_owned()));
    };
    for (key, value) in values {
        match (entries.get_mut(key), value) {
            (Some(existing), Some(value)) => {
                let Some(written) = existing.as_value_mut() else {
                    return Err(EditError::NotAValue(format!("{table}.{key}")));
                };
                let decor = written.decor().clone();
                *written = toml_edit::Value::from(*value);
                *written.decor_mut() = decor;
            }
            (None, Some(value)) => {
                entries.insert(key, toml_edit::value(*value));
            }
            (Some(_), None) => {
                entries.remove(key);
            }
            (None, None) => {}
        }
    }
    // A table written inline is spaced afresh, or a value's spacing would
    // come before the next one's comma.
    if let Some(inline) = item.as_inline_table_mut() {
        inline.fmt();
    }
    Ok(match text {
        Some(_) => document.to_string(),
        None => format!("{new_file}\n{document}"),
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
        assert_eq!(
            set_flag(Some("tickets = 3\n"), "tickets", "mine", true),
            Err(EditError::NotATable("tickets".to_owned()))
        );
        assert!(matches!(
            set_flag(Some("[tickets]\nmine = ["), "tickets", "mine", true),
            Err(EditError::Syntax(_))
        ));
    }

    #[test]
    fn texts_are_set_and_taken_out_keeping_the_rest_as_written() {
        let jira = [
            ("tracker", Some("jira")),
            ("site", Some("acme.atlassian.net")),
            ("project", Some("APP")),
            ("team", None),
        ];
        let text = set_texts(None, "tickets", &jira).unwrap();
        assert!(text.starts_with(NEW_FILE), "{text}");
        assert!(
            text.ends_with(
                "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"APP\"\n"
            ),
            "{text}"
        );

        let written = "# mine\n[tickets]\nmine = true # just mine\ntracker = \"github\" # was\n\
                       team = \"ENG\"\n\n[pull_requests]\nhost = \"gitlab\"\n";
        assert_eq!(
            set_texts(Some(written), "tickets", &jira).unwrap(),
            "# mine\n[tickets]\nmine = true # just mine\ntracker = \"jira\" # was\n\
             site = \"acme.atlassian.net\"\nproject = \"APP\"\n\n[pull_requests]\nhost = \"gitlab\"\n"
        );
        // Following the git remote again: the keys go, the rest stays.
        let none: Vec<(&str, Option<&str>)> = ["tracker", "site", "project"]
            .map(|key| (key, None))
            .to_vec();
        assert_eq!(
            set_texts(
                Some("[tickets]\nmine = true\ntracker = \"jira\"\nsite = \"a\"\n"),
                "tickets",
                &none
            )
            .unwrap(),
            "[tickets]\nmine = true\n"
        );
        assert_eq!(
            set_texts(Some("tickets = { mine = true }\n"), "tickets", &jira[..1]).unwrap(),
            "tickets = { mine = true, tracker = \"jira\" }\n"
        );
        assert_eq!(
            set_texts(Some("tickets = 3\n"), "tickets", &jira),
            Err(EditError::NotATable("tickets".to_owned()))
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
