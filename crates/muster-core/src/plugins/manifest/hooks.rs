//! A manifest's `[[hooks]]`, read as strictly as the rest: the events each
//! runs for by name, its command, its deadline, and its filters, each
//! refused where none of its events could ever carry its value.

use super::{ManifestError, Read};
use crate::plugins::events::EventKind;
use crate::plugins::hooks::{Hook, MOST_TIMEOUT, REASONS, TIMEOUT, When, reason_name};
use crate::plugins::placeholders::Placeholder;

const HOOK_KEYS: [&str; 5] = ["id", "on", "command", "timeout", "when"];
const WHEN_KEYS: [&str; 4] = ["project", "harness", "reason", "away"];

impl Read<'_> {
    pub(super) fn hooks(&self) -> Result<Vec<Hook>, ManifestError> {
        let mut hooks: Vec<Hook> = Vec::new();
        for read in self.tables("hooks", "a [[hooks]] table for each hook")? {
            read.known(&HOOK_KEYS)?;
            let id = read.required("id", "the hook's id, as `id = \"ping\"`")?;
            if !crate::plugins::is_id(&id) {
                return Err(read.invalid(
                    "id",
                    "1 to 32 of a-z, 0-9 and -, starting with a letter or digit",
                ));
            }
            if hooks.iter().any(|hook| hook.id == id) {
                return Err(ManifestError::Twice {
                    key: read.key("id"),
                    id,
                });
            }
            let on = read.events()?;
            let command = read.command("command")?;
            read.hook_placeholders(&command, &on)?;
            let timeout = match read.table.get("timeout") {
                None => TIMEOUT,
                Some(toml::Value::Integer(seconds)) => u64::try_from(*seconds)
                    .ok()
                    .filter(|seconds| (1..=MOST_TIMEOUT).contains(seconds))
                    .ok_or_else(|| {
                        read.invalid("timeout", "a whole number of seconds from 1 to 60")
                    })?,
                Some(_) => {
                    return Err(read.invalid("timeout", "a whole number of seconds from 1 to 60"));
                }
            };
            let when = match read.table.get("when") {
                None => When::default(),
                Some(value) => {
                    let at = format!("{}when.", read.at);
                    Read::table_at(value, at, "a [hooks.when] table")?.when(&on)?
                }
            };
            hooks.push(Hook {
                id,
                on,
                command,
                timeout,
                when,
            });
        }
        Ok(hooks)
    }

    /// The events at `on`, by name, each once.
    fn events(&self) -> Result<Vec<EventKind>, ManifestError> {
        let expected = "a list of events, as [\"session.needs_you\", \"session.finished\"]";
        if !self.table.contains_key("on") {
            return Err(ManifestError::Missing {
                key: self.key("on"),
                expected,
            });
        }
        let names = self.list("on", expected)?;
        if names.is_empty() {
            return Err(self.invalid("on", expected));
        }
        let mut on: Vec<EventKind> = Vec::new();
        for name in names {
            let kind = EventKind::named(&name).ok_or_else(|| ManifestError::UnknownEvent {
                key: self.key("on"),
                word: name,
                known: EventKind::ALL.map(EventKind::name).join(" "),
            })?;
            if !on.contains(&kind) {
                on.push(kind);
            }
        }
        Ok(on)
    }

    /// Refuses a word in braces in a hook's `command` that is no
    /// placeholder, one a hook is not told, or a session's where one of
    /// its events is no session's.
    fn hook_placeholders(&self, command: &[String], on: &[EventKind]) -> Result<(), ManifestError> {
        for word in command
            .iter()
            .flat_map(|argument| Placeholder::words_in(argument))
        {
            let key = self.key("command");
            match Placeholder::named(word) {
                None => {
                    return Err(ManifestError::UnknownPlaceholder {
                        key,
                        word: word.to_owned(),
                        known: Placeholder::ALL
                            .into_iter()
                            .filter(|placeholder| placeholder.is_told_to_hooks())
                            .map(|placeholder| placeholder.to_string())
                            .collect::<Vec<_>>()
                            .join(" "),
                    });
                }
                Some(placeholder) if !placeholder.is_told_to_hooks() => {
                    return Err(ManifestError::HookPlaceholder {
                        key,
                        word: word.to_owned(),
                    });
                }
                Some(placeholder) if placeholder.is_a_session_s() => {
                    if let Some(event) = on.iter().find(|kind| !kind.is_a_session_s()) {
                        return Err(ManifestError::NoSession {
                            key,
                            word: word.to_owned(),
                            event: event.name(),
                        });
                    }
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// A hook's `[hooks.when]`, for its events `on`.
    fn when(&self, on: &[EventKind]) -> Result<When, ManifestError> {
        self.known(&WHEN_KEYS)?;
        let project = self.list("project", "a list of projects' names, as [\"muster\"]")?;
        let harness = self.list("harness", "a list of harnesses, as [\"claude\", \"codex\"]")?;
        let reasons = "a list of reasons: question, permission, startup, action";
        let reason = self
            .list("reason", reasons)?
            .iter()
            .map(|name| {
                REASONS
                    .into_iter()
                    .find(|reason| reason_name(*reason) == name)
                    .ok_or_else(|| self.invalid("reason", reasons))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let away = match self.table.get("away") {
            None => None,
            Some(toml::Value::Boolean(away)) => Some(*away),
            Some(_) => return Err(self.invalid("away", "true or false")),
        };
        if !harness.is_empty() && !on.iter().any(|kind| kind.is_a_session_s()) {
            return Err(ManifestError::NotCarried {
                key: self.key("harness"),
                carried_by: "a session's events (session.*, ticket.assigned)",
            });
        }
        if !reason.is_empty() && !on.iter().any(|kind| kind.has_reason()) {
            return Err(ManifestError::NotCarried {
                key: self.key("reason"),
                carried_by: "session.needs_you",
            });
        }
        Ok(When {
            project,
            harness,
            reason,
            away,
        })
    }
}

#[cfg(test)]
mod tests;
