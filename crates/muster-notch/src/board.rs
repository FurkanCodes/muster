//! What the notch shows, worked out from the daemon's state alone: no IO,
//! no AppKit, so it is tested anywhere (`ARC-22`).
//!
//! The notch shows only while the person looks at no Muster window and an
//! agent works or waits on them (a question asked, or news not yet seen),
//! and only while Muster's own settings have it on. Level with the notch,
//! the counts: what waits on the person, and what works; pointed at, the
//! sessions that wait on the person, oldest first, then those working.

use muster_core::protocol::Snapshot;
use muster_core::{Session, SessionId, SessionStatus, Timestamp, WaitReason};

/// The most sessions listed; the rest are counted.
pub const MOST_ROWS: usize = 8;

/// What the notch shows now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    /// Whether it shows at all.
    pub shown: bool,
    /// Agents working.
    pub working: usize,
    /// Sessions waiting on the person: asking, or with news not yet seen.
    pub waiting: usize,
    /// Those waiting, then those working, at most [`MOST_ROWS`].
    pub rows: Vec<Row>,
    /// Sessions waiting or working beyond the rows.
    pub more: usize,
}

/// One session listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub session: SessionId,
    pub mark: Mark,
    pub project: String,
    /// The project's place among the projects, which picks its colour, as
    /// in the UI.
    pub colour: usize,
    pub title: String,
    /// What it is doing: its activity while it works or asks, else what
    /// waits.
    pub doing: String,
    /// How long it has been so: `12s`, `4m`, `2h`.
    pub since: String,
}

/// What a listed session's mark says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// It asks the person something.
    Asking,
    /// It has news the person has not seen: finished, or ended.
    News,
    /// It works.
    Working,
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
        let listed: Vec<&Session> = waiting.iter().chain(&working).copied().collect();
        Self {
            shown: connected && busy && !state.looked_at && state.preferences.notch,
            working: working.len(),
            waiting: waiting.len(),
            rows: listed
                .iter()
                .take(MOST_ROWS)
                .map(|session| row(state, session, now))
                .collect(),
            more: listed.len().saturating_sub(MOST_ROWS),
        }
    }

    /// Nothing to show.
    pub fn hidden() -> Self {
        Self {
            shown: false,
            working: 0,
            waiting: 0,
            rows: Vec::new(),
            more: 0,
        }
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
    let project = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == session.workspace)
        .map(|workspace| workspace.project);
    let (name, colour) = project
        .and_then(|project| {
            state
                .projects
                .iter()
                .position(|found| found.id == project)
                .map(|at| (state.projects[at].name.clone(), at))
        })
        .unwrap_or_default();
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
        session: session.id,
        mark,
        project: printable(&name),
        colour,
        title: printable(&session.title),
        doing: printable(&doing),
        since: ago(session.status_since, now),
    }
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
        }
    }

    fn board(state: &Snapshot) -> Board {
        Board::of(state, true, Timestamp::from_millis(NOW))
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
        assert_eq!((shown.working, shown.waiting), (1, 0));

        // A Muster window looked at, the setting off, or no daemon: hidden.
        state.looked_at = true;
        assert!(!board(&state).shown);
        state.looked_at = false;
        state.preferences = Preferences {
            notch: false,
            file: PathBuf::new(),
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
        assert_eq!((news.working, news.waiting), (0, 1));
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
        assert_eq!((board.working, board.waiting), (1, 3));
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
