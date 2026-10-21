//! Stacked work (`PRODUCT.md` §6, `PRD-14`): a worktree made on top of
//! another's branch rather than on the project's base, so an agent can
//! build on work not merged yet. Its work is measured, reviewed, tidied
//! and opened as a pull request against that branch; and at the person's
//! request it moves onto that branch's newest commit, or onto the base
//! once that branch has merged.

use serde::{Deserialize, Serialize};

/// The protocol version that knows stacked work.
pub const STACKS_PROTOCOL: u32 = 28;

/// The branch a workspace's branch is stacked on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Stacked {
    /// That branch's name, such as `muster/issue-12-fix-the-login`.
    pub branch: String,
    /// The commit of it the work follows: its newest when the worktree
    /// was made or last moved onto it.
    pub at: String,
}

#[cfg(test)]
mod tests {
    use crate::protocol::Reply;
    use crate::time::Timestamp;
    use crate::timeline::{Entry, EntryKind, Timeline, entry_message, parse_entry};

    fn moved() -> Entry {
        Entry {
            number: 4,
            kind: EntryKind::Moved {
                onto: "origin/main".to_owned(),
            },
            started: Timestamp::from_millis(1_000),
            ended: Timestamp::from_millis(1_000),
            summary: None,
            head_before: Some("a".repeat(40)),
            head_after: Some("b".repeat(40)),
        }
    }

    #[test]
    fn a_move_is_kept_as_an_entry_and_read_back() {
        assert_eq!(parse_entry(&entry_message(&moved())), Some(moved()));
    }

    #[test]
    fn an_older_client_is_told_a_move_as_edits_between_turns() {
        let reply = || {
            Reply::Timeline(Timeline {
                entries: vec![moved()],
            })
        };
        let kind = |reply: Reply| match reply {
            Reply::Timeline(timeline) => timeline.entries[0].kind.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            kind(reply().for_version(super::STACKS_PROTOCOL - 1)),
            EntryKind::Between
        );
        assert_eq!(
            kind(reply().for_version(super::STACKS_PROTOCOL)),
            moved().kind
        );
        assert_eq!(Reply::Pong.for_version(1), Reply::Pong);
    }
}
