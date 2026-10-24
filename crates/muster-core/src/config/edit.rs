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
    set_in(text, NEW_FILE, table, key, value)
}

/// [`set_flag`] in a file of another kind, which starts with `new_file`
/// when there is none yet, and for any value: a flag, a number, a text.
pub fn set_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    value: impl Into<toml_edit::Value>,
) -> Result<String, EditError> {
    set_texts_or_values(text, new_file, table, [(key, Some(value.into()))])
}

/// [`set_in`] for a list of texts, written on one line.
pub fn set_list_in(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    key: &str,
    values: &[String],
) -> Result<String, EditError> {
    let list: toml_edit::Array = values.iter().map(String::as_str).collect();
    set_in(text, new_file, table, key, list)
}

/// `text` without `[table] key`, and without the table if nothing is left
/// in it; as it was where neither is there.
pub fn remove_in(text: Option<&str>, table: &str, key: &str) -> Result<String, EditError> {
    let Some(text) = text else {
        return Ok(String::new());
    };
    let mut document = parse(text)?;
    let emptied = match document
        .get_mut(table)
        .and_then(|item| item.as_table_like_mut())
    {
        Some(entries) => {
            entries.remove(key);
            entries.is_empty()
        }
        None => false,
    };
    if emptied {
        document.remove(table);
    }
    Ok(document.to_string())
}

/// `text` without `[table]` and all in it.
pub fn remove_table_in(text: Option<&str>, table: &str) -> Result<String, EditError> {
    let Some(text) = text else {
        return Ok(String::new());
    };
    let mut document = parse(text)?;
    let above = document
        .get(table)
        .and_then(toml_edit::Item::as_table)
        .and_then(written_above);
    document.remove(table);
    if let Some((position, above)) = above {
        keep_above(&mut document, position, &above);
    }
    Ok(document.to_string())
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
    let values = values
        .iter()
        .map(|(key, value)| (*key, value.map(toml_edit::Value::from)));
    set_texts_or_values(text, new_file, table, values)
}

/// `text` with each `[table] key` of `values` set, or taken out where it
/// has none, as [`set_texts`] says.
fn set_texts_or_values<'a>(
    text: Option<&str>,
    new_file: &str,
    table: &str,
    values: impl IntoIterator<Item = (&'a str, Option<toml_edit::Value>)>,
) -> Result<String, EditError> {
    let mut document = parse(text.unwrap_or_default())?;
    let item = document.entry(table).or_insert_with(toml_edit::table);
    let Some(entries) = item.as_table_like_mut() else {
        return Err(EditError::NotATable(table.to_owned()));
    };
    set_each(entries, table, values)?;
    // A table written inline is spaced afresh, or a value's spacing would
    // come before the next one's comma.
    if let Some(inline) = item.as_inline_table_mut() {
        inline.fmt();
    }
    Ok(written(text, new_file, &document))
}

/// Sets each key of `values` in `entries`, at `path`, or takes it out
/// where it has none; a value already there keeps its comment.
fn set_each<'a>(
    entries: &mut dyn toml_edit::TableLike,
    path: &str,
    values: impl IntoIterator<Item = (&'a str, Option<toml_edit::Value>)>,
) -> Result<(), EditError> {
    for (key, value) in values {
        match (entries.get_mut(key), value) {
            (Some(existing), Some(value)) => {
                let Some(written) = existing.as_value_mut() else {
                    return Err(EditError::NotAValue(format!("{path}.{key}")));
                };
                let decor = written.decor().clone();
                *written = value;
                *written.decor_mut() = decor;
            }
            (None, Some(value)) => {
                entries.insert(key, toml_edit::Item::Value(value));
            }
            (Some(_), None) => {
                entries.remove(key);
            }
            (None, None) => {}
        }
    }
    Ok(())
}

/// The file as `document` writes it: after `new_file`'s first lines where
/// there was none (`text`).
fn written(text: Option<&str>, new_file: &str, document: &toml_edit::DocumentMut) -> String {
    match text {
        Some(_) => document.to_string(),
        None => format!("{new_file}\n{document}"),
    }
}

/// A value written into a table within a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written<'a> {
    Text(&'a str),
    Flag(bool),
}

impl From<Written<'_>> for toml_edit::Value {
    fn from(written: Written<'_>) -> Self {
        match written {
            Written::Text(text) => Self::from(text),
            Written::Flag(flag) => Self::from(flag),
        }
    }
}

/// `text` (`None` when there is no file yet, which then starts with
/// `new_file`) with each key of `values` in `[table.name]` set, or taken
/// out where it has none: a plugin's `[plugins.<id>]`. Values already
/// there keep their comments, and the rest of the file stays as written.
/// A new entry is written as its own `[table.name]` after the rest, with
/// no empty `[table]` above it; one written inline or with dotted keys is
/// changed where it is.
pub fn set_entry_in(
    text: Option<&str>,
    new_file: &str,
    (table, name): (&str, &str),
    values: &[(&str, Option<Written<'_>>)],
) -> Result<String, EditError> {
    let mut document = parse(text.unwrap_or_default())?;
    let outer = document.entry(table).or_insert_with(|| {
        let mut implicit = toml_edit::Table::new();
        implicit.set_implicit(true);
        toml_edit::Item::Table(implicit)
    });
    let inline = outer.is_inline_table();
    let Some(entries) = outer.as_table_like_mut() else {
        return Err(EditError::NotATable(table.to_owned()));
    };
    let path = format!("{table}.{name}");
    let item = entries.entry(name).or_insert_with(|| {
        if inline {
            toml_edit::Item::Value(toml_edit::Value::InlineTable(toml_edit::InlineTable::new()))
        } else {
            toml_edit::table()
        }
    });
    let Some(entry) = item.as_table_like_mut() else {
        return Err(EditError::NotATable(path));
    };
    let values = values
        .iter()
        .map(|(key, value)| (*key, value.map(toml_edit::Value::from)));
    set_each(entry, &path, values)?;
    // A table written inline is spaced afresh, or a value's spacing would
    // come before the next one's comma.
    if let Some(inline) = item.as_inline_table_mut() {
        inline.fmt();
    }
    if let Some(inline) = document
        .get_mut(table)
        .and_then(toml_edit::Item::as_inline_table_mut)
    {
        inline.fmt();
    }
    Ok(written(text, new_file, &document))
}

/// `text` without `[table.name]` and all in it, and without `[table]` if
/// nothing is left in it; as it was where neither is there.
pub fn remove_entry_in(
    text: Option<&str>,
    (table, name): (&str, &str),
) -> Result<String, EditError> {
    let Some(text) = text else {
        return Ok(String::new());
    };
    let mut document = parse(text)?;
    let above = document
        .get(table)
        .and_then(toml_edit::Item::as_table_like)
        .and_then(|entries| entries.get(name))
        .and_then(toml_edit::Item::as_table)
        .and_then(written_above);
    let emptied = match document
        .get_mut(table)
        .and_then(|item| item.as_table_like_mut())
    {
        Some(entries) => {
            entries.remove(name);
            entries.is_empty()
        }
        None => false,
    };
    if emptied {
        document.remove(table);
    }
    if let Some((position, above)) = above {
        keep_above(&mut document, position, &above);
    }
    Ok(document.to_string())
}

/// What is written above `table`'s header that is not its own: the
/// paragraphs a blank line sets apart from it, such as the file's own
/// first lines. The comment lines just above it are its own, and go with
/// it. With where the table was.
fn written_above(table: &toml_edit::Table) -> Option<(isize, String)> {
    let prefix = table.decor().prefix()?.as_str()?;
    let kept = &prefix[..prefix.rfind("\n\n").map_or(0, |at| at + 2)];
    let position = table.position()?;
    (!kept.trim().is_empty()).then(|| (position, kept.to_owned()))
}

/// Puts `above`, written above a table that was at `position`, above the
/// table that came after it, or at the end where none did.
fn keep_above(document: &mut toml_edit::DocumentMut, position: isize, above: &str) {
    fn after(
        table: &toml_edit::Table,
        position: isize,
        path: &mut Vec<String>,
        best: &mut Option<(isize, Vec<String>)>,
    ) {
        for (key, item) in table {
            let Some(inner) = item.as_table() else {
                continue;
            };
            path.push(key.to_owned());
            if let Some(at) = inner.position()
                && !inner.is_implicit()
                && at > position
                && best.as_ref().is_none_or(|(best, _)| at < *best)
            {
                *best = Some((at, path.clone()));
            }
            after(inner, position, path, best);
            path.pop();
        }
    }
    let mut best = None;
    after(document.as_table(), position, &mut Vec::new(), &mut best);
    let next = best.and_then(|(_, path)| {
        path.iter().try_fold(document.as_table_mut(), |table, key| {
            table.get_mut(key)?.as_table_mut()
        })
    });
    if let Some(table) = next {
        let own = table
            .decor()
            .prefix()
            .and_then(toml_edit::RawString::as_str)
            .unwrap_or_default()
            .to_owned();
        // One blank line between them is enough.
        let own = own
            .strip_prefix('\n')
            .filter(|_| above.ends_with("\n\n"))
            .unwrap_or(&own);
        table.decor_mut().set_prefix(format!("{above}{own}"));
    } else {
        let trailing = document.trailing().as_str().unwrap_or_default().to_owned();
        document.set_trailing(format!("{above}{trailing}"));
    }
}

fn parse(text: &str) -> Result<toml_edit::DocumentMut, EditError> {
    text.parse()
        .map_err(|error: toml_edit::TomlError| EditError::Syntax(error.message().to_owned()))
}

/// The keys `[table]` has in `text`; none where it has no such table, or
/// does not read.
pub fn keys_of(text: Option<&str>, table: &str) -> Vec<String> {
    text.and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|written| {
            written
                .get(table)
                .and_then(toml::Value::as_table)
                .map(|entries| entries.keys().cloned().collect())
        })
        .unwrap_or_default()
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

    const PLUGIN: (&str, &str) = ("plugins", "zoetrope");

    fn installed() -> Vec<(&'static str, Option<Written<'static>>)> {
        vec![
            (
                "git",
                Some(Written::Text("https://github.com/someone/zoetrope")),
            ),
            ("ref", None),
            ("enabled", Some(Written::Flag(true))),
        ]
    }

    #[test]
    fn a_plugin_s_entry_is_its_own_table_after_the_rest_with_no_empty_one_above() {
        let new_file = "# Muster's own.\n";
        let text = set_entry_in(None, new_file, PLUGIN, &installed()).unwrap();
        assert_eq!(
            text,
            "# Muster's own.\n\n[plugins.zoetrope]\ngit = \"https://github.com/someone/zoetrope\"\n\
             enabled = true\n"
        );
        let written =
            "# mine\n[notch]\nshow = false # not on this Mac\n\n[keys]\nnew_session = \"N\"\n";
        let text = set_entry_in(Some(written), new_file, PLUGIN, &installed()).unwrap();
        assert_eq!(
            text,
            format!(
                "{written}\n[plugins.zoetrope]\ngit = \"https://github.com/someone/zoetrope\"\n\
                 enabled = true\n"
            )
        );
        // Another comes after it.
        let more = set_entry_in(
            Some(&text),
            new_file,
            ("plugins", "my-panel"),
            &[("folder", Some(Written::Text("/p")))],
        )
        .unwrap();
        assert!(
            more.ends_with("enabled = true\n\n[plugins.my-panel]\nfolder = \"/p\"\n"),
            "{more}"
        );
        let read: toml::Table = more.parse().unwrap();
        assert_eq!(read["plugins"]["my-panel"]["folder"].as_str(), Some("/p"));
        assert_eq!(read["notch"]["show"].as_bool(), Some(false));
    }

    #[test]
    fn a_plugin_s_values_change_where_they_are_keeping_their_comments() {
        let written = "[plugins.zoetrope] # from the docs\ngit = \"https://h/z\"\nref = \"v1\"\n\
                       enabled = true # for now\n\n[plugins.other]\nfolder = \"/o\"\n";
        let text = set_entry_in(
            Some(written),
            "",
            PLUGIN,
            &[("enabled", Some(Written::Flag(false))), ("ref", None)],
        )
        .unwrap();
        assert_eq!(
            text,
            "[plugins.zoetrope] # from the docs\ngit = \"https://h/z\"\n\
             enabled = false # for now\n\n[plugins.other]\nfolder = \"/o\"\n"
        );
        assert_eq!(
            set_entry_in(
                Some("plugins.zoetrope.enabled = true\n"),
                "",
                PLUGIN,
                &[("enabled", Some(Written::Flag(false)))]
            )
            .unwrap(),
            "plugins.zoetrope.enabled = false\n"
        );
        assert_eq!(
            set_entry_in(
                Some("plugins = { zoetrope = { enabled = true } }\n"),
                "",
                PLUGIN,
                &[("approved", Some(Written::Text("ab")))]
            )
            .unwrap(),
            "plugins = { zoetrope = { enabled = true, approved = \"ab\" } }\n"
        );
        assert_eq!(
            set_entry_in(
                Some("plugins = {}\n"),
                "",
                PLUGIN,
                &[("enabled", Some(Written::Flag(true)))]
            )
            .unwrap(),
            "plugins = { zoetrope = { enabled = true } }\n"
        );
    }

    #[test]
    fn what_cannot_hold_a_plugin_is_refused_not_rewritten() {
        let flag = [("enabled", Some(Written::Flag(true)))];
        assert_eq!(
            set_entry_in(Some("plugins = 3\n"), "", PLUGIN, &flag),
            Err(EditError::NotATable("plugins".to_owned()))
        );
        assert_eq!(
            set_entry_in(Some("[plugins]\nzoetrope = 3\n"), "", PLUGIN, &flag),
            Err(EditError::NotATable("plugins.zoetrope".to_owned()))
        );
        assert_eq!(
            set_entry_in(Some("[plugins.zoetrope.enabled]\n"), "", PLUGIN, &flag),
            Err(EditError::NotAValue("plugins.zoetrope.enabled".to_owned()))
        );
    }

    #[test]
    fn a_plugin_s_entry_goes_whole_and_its_table_with_the_last() {
        let written = "# mine\n[notch]\nshow = false\n\n[plugins.zoetrope]\ngit = \"https://h/z\"\n\
                       enabled = true\n\n[plugins.other]\nfolder = \"/o\"\n";
        let text = remove_entry_in(Some(written), PLUGIN).unwrap();
        assert_eq!(
            text,
            "# mine\n[notch]\nshow = false\n\n[plugins.other]\nfolder = \"/o\"\n"
        );
        let text = remove_entry_in(Some(&text), ("plugins", "other")).unwrap();
        assert_eq!(text, "# mine\n[notch]\nshow = false\n");
        // Not there: as it was.
        assert_eq!(remove_entry_in(Some(&text), PLUGIN).unwrap(), text);
        // What is written above it, a blank line apart, stays; its own
        // comment goes with it.
        let file =
            "# Your own settings.\n\n# zoe, for graphs\n[plugins.zoetrope]\nenabled = true\n";
        assert_eq!(
            remove_entry_in(Some(file), PLUGIN).unwrap(),
            "# Your own settings.\n\n"
        );
        let file =
            "# Your own settings.\n\n[plugins.zoetrope]\nenabled = true\n\n[notch]\nshow = false\n";
        assert_eq!(
            remove_entry_in(Some(file), PLUGIN).unwrap(),
            "# Your own settings.\n\n[notch]\nshow = false\n"
        );
        let new = set_entry_in(None, "# Muster's own.\n", PLUGIN, &installed()).unwrap();
        assert_eq!(
            remove_entry_in(Some(&new), PLUGIN).unwrap(),
            "# Muster's own.\n\n"
        );
        assert_eq!(remove_entry_in(None, PLUGIN).unwrap(), "");
        assert_eq!(
            remove_entry_in(
                Some("plugins = { zoetrope = { enabled = true } }\n"),
                PLUGIN
            )
            .unwrap(),
            ""
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
