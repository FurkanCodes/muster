//! What in the work needs the person besides a question, from the daemon's
//! state alone (`ARC-22`): two workspaces whose work would conflict were
//! both merged, and an open pull request whose checks failed; and what
//! each session's work comes to, for its line. Pure, so it is tested
//! anywhere.

use muster_core::protocol::Snapshot;
use muster_core::text::one_line;
use muster_core::{Checks, PullRequestState, Session, SessionId, Workspace, WorkspaceId};

/// Something that needs the person, as the board keeps it: so a new one is
/// told from one there before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A session: asking the person, or listed.
    Session(SessionId),
    /// Two workspaces whose work would conflict, the lesser first.
    Conflict(WorkspaceId, WorkspaceId),
    /// A pull request, by its workspace and number, whose checks failed.
    Checks(WorkspaceId, u64),
}

/// A trouble found: what it is, where, and what to say of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trouble {
    pub key: Key,
    /// The session a click shows: the newest in the workspace.
    pub session: SessionId,
    pub workspace: WorkspaceId,
    pub title: String,
    pub doing: String,
}

/// Every trouble in `state`: the conflicts, then the failed checks, each
/// in the order of the workspaces.
pub fn troubles(state: &Snapshot) -> Vec<Trouble> {
    let mut found = Vec::new();
    for workspace in &state.workspaces {
        for clash in workspace
            .clashes
            .iter()
            .filter(|clash| clash.would_conflict())
        {
            // Each pair once, from the workspace that comes first.
            let Some(other) = state
                .workspaces
                .iter()
                .find(|other| other.id == clash.with)
                .filter(|other| workspace.id < other.id)
            else {
                continue;
            };
            let Some(session) = newest(state, workspace.id).or_else(|| newest(state, other.id))
            else {
                continue;
            };
            let first = clash
                .shown
                .iter()
                .find(|file| file.conflict)
                .map_or_else(String::new, |file| one_line(&file.path));
            let more = clash.conflicts.saturating_sub(1);
            let doing = match (first.is_empty(), more) {
                (true, _) => "would conflict".to_owned(),
                (false, 0) => format!("would conflict in {first}"),
                (false, more) => format!("would conflict in {first} and {more} more"),
            };
            found.push(Trouble {
                key: Key::Conflict(workspace.id, other.id),
                session: session.id,
                workspace: workspace.id,
                title: format!("{} × {}", branch(workspace), branch(other)),
                doing,
            });
        }
    }
    for workspace in &state.workspaces {
        let Some(pull) = workspace
            .pull_request
            .as_ref()
            .filter(|pull| pull.state.is_open() && pull.checks == Some(Checks::Failing))
        else {
            continue;
        };
        let Some(session) = newest(state, workspace.id) else {
            continue;
        };
        found.push(Trouble {
            key: Key::Checks(workspace.id, pull.number),
            session: session.id,
            workspace: workspace.id,
            title: format!("#{} checks failed", pull.number),
            doing: branch(workspace),
        });
    }
    found
}

/// What the work of `session`'s workspace comes to, for its line: its
/// changes, then its pull request and its checks' mark: `+120 −30  #88 ✗`.
pub fn work(state: &Snapshot, session: &Session) -> String {
    let Some(workspace) = state
        .workspaces
        .iter()
        .find(|workspace| session.is_in(workspace.id))
    else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(changes) = workspace.changes.filter(|changes| changes.files > 0) {
        parts.push(changes.words());
    }
    if let Some(pull) = &workspace.pull_request {
        let checks = pull
            .checks
            .filter(|_| pull.state.is_open())
            .map(|checks| match checks {
                Checks::Passing => " ✓",
                Checks::Failing => " ✗",
                Checks::Pending => " ◌",
            })
            .unwrap_or_default();
        match pull.state {
            PullRequestState::Open => parts.push(format!("#{}{checks}", pull.number)),
            PullRequestState::Draft => parts.push(format!("#{} draft{checks}", pull.number)),
            PullRequestState::Merged => parts.push(format!("#{} merged", pull.number)),
            PullRequestState::Closed => {}
        }
    }
    parts.join("  ")
}

/// The newest session in `workspace`.
fn newest(state: &Snapshot, workspace: WorkspaceId) -> Option<&Session> {
    state
        .sessions
        .iter()
        .filter(|session| session.is_in(workspace))
        .max_by_key(|session| session.created_at)
}

/// A workspace's branch on one line: a branch name is the person's, or an
/// agent's, and is drawn as text only.
fn branch(workspace: &Workspace) -> String {
    one_line(workspace.kind.branch())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::clashes::{Clash, ClashFile};
    use muster_core::{
        ChangeSummary, Project, ProjectId, PullRequestStatus, SessionStatus, Timestamp,
        WorkspaceKind,
    };

    use super::*;

    fn workspace(project: ProjectId, branch: &str) -> Workspace {
        Workspace {
            id: WorkspaceId::new(),
            project,
            kind: WorkspaceKind::Worktree {
                path: PathBuf::from("/w").join(branch),
                branch: branch.to_owned(),
            },
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
            ticket: None,
        }
    }

    fn session(workspace: WorkspaceId, created: i64) -> Session {
        Session {
            id: SessionId::new(),
            workspace: Some(workspace),
            harness: "claude".to_owned(),
            title: "t".to_owned(),
            status: SessionStatus::Running,
            unseen: false,
            resume_id: None,
            created_at: Timestamp::from_millis(created),
            ended_at: None,
            status_since: Timestamp::from_millis(created),
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
            started_by: None,
            agent: None,
        }
    }

    fn pull(number: u64, state: PullRequestState, checks: Option<Checks>) -> PullRequestStatus {
        PullRequestStatus {
            number,
            url: format!("https://example.com/pull/{number}"),
            state,
            checks,
            review: None,
        }
    }

    fn state() -> Snapshot {
        let project = Project {
            id: ProjectId::new(),
            name: "app".to_owned(),
            root: PathBuf::from("/app"),
            created_at: Timestamp::from_millis(0),
        };
        let (mut login, mut notch) = (
            workspace(project.id, "fix-login"),
            workspace(project.id, "notch"),
        );
        // Each says the other; the pair is told once.
        let clash = |with: WorkspaceId| Clash {
            with,
            files: 3,
            conflicts: 2,
            shown: vec![
                ClashFile {
                    path: "src/tidy.rs".to_owned(),
                    conflict: false,
                },
                ClashFile {
                    path: "auth/session.rs".to_owned(),
                    conflict: true,
                },
            ],
        };
        login.clashes = vec![clash(notch.id)];
        notch.clashes = vec![clash(login.id)];
        Snapshot {
            projects: vec![project],
            sessions: vec![
                session(login.id, 1),
                session(login.id, 5),
                session(notch.id, 2),
            ],
            workspaces: vec![login, notch],
            ..Snapshot::default()
        }
    }

    #[test]
    fn two_workspaces_that_would_conflict_are_one_trouble_shown_by_the_newest_session() {
        let state = state();
        let found = troubles(&state);
        assert_eq!(found.len(), 1, "{found:?}");
        let conflict = &found[0];
        let (first, second) = (&state.workspaces[0], &state.workspaces[1]);
        let (lesser, greater) = if first.id < second.id {
            (first, second)
        } else {
            (second, first)
        };
        assert_eq!(conflict.key, Key::Conflict(lesser.id, greater.id));
        assert_eq!(
            conflict.title,
            format!("{} × {}", lesser.kind.branch(), greater.kind.branch())
        );
        assert_eq!(
            conflict.doing,
            "would conflict in auth/session.rs and 1 more"
        );
        let newest = state
            .sessions
            .iter()
            .filter(|session| session.is_in(lesser.id))
            .max_by_key(|session| session.created_at)
            .unwrap();
        assert_eq!(conflict.session, newest.id);
    }

    #[test]
    fn an_overlap_alone_is_no_trouble() {
        let mut state = state();
        for workspace in &mut state.workspaces {
            for clash in &mut workspace.clashes {
                clash.conflicts = 0;
            }
        }
        assert!(troubles(&state).is_empty());
    }

    #[test]
    fn failed_checks_on_an_open_pull_request_are_trouble_and_on_a_merged_one_not() {
        let mut state = state();
        for workspace in &mut state.workspaces {
            workspace.clashes.clear();
        }
        state.workspaces[0].pull_request =
            Some(pull(88, PullRequestState::Open, Some(Checks::Failing)));
        state.workspaces[1].pull_request =
            Some(pull(9, PullRequestState::Merged, Some(Checks::Failing)));
        let found = troubles(&state);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, Key::Checks(state.workspaces[0].id, 88));
        assert_eq!(found[0].title, "#88 checks failed");
        assert_eq!(found[0].doing, "fix-login");
    }

    #[test]
    fn a_line_s_work_is_its_changes_and_its_pull_request() {
        let mut state = state();
        let session = state.sessions[0].clone();
        assert_eq!(work(&state, &session), "");
        state.workspaces[0].changes = Some(ChangeSummary {
            files: 4,
            added: 120,
            removed: 30,
        });
        state.workspaces[0].pull_request =
            Some(pull(88, PullRequestState::Open, Some(Checks::Failing)));
        assert_eq!(work(&state, &session), "+120 −30  #88 ✗");
        state.workspaces[0].pull_request =
            Some(pull(88, PullRequestState::Draft, Some(Checks::Pending)));
        assert_eq!(work(&state, &session), "+120 −30  #88 draft ◌");
        state.workspaces[0].pull_request =
            Some(pull(88, PullRequestState::Merged, Some(Checks::Failing)));
        assert_eq!(work(&state, &session), "+120 −30  #88 merged");
        state.workspaces[0].pull_request = Some(pull(88, PullRequestState::Closed, None));
        assert_eq!(work(&state, &session), "+120 −30");
    }
}
