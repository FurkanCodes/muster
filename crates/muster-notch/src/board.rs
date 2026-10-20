//! What the notch shows, worked out from the daemon's state alone: no IO,
//! no AppKit, so it is tested anywhere (`ARC-22`).
//!
//! The notch shows only while the person looks at no Muster window and an
//! agent works or waits on them (a question asked, or news not yet seen),
//! and only while Muster's own settings have it on. Level with the notch,
//! the counts: on the left the questions asked of the person, and on the
//! right what works, or once nothing works, what finished unseen; pointed
//! at, the sessions that ask the person something, oldest first, then the
//! trouble in the work (`trouble`: conflicts, failed checks), then the
//! sessions with news, then those working. Each line of a session ends
//! with what its work comes to. What newly needs the person (a question,
//! a conflict, failed checks) is told from what did before, for the notch
//! to show it a moment unasked.

use muster_core::protocol::Snapshot;
use muster_core::{Session, SessionId, SessionStatus, Timestamp, WaitReason};

use crate::trouble::{self, Key};

/// The most lines listed; the rest are counted.
pub const MOST_ROWS: usize = 8;

/// What the notch shows now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    /// Whether it shows at all.
    pub shown: bool,
    /// Agents working.
    pub working: usize,
    /// Sessions asking the person something.
    pub asking: usize,
    /// Sessions with news the person has not seen, asking nothing.
    pub news: usize,
    /// Trouble in the work: conflicts and failed checks.
    pub troubles: usize,
    /// Everything that needs the person, listed or not: the sessions
    /// asking, and the trouble; to tell what is new.
    pub alerts: Vec<Key>,
    /// Those waiting, then those working, at most [`MOST_ROWS`].
    pub rows: Vec<Row>,
    /// Sessions waiting or working beyond the rows.
    pub more: usize,
}

/// One line listed: a session, or trouble in the work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// What the line is of, to tell it again as the state changes.
    pub key: Key,
    /// The session a click on it shows.
    pub session: SessionId,
    pub mark: Mark,
    /// The harness it runs, which picks its logo.
    pub harness: String,
    pub project: String,
    /// The project's place among the projects, which picks its colour, as
    /// in the UI.
    pub colour: usize,
    pub title: String,
    /// What it is doing: its activity while it works or asks, else what
    /// waits.
    pub doing: String,
    /// How long it has been so: `12s`, `4m`, `2h`; nothing for trouble.
    pub since: String,
    /// What its work comes to: `+120 −30  #88 ✗`.
    pub work: String,
}

/// What a listed line's mark says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// It asks the person something.
    Asking,
    /// It has news the person has not seen: finished, or ended.
    News,
    /// It works.
    Working,
    /// Two workspaces' work would conflict.
    Conflict,
    /// A pull request's checks failed.
    ChecksFailed,
}

impl Board {
    /// The board for `state` at `now`. `connected` is whether the daemon
    /// answers; without it nothing is shown.
    pub fn of(state: &Snapshot, connected: bool, now: Timestamp) -> Self {
        let mut waiting: Vec<&Session> = state
            .sessions
            .iter()
            .filter(|session| waits(session))
            .collect();
        // Questions first, then news, each oldest first.
        waiting.sort_by_key(|session| (!asks(session), session.status_since));
        let working: Vec<&Session> = state
            .sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running && !waits(session))
            .collect();
        let busy = !waiting.is_empty() || !working.is_empty();
        let (asking, news): (Vec<&Session>, Vec<&Session>) =
            waiting.iter().partition(|session| asks(session));
        let troubles = trouble::troubles(state);
        let mut rows: Vec<Row> = asking
            .iter()
            .map(|session| row(state, session, now))
            .chain(troubles.iter().map(|found| trouble_row(state, found)))
            .chain(
                news.iter()
                    .chain(&working)
                    .map(|session| row(state, session, now)),
            )
            .collect();
        let more = rows.len().saturating_sub(MOST_ROWS);
        rows.truncate(MOST_ROWS);
        Self {
            shown: connected && busy && !state.looked_at && state.preferences.notch,
            working: working.len(),
            asking: asking.len(),
            news: news.len(),
            troubles: troubles.len(),
            alerts: asking
                .iter()
                .map(|session| Key::Session(session.id))
                .chain(troubles.iter().map(|found| found.key))
                .collect(),
            rows,
            more,
        }
    }

    /// Until a listed time reads differently. Hidden boards have no clock;
    /// minute/hour/day labels wake only at their next boundary (ARC-13).
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn next_change(
        &self,
        state: &Snapshot,
        now: Timestamp,
    ) -> Option<std::time::Duration> {
        if !self.shown {
            return None;
        }
        self.rows
            .iter()
            .filter(|row| !row.since.is_empty())
            .filter_map(|row| {
                let session = state
                    .sessions
                    .iter()
                    .find(|session| session.id == row.session)?;
                let elapsed = now
                    .as_millis()
                    .saturating_sub(session.status_since.as_millis());
                let every: i64 = match elapsed.max(0) / 1000 {
                    0..60 => 1000,
                    60..3600 => 60_000,
                    3600..86_400 => 3_600_000,
                    _ => 86_400_000,
                };
                let left = if elapsed < 0 {
                    every.saturating_sub(elapsed)
                } else {
                    every - elapsed % every
                };
                Some(std::time::Duration::from_millis(
                    u64::try_from(left).unwrap_or(1000),
                ))
            })
            .min()
    }

    /// Nothing to show.
    pub fn hidden() -> Self {
        Self {
            shown: false,
            working: 0,
            asking: 0,
            news: 0,
            troubles: 0,
            alerts: Vec::new(),
            rows: Vec::new(),
            more: 0,
        }
    }

    /// What needs the person now that did not in `before`, while the notch
    /// was shown then and is now: what came while the person looked at
    /// Muster, they have seen there.
    fn anew<'a>(&'a self, before: &'a Self) -> impl Iterator<Item = &'a Key> {
        let both = self.shown && before.shown;
        self.alerts
            .iter()
            .filter(move |key| both && !before.alerts.contains(key))
    }

    /// Whether a session asks now that did not in `before`: a new question,
    /// which the notch draws the eye to for a moment.
    pub fn asks_anew(&self, before: &Self) -> bool {
        self.anew(before).any(|key| matches!(key, Key::Session(_)))
    }

    /// The listed lines of what newly needs the person since `before`, for
    /// the notch to show a moment unasked.
    pub fn fresh(&self, before: &Self) -> Vec<Row> {
        let new: Vec<&Key> = self.anew(before).collect();
        self.rows
            .iter()
            .filter(|row| new.contains(&&row.key))
            .cloned()
            .collect()
    }

    /// The first row working where rows waiting on the person come before
    /// it: where the list's two groups meet.
    pub fn first_working(&self) -> Option<usize> {
        let at = self.rows.iter().position(|row| row.mark == Mark::Working)?;
        (at > 0).then_some(at)
    }
}

/// Whether the session waits on the person, as the UI's Needs you has it.
fn waits(session: &Session) -> bool {
    asks(session) || session.unseen
}

fn asks(session: &Session) -> bool {
    matches!(session.status, SessionStatus::NeedsYou { .. })
}

fn row(state: &Snapshot, session: &Session, now: Timestamp) -> Row {
    let (name, colour) = project_of(state, session.workspace);
    let mark = if asks(session) {
        Mark::Asking
    } else if session.unseen {
        Mark::News
    } else {
        Mark::Working
    };
    let doing = match (&session.activity, mark) {
        (Some(activity), Mark::Asking | Mark::Working) => activity.text.clone(),
        _ => describe(session.status).to_owned(),
    };
    Row {
        key: Key::Session(session.id),
        session: session.id,
        mark,
        harness: session.harness.clone(),
        project: printable(&name),
        colour,
        title: printable(&session.title),
        doing: printable(&doing),
        since: ago(session.status_since, now),
        work: printable(&trouble::work(state, session)),
    }
}

/// The line of trouble `found`, in its workspace's project.
fn trouble_row(state: &Snapshot, found: &trouble::Trouble) -> Row {
    let (name, colour) = project_of(state, found.workspace);
    Row {
        key: found.key,
        session: found.session,
        mark: match found.key {
            Key::Checks(..) => Mark::ChecksFailed,
            Key::Conflict(..) | Key::Session(_) => Mark::Conflict,
        },
        harness: String::new(),
        project: printable(&name),
        colour,
        title: printable(&found.title),
        doing: printable(&found.doing),
        since: String::new(),
        work: String::new(),
    }
}

/// The name of `workspace`'s project, and its place among the projects,
/// which picks its colour, as in the UI.
fn project_of(state: &Snapshot, workspace: muster_core::WorkspaceId) -> (String, usize) {
    state
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .and_then(|found| {
            state
                .projects
                .iter()
                .position(|project| project.id == found.project)
        })
        .map(|at| (state.projects[at].name.clone(), at))
        .unwrap_or_default()
}

/// What a status says, in a word or two.
fn describe(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Fresh => "idle",
        SessionStatus::Running => "working",
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        } => "wants permission",
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        } => "has a question",
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        } => "asks before it starts",
        SessionStatus::NeedsYou {
            reason: WaitReason::Action,
        } => "needs you",
        SessionStatus::Finished => "finished",
        SessionStatus::Exited { success: true } => "ended",
        SessionStatus::Exited { success: false } => "ended with an error",
    }
}

/// How long since `then`: `12s`, `4m`, `2h`, `3d`.
pub fn ago(then: Timestamp, now: Timestamp) -> String {
    let seconds = (now.as_millis() - then.as_millis()).max(0) / 1000;
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..86_400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// `text` on one line without control characters: a title can be an
/// issue's, which anyone can write (`PRD-07`).
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::settings::Preferences;
    use muster_core::{Activity, Project, ProjectId, Workspace, WorkspaceId, WorkspaceKind};

    use super::*;

    const NOW: i64 = 1_000_000;

    fn state() -> (Snapshot, WorkspaceId) {
        let projects = ["muster", "lamphaus"].map(|name| Project {
            id: ProjectId::new(),
            name: name.to_owned(),
            root: PathBuf::from("/src").join(name),
            created_at: Timestamp::from_millis(0),
        });
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: projects[1].id,
            kind: WorkspaceKind::Worktree {
                path: PathBuf::from("/w"),
                branch: "muster/work".to_owned(),
            },
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
        };
        let id = workspace.id;
        (
            Snapshot {
                projects: projects.to_vec(),
                workspaces: vec![workspace],
                ..Snapshot::default()
            },
            id,
        )
    }

    fn session(
        workspace: WorkspaceId,
        title: &str,
        status: SessionStatus,
        unseen: bool,
        since: i64,
    ) -> Session {
        Session {
            id: SessionId::new(),
            workspace,
            harness: "claude".to_owned(),
            title: title.to_owned(),
            status,
            unseen,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(since),
            activity: None,
            spend: None,
            tidy: None,
        }
    }

    fn board(state: &Snapshot) -> Board {
        Board::of(state, true, Timestamp::from_millis(NOW))
    }

    #[test]
    fn only_a_visible_time_s_next_change_wakes_the_notch_arc_13() {
        use std::time::Duration;
        let (mut state, work) = state();
        state
            .sessions
            .push(session(work, "busy", SessionStatus::Running, false, 0));
        for (millis, left) in [
            (0, 1000),
            (1234, 766),
            (60_001, 59_999),
            (3_600_001, 3_599_999),
            (86_400_001, 86_399_999),
        ] {
            let now = Timestamp::from_millis(millis);
            let board = Board::of(&state, true, now);
            assert_eq!(
                board.next_change(&state, now),
                Some(Duration::from_millis(left))
            );
        }
        let now = Timestamp::from_millis(10);
        state.looked_at = true;
        assert_eq!(Board::of(&state, true, now).next_change(&state, now), None);
        // A newly active line advances the next deadline, even when other
        // rows have already settled to minute labels.
        state.looked_at = false;
        state
            .sessions
            .push(session(work, "new", SessionStatus::Running, false, 60_000));
        let now = Timestamp::from_millis(60_001);
        assert_eq!(
            Board::of(&state, true, now).next_change(&state, now),
            Some(Duration::from_millis(999))
        );
    }

    #[test]
    fn it_shows_while_an_agent_works_or_waits_and_nobody_looks() {
        let (mut state, work) = state();
        // Nothing works: nothing shows.
        state
            .sessions
            .push(session(work, "idle", SessionStatus::Finished, false, 0));
        assert!(!board(&state).shown);

        state
            .sessions
            .push(session(work, "busy", SessionStatus::Running, false, 0));
        let shown = board(&state);
        assert!(shown.shown);
        assert_eq!((shown.working, shown.asking, shown.news), (1, 0, 0));

        // A Muster window looked at, the setting off, or no daemon: hidden.
        state.looked_at = true;
        assert!(!board(&state).shown);
        state.looked_at = false;
        state.preferences = Preferences {
            notch: false,
            file: PathBuf::new(),
            ..Preferences::default()
        };
        assert!(!board(&state).shown);
        state.preferences = Preferences::default();
        assert!(!Board::of(&state, false, Timestamp::from_millis(NOW)).shown);

        // News not yet seen keeps it shown with nothing working.
        state.sessions[1].status = SessionStatus::Finished;
        assert!(!board(&state).shown);
        state.sessions[1].unseen = true;
        let news = board(&state);
        assert!(news.shown);
        assert_eq!((news.working, news.asking, news.news), (0, 0, 1));
    }

    #[test]
    fn questions_come_first_then_news_then_work_each_oldest_first() {
        let (mut state, work) = state();
        let asking = SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        };
        state.sessions = vec![
            session(work, "working", SessionStatus::Running, false, 10),
            session(work, "new news", SessionStatus::Finished, true, 50),
            session(work, "old news", SessionStatus::Finished, true, 20),
            session(work, "asking", asking, false, 900_000),
        ];
        state.sessions[0].activity = Some(Activity {
            text: "editing view.rs".to_owned(),
            since: Timestamp::from_millis(10),
        });
        let board = board(&state);
        let titles: Vec<&str> = board.rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["asking", "old news", "new news", "working"]);
        let marks: Vec<Mark> = board.rows.iter().map(|row| row.mark).collect();
        assert_eq!(marks, [Mark::Asking, Mark::News, Mark::News, Mark::Working]);
        assert_eq!(board.rows[0].doing, "wants permission");
        assert_eq!(board.rows[0].since, "1m");
        assert_eq!(board.rows[3].doing, "editing view.rs");
        assert_eq!(board.rows[3].project, "lamphaus");
        assert_eq!(board.rows[3].colour, 1);
        assert_eq!((board.working, board.asking, board.news), (1, 1, 2));
        assert_eq!(board.rows[0].harness, "claude");
        assert_eq!(board.first_working(), Some(3));
    }

    #[test]
    fn a_new_question_is_told_from_one_asked_before() {
        let (mut state, work) = state();
        let asking = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        state
            .sessions
            .push(session(work, "first", asking, false, 10));
        let before = board(&state);
        assert!(!before.asks_anew(&before), "the same question");
        // While the notch was hidden, the person saw it in Muster.
        assert!(!before.asks_anew(&Board::hidden()));
        // Another asks: new. One stops asking: nothing new.
        state
            .sessions
            .push(session(work, "second", asking, false, 20));
        let after = board(&state);
        assert!(after.asks_anew(&before));
        let fresh = after.fresh(&before);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].title, "second");
        assert!(!before.asks_anew(&after));
        // News is no question, and a hidden notch draws no eye.
        let mut news = state.clone();
        news.sessions
            .push(session(work, "done", SessionStatus::Finished, true, 5));
        assert!(!board(&news).asks_anew(&after));
        state.looked_at = true;
        assert!(!board(&state).asks_anew(&before));
    }

    #[test]
    fn trouble_comes_after_the_questions_and_before_the_news() {
        use muster_core::clashes::{Clash, ClashFile};
        let (mut state, work) = state();
        let mut other = state.workspaces[0].clone();
        other.id = WorkspaceId::new();
        let clash = |with: WorkspaceId| Clash {
            with,
            files: 1,
            conflicts: 1,
            shown: vec![ClashFile {
                path: "auth/session.rs".to_owned(),
                conflict: true,
            }],
        };
        state.workspaces[0].clashes = vec![clash(other.id)];
        other.clashes = vec![clash(work)];
        state.workspaces.push(other);
        let asking = SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        };
        state.sessions = vec![
            session(work, "working", SessionStatus::Running, false, 10),
            session(work, "news", SessionStatus::Finished, true, 20),
            session(work, "asking", asking, false, 30),
        ];
        let calm = {
            let mut calm = state.clone();
            for workspace in &mut calm.workspaces {
                workspace.clashes.clear();
            }
            board(&calm)
        };
        let board = board(&state);
        let marks: Vec<Mark> = board.rows.iter().map(|row| row.mark).collect();
        assert_eq!(
            marks,
            [Mark::Asking, Mark::Conflict, Mark::News, Mark::Working]
        );
        assert_eq!(board.troubles, 1);
        assert_eq!(board.rows[1].doing, "would conflict in auth/session.rs");
        assert_eq!(board.rows[1].project, "lamphaus");
        // The conflict is new since a board without it: shown a moment.
        let fresh = board.fresh(&calm);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].mark, Mark::Conflict);
        assert!(!board.asks_anew(&calm), "no new question");
    }

    #[test]
    fn many_sessions_are_counted_past_the_rows() {
        let (mut state, work) = state();
        for at in 0..MOST_ROWS + 3 {
            let at = i64::try_from(at).unwrap();
            state
                .sessions
                .push(session(work, "w", SessionStatus::Running, false, at));
        }
        let board = board(&state);
        assert_eq!((board.rows.len(), board.more), (MOST_ROWS, 3));
    }

    #[test]
    fn a_title_cannot_reach_the_screen_with_its_control_characters() {
        let (mut state, work) = state();
        state.sessions.push(session(
            work,
            "fix\u{1b}[2J\nlogin",
            SessionStatus::Running,
            false,
            0,
        ));
        assert_eq!(board(&state).rows[0].title, "fix [2J login");
    }

    #[test]
    fn times_read_in_their_largest_unit() {
        let at = |seconds: i64| {
            ago(
                Timestamp::from_millis(0),
                Timestamp::from_millis(seconds * 1000),
            )
        };
        assert_eq!(
            [
                at(0),
                at(59),
                at(60),
                at(3599),
                at(3600),
                at(86_400),
                at(-5)
            ],
            ["0s", "59s", "1m", "59m", "1h", "1d", "0s"]
        );
    }
}
