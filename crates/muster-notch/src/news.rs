//! What newly needs the person, for Muster's helper to notify them of
//! (`PRODUCT.md` §6), as the UI tells it (`muster_core::news`): a session
//! that comes to need them, once; a finish only once it has stayed one a
//! moment, as an agent that ends a turn often starts another at once; and
//! never what a snapshot shows, which is no news; and only the kinds the
//! person chose, each with its sound (`muster_core::telling`). Pure but
//! for the time it is handed.

use std::collections::HashSet;
use std::time::Duration;

use muster_core::SessionId;
use muster_core::news::{Notice, needs_attention, notice, told_at_once};
use muster_core::protocol::Snapshot;
use muster_core::telling::{Kind, Sound};
use tokio::time::Instant;

/// How long a finish is held before it is told: as in the UI.
pub(crate) const HOLD: Duration = Duration::from_secs(2);

/// What needed the person when last looked, and the finishes held.
#[derive(Debug, Default)]
pub(crate) struct News {
    needing: HashSet<SessionId>,
    held: Vec<(SessionId, Instant)>,
}

impl News {
    /// A snapshot: what it shows is no news.
    pub(crate) fn known(&mut self, state: &Snapshot) {
        self.needing = needing(state);
        self.held.clear();
    }

    /// The state changed at `now`: the sessions that newly need the person
    /// and are told at once; a finish is held.
    pub(crate) fn changed(&mut self, state: &Snapshot, now: Instant) -> Vec<SessionId> {
        let needing = needing(state);
        let mut told = Vec::new();
        for session in needing.difference(&self.needing) {
            let Some(stored) = state.sessions.iter().find(|stored| stored.id == *session) else {
                continue;
            };
            if told_at_once(stored) {
                told.push(*session);
            } else {
                self.held.retain(|(held, _)| held != session);
                self.held.push((*session, now));
            }
        }
        self.needing = needing;
        told.sort();
        told
    }

    /// The finishes held long enough by `now`, still finished and unseen:
    /// those at work again meanwhile, or gone, are dropped.
    pub(crate) fn due(&mut self, state: &Snapshot, now: Instant) -> Vec<SessionId> {
        let (due, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(_, at)| now.duration_since(*at) >= HOLD);
        self.held = held;
        due.into_iter()
            .map(|(session, _)| session)
            .filter(|session| {
                state.sessions.iter().any(|stored| {
                    stored.id == *session
                        && stored.status == muster_core::SessionStatus::Finished
                        && needs_attention(stored)
                })
            })
            .collect()
    }

    /// Whether a finish is held, waiting to be told.
    pub(crate) fn holding(&self) -> bool {
        !self.held.is_empty()
    }
}

/// A notification as posted: of `session`, laid out as `notice`, with
/// `sound`, kept with the others of its project (`group`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Posted {
    pub(crate) session: SessionId,
    pub(crate) notice: Notice,
    pub(crate) sound: Sound,
    pub(crate) group: String,
}

/// The notifications of `told` the person chose to be told of, as
/// `state` says each is now.
pub(crate) fn posts(state: &Snapshot, told: &[SessionId]) -> Vec<Posted> {
    told.iter()
        .filter_map(|session| {
            let stored = state.sessions.iter().find(|stored| stored.id == *session)?;
            let tell = state.preferences.telling.of(Kind::of(stored));
            if !tell.on {
                return None;
            }
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == stored.workspace);
            let project = workspace.and_then(|workspace| {
                state
                    .projects
                    .iter()
                    .find(|project| project.id == workspace.project)
            });
            let branch = workspace.map(|workspace| workspace.kind.branch());
            Some(Posted {
                session: *session,
                notice: notice(stored, project.map(|found| found.name.as_str()), branch),
                sound: tell.sound.clone(),
                group: project.map_or_else(|| "muster".to_owned(), |found| found.id.to_string()),
            })
        })
        .collect()
}

fn needing(state: &Snapshot) -> HashSet<SessionId> {
    state
        .sessions
        .iter()
        .filter(|session| needs_attention(session))
        .map(|session| session.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use muster_core::{Session, SessionStatus, Timestamp, WaitReason, WorkspaceId};

    use super::*;

    fn session(status: SessionStatus, unseen: bool) -> Session {
        Session {
            id: SessionId::new(),
            workspace: WorkspaceId::new(),
            harness: "claude".to_owned(),
            title: "Fix the login".to_owned(),
            status,
            unseen,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
        }
    }

    fn state(sessions: Vec<Session>) -> Snapshot {
        Snapshot {
            sessions,
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_question_is_told_at_once_and_once_and_a_snapshot_never() {
        let start = Instant::now();
        let mut news = News::default();
        let asking = session(
            SessionStatus::NeedsYou {
                reason: WaitReason::Question,
            },
            false,
        );
        news.known(&state(vec![asking.clone()]));
        assert!(news.changed(&state(vec![asking.clone()]), start).is_empty());

        let mut working = asking.clone();
        working.status = SessionStatus::Running;
        assert!(news.changed(&state(vec![working]), start).is_empty());
        assert_eq!(
            news.changed(&state(vec![asking.clone()]), start),
            [asking.id]
        );
        assert!(news.changed(&state(vec![asking]), start).is_empty(), "once");
    }

    #[test]
    fn a_finish_is_told_once_it_has_stayed_one_two_seconds() {
        let start = Instant::now();
        let mut news = News::default();
        let finished = session(SessionStatus::Finished, true);
        assert!(
            news.changed(&state(vec![finished.clone()]), start)
                .is_empty()
        );
        assert!(news.holding());
        let now = state(vec![finished.clone()]);
        assert!(news.due(&now, start + Duration::from_secs(1)).is_empty());
        assert_eq!(news.due(&now, start + HOLD), [finished.id]);
        assert!(!news.holding());

        // At work again within the hold: never told.
        let again = session(SessionStatus::Finished, true);
        news.changed(&state(vec![again.clone()]), start);
        let mut working = again;
        working.status = SessionStatus::Running;
        working.unseen = false;
        assert!(news.due(&state(vec![working]), start + HOLD).is_empty());
    }

    #[test]
    fn only_the_kinds_chosen_are_posted_each_with_its_sound_by_project() {
        use muster_core::telling::{Tell, Telling};
        use muster_core::{Project, ProjectId, Workspace, WorkspaceKind};
        let project = Project {
            id: ProjectId::new(),
            name: "app".to_owned(),
            root: "/src/app".into(),
            created_at: Timestamp::from_millis(0),
        };
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: project.id,
            kind: WorkspaceKind::Worktree {
                path: "/w".into(),
                branch: "muster/login".to_owned(),
            },
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
        };
        let mut asking = session(
            SessionStatus::NeedsYou {
                reason: WaitReason::Question,
            },
            false,
        );
        asking.workspace = workspace.id;
        let finished = session(SessionStatus::Finished, true);
        let mut state = state(vec![asking.clone(), finished.clone()]);
        state.projects = vec![project.clone()];
        state.workspaces = vec![workspace];
        state.preferences.telling = Telling::default()
            .with(
                Kind::Question,
                Tell {
                    on: true,
                    sound: Sound::Named("Glass".to_owned()),
                },
            )
            .with(
                Kind::Finish,
                Tell {
                    on: false,
                    sound: Sound::Default,
                },
            );
        let posted = posts(&state, &[asking.id, finished.id]);
        assert_eq!(
            posted,
            [Posted {
                session: asking.id,
                notice: Notice {
                    title: "app · muster/login".to_owned(),
                    subtitle: "Fix the login".to_owned(),
                    body: "Has a question for you".to_owned(),
                },
                sound: Sound::Named("Glass".to_owned()),
                group: project.id.to_string(),
            }]
        );
    }
}
