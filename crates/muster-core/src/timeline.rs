//! A workspace's timeline (`PRODUCT.md` §6): what each agent's turn there
//! changed, what changed between turns, and the undoing of either, each an
//! entry the person can look at and put the files back from (`PRD-14`).
//!
//! The daemon keeps every entry as a commit in a git store of Muster's
//! own: the commit holds the files as the entry left them, its one parent
//! the files as it found them, and its message what the entry was, in the
//! lines this module writes and reads. The store is the whole record.

use serde::{Deserialize, Serialize};

use crate::changes::ChangeSummary;
use crate::ids::SessionId;
use crate::time::Timestamp;

/// The most entries a workspace's timeline keeps; older ones go
/// (`ARC-14`).
pub const MAX_ENTRIES: usize = 100;

/// What made an entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryKind {
    /// An agent's turn, from when it began working to when it stopped.
    Turn {
        session: SessionId,
        /// The agent's name, such as `Claude Code`.
        agent: String,
    },
    /// What changed between agents' turns: the person's own edits, or
    /// anything else's.
    Between,
    /// The person put the files back as they were before entry `of`.
    Undo { of: u64 },
    /// The person moved the branch onto `onto` (`crate::stacks`).
    Moved { onto: String },
}

/// One entry of a workspace's timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Entry {
    /// Its place in the workspace's timeline, from 1.
    pub number: u64,
    pub kind: EntryKind,
    pub started: Timestamp,
    pub ended: Timestamp,
    /// What it changed in the files; `None` where it changed none, as a
    /// turn that only committed what was there.
    #[serde(default)]
    pub summary: Option<ChangeSummary>,
    /// The commit checked out as it began, and as it ended; `None` in a
    /// repository with no commit yet.
    #[serde(default)]
    pub head_before: Option<String>,
    #[serde(default)]
    pub head_after: Option<String>,
}

/// A workspace's timeline, as the daemon answers it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Timeline {
    /// Newest first.
    pub entries: Vec<Entry>,
}

/// What putting the files back from an entry would do, for the person to
/// say yes to first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct UndoPlan {
    pub number: u64,
    /// How many later entries' changes go with it.
    pub later: u64,
    /// The branch, where it moved since: whether it can move back too.
    #[serde(default)]
    pub branch: Option<BranchMove>,
}

/// Moving a workspace's branch back to where it was as an entry began.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BranchMove {
    pub branch: String,
    /// The commit it would move to, shortened.
    pub to: String,
    /// The commits it would leave, or with `forward`, take on again.
    pub commits: u64,
    pub forward: bool,
    /// Why it cannot move, where it cannot, such as its commits being
    /// pushed already.
    #[serde(default)]
    pub refused: Option<String>,
}

/// What an entry's message starts with.
const ENTRY: &str = "muster timeline entry";

/// What a snapshot's message starts with.
const SNAPSHOT: &str = "muster snapshot";

/// The message of the commit that keeps `entry`.
pub fn entry_message(entry: &Entry) -> String {
    let mut lines = vec![ENTRY.to_owned(), format!("number: {}", entry.number)];
    match &entry.kind {
        EntryKind::Turn { session, agent } => {
            lines.push("kind: turn".to_owned());
            lines.push(format!("session: {session}"));
            lines.push(format!("agent: {}", crate::text::one_line(agent)));
        }
        EntryKind::Between => lines.push("kind: between".to_owned()),
        EntryKind::Undo { of } => {
            lines.push("kind: undo".to_owned());
            lines.push(format!("of: {of}"));
        }
        EntryKind::Moved { onto } => {
            lines.push("kind: moved".to_owned());
            lines.push(format!("onto: {}", crate::text::one_line(onto)));
        }
    }
    lines.push(format!("started: {}", entry.started.as_millis()));
    lines.push(format!("ended: {}", entry.ended.as_millis()));
    if let Some(summary) = entry.summary {
        lines.push(format!("files: {}", summary.files));
        lines.push(format!("added: {}", summary.added));
        lines.push(format!("removed: {}", summary.removed));
    }
    if let Some(head) = &entry.head_before {
        lines.push(format!("head-before: {head}"));
    }
    if let Some(head) = &entry.head_after {
        lines.push(format!("head-after: {head}"));
    }
    lines.join("\n")
}

/// The entry a commit's message keeps; `None` for any other message.
pub fn parse_entry(message: &str) -> Option<Entry> {
    let fields = fields(message, ENTRY)?;
    let get = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    let number = |key: &str| get(key).and_then(|value| value.parse::<u64>().ok());
    let time = |key: &str| {
        get(key)
            .and_then(|value| value.parse::<i64>().ok())
            .map(Timestamp::from_millis)
    };
    let kind = match get("kind")? {
        "turn" => EntryKind::Turn {
            session: get("session")?.parse().ok()?,
            agent: get("agent").unwrap_or_default().to_owned(),
        },
        "between" => EntryKind::Between,
        "undo" => EntryKind::Undo { of: number("of")? },
        "moved" => EntryKind::Moved {
            onto: get("onto").unwrap_or_default().to_owned(),
        },
        _ => return None,
    };
    let summary = match (number("files"), number("added"), number("removed")) {
        (Some(files), Some(added), Some(removed)) => Some(ChangeSummary {
            files,
            added,
            removed,
        }),
        _ => None,
    };
    Some(Entry {
        number: number("number")?,
        kind,
        started: time("started")?,
        ended: time("ended")?,
        summary,
        head_before: get("head-before")
            .filter(|id| is_object_id(id))
            .map(str::to_owned),
        head_after: get("head-after")
            .filter(|id| is_object_id(id))
            .map(str::to_owned),
    })
}

/// The message of a snapshot: the files as they were `at`, with `head`
/// checked out.
pub fn snapshot_message(at: Timestamp, head: Option<&str>) -> String {
    let mut message = format!("{SNAPSHOT}\nat: {}", at.as_millis());
    if let Some(head) = head {
        message.push_str("\nhead: ");
        message.push_str(head);
    }
    message
}

/// When a snapshot was taken and what was checked out; `None` for any
/// other message.
pub fn parse_snapshot(message: &str) -> Option<(Timestamp, Option<String>)> {
    let fields = fields(message, SNAPSHOT)?;
    let get = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    let at = Timestamp::from_millis(get("at")?.parse().ok()?);
    let head = get("head").filter(|id| is_object_id(id)).map(str::to_owned);
    Some((at, head))
}

/// The `key: value` lines of a message that starts with `first`.
fn fields<'a>(message: &'a str, first: &str) -> Option<Vec<(&'a str, &'a str)>> {
    let mut lines = message.lines();
    if lines.next()?.trim() != first {
        return None;
    }
    Some(
        lines
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.trim(), value.trim()))
            .collect(),
    )
}

/// Whether `id` reads as a git object's id, in full.
pub fn is_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The entries whose changes putting the files back from `number` also
/// takes away: those after it.
pub fn later_than(timeline: &Timeline, number: u64) -> u64 {
    let later = timeline
        .entries
        .iter()
        .filter(|entry| entry.number > number && entry.summary.is_some())
        .count();
    u64::try_from(later).unwrap_or(u64::MAX)
}

/// The number of a turn among its session's turns in `timeline`, from 1,
/// counting only those still kept.
pub fn turn_of_session(timeline: &Timeline, entry: &Entry) -> Option<usize> {
    let EntryKind::Turn { session, .. } = &entry.kind else {
        return None;
    };
    let turns = timeline
        .entries
        .iter()
        .filter(|other| other.number <= entry.number)
        .filter(|other| matches!(&other.kind, EntryKind::Turn { session: of, .. } if of == session))
        .count();
    Some(turns)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    fn turn(number: u64) -> Entry {
        Entry {
            number,
            kind: EntryKind::Turn {
                session: SessionId::new(),
                agent: "Claude Code".to_owned(),
            },
            started: Timestamp::from_millis(1_000),
            ended: Timestamp::from_millis(61_000),
            summary: Some(ChangeSummary {
                files: 3,
                added: 40,
                removed: 2,
            }),
            head_before: Some(HEAD.to_owned()),
            head_after: None,
        }
    }

    #[test]
    fn an_entry_reads_back_as_it_was_written() {
        let entries = [
            turn(7),
            Entry {
                kind: EntryKind::Between,
                summary: None,
                head_before: None,
                ..turn(8)
            },
            Entry {
                kind: EntryKind::Undo { of: 7 },
                head_after: Some(HEAD.to_owned()),
                ..turn(9)
            },
        ];
        for entry in entries {
            assert_eq!(parse_entry(&entry_message(&entry)), Some(entry));
        }
    }

    #[test]
    fn an_agent_s_name_cannot_break_the_message_s_lines() {
        let mut entry = turn(1);
        entry.kind = EntryKind::Turn {
            session: SessionId::new(),
            agent: "Claude\nkind: between".to_owned(),
        };
        let read = parse_entry(&entry_message(&entry)).unwrap();
        assert!(
            matches!(read.kind, EntryKind::Turn { agent, .. } if agent == "Claude kind: between")
        );
    }

    #[test]
    fn other_messages_and_broken_entries_are_not_entries() {
        assert_eq!(parse_entry("Fix the login"), None);
        assert_eq!(parse_entry(""), None);
        let message = entry_message(&turn(1));
        assert_eq!(
            parse_entry(&message.replace("kind: turn", "kind: odd")),
            None
        );
        assert_eq!(
            parse_entry(&message.replace("number: 1", "number: x")),
            None
        );
        // A head that is no object id is left out, not taken.
        let read = parse_entry(&message.replace(HEAD, "--upload-pack=x")).unwrap();
        assert_eq!(read.head_before, None);
    }

    #[test]
    fn a_snapshot_says_when_it_was_taken_and_what_was_checked_out() {
        let at = Timestamp::from_millis(5);
        assert_eq!(
            parse_snapshot(&snapshot_message(at, Some(HEAD))),
            Some((at, Some(HEAD.to_owned())))
        );
        assert_eq!(
            parse_snapshot(&snapshot_message(at, None)),
            Some((at, None))
        );
        assert_eq!(parse_snapshot(&entry_message(&turn(1))), None);
    }

    #[test]
    fn later_entries_are_those_after_that_changed_files() {
        let timeline = Timeline {
            entries: vec![
                turn(4),
                Entry {
                    summary: None,
                    ..turn(3)
                },
                turn(2),
                turn(1),
            ],
        };
        assert_eq!(later_than(&timeline, 1), 2);
        assert_eq!(later_than(&timeline, 4), 0);
    }

    #[test]
    fn a_turn_is_counted_among_its_session_s() {
        let one = turn(1);
        let mut two = turn(2);
        two.kind = one.kind.clone();
        let other = turn(3);
        let timeline = Timeline {
            entries: vec![other.clone(), two.clone(), one.clone()],
        };
        assert_eq!(turn_of_session(&timeline, &two), Some(2));
        assert_eq!(turn_of_session(&timeline, &other), Some(1));
        let between = Entry {
            kind: EntryKind::Between,
            ..turn(4)
        };
        assert_eq!(turn_of_session(&timeline, &between), None);
    }
}
