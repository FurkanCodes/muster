//! The notch's copy of the daemon's state, kept current from the snapshot
//! and the deltas after it, as every client's is.

use muster_core::protocol::{Delta, Snapshot};

/// Applies one change to `state`.
pub(crate) fn apply(state: &mut Snapshot, delta: Delta) {
    match delta {
        Delta::ProjectUpserted(project) => upsert(&mut state.projects, project, |p| p.id),
        Delta::ProjectRemoved { id } => {
            let gone: Vec<_> = state
                .workspaces
                .iter()
                .filter(|workspace| workspace.project == id)
                .map(|workspace| workspace.id)
                .collect();
            state
                .sessions
                .retain(|session| !gone.contains(&session.workspace));
            state.workspaces.retain(|workspace| workspace.project != id);
            state.projects.retain(|project| project.id != id);
        }
        Delta::WorkspaceUpserted(workspace) => {
            upsert(&mut state.workspaces, workspace, |w| w.id);
        }
        Delta::WorkspaceRemoved { id } => {
            state.workspaces.retain(|workspace| workspace.id != id);
            state.sessions.retain(|session| session.workspace != id);
        }
        Delta::SessionUpserted(session) => upsert(&mut state.sessions, session, |s| s.id),
        Delta::SessionRemoved { id } => state.sessions.retain(|session| session.id != id),
        Delta::SessionStatus {
            session,
            status,
            unseen,
            since,
        } => {
            if let Some(stored) = state.sessions.iter_mut().find(|s| s.id == session) {
                stored.status = status;
                stored.unseen = unseen;
                stored.status_since = since;
            }
        }
        Delta::SessionActivity { session, activity } => {
            if let Some(stored) = state.sessions.iter_mut().find(|s| s.id == session) {
                stored.activity = activity;
            }
        }
        Delta::LookedAt { looked_at } => state.looked_at = looked_at,
        Delta::Preferences(preferences) => state.preferences = preferences,
        Delta::Notifier { ready } => state.notifier = ready,
        Delta::PluginsChanged(plugins) => state.plugins = plugins,
        // Nothing the notch shows.
        Delta::Resources(_) | Delta::RunUpserted(_) => {}
    }
}

fn upsert<T, K: PartialEq>(items: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) {
    match items
        .iter_mut()
        .find(|existing| key(existing) == key(&item))
    {
        Some(existing) => *existing = item,
        None => items.push(item),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::settings::Preferences;
    use muster_core::{
        Project, ProjectId, Session, SessionId, SessionStatus, Timestamp, Workspace, WorkspaceId,
        WorkspaceKind,
    };

    use super::*;

    #[test]
    fn deltas_keep_the_copy_current() {
        let project = Project {
            id: ProjectId::new(),
            name: "app".to_owned(),
            root: PathBuf::from("/app"),
            created_at: Timestamp::from_millis(0),
        };
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: project.id,
            kind: WorkspaceKind::Checkout {
                path: PathBuf::from("/app"),
                branch: "main".to_owned(),
            },
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
            ticket: None,
        };
        let session = Session {
            id: SessionId::new(),
            workspace: workspace.id,
            harness: "claude".to_owned(),
            title: "t".to_owned(),
            status: SessionStatus::Fresh,
            unseen: false,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
            started_by: None,
            agent: None,
        };
        let mut state = Snapshot::default();
        apply(&mut state, Delta::ProjectUpserted(project.clone()));
        apply(&mut state, Delta::WorkspaceUpserted(workspace.clone()));
        apply(&mut state, Delta::SessionUpserted(session.clone()));
        apply(
            &mut state,
            Delta::SessionStatus {
                session: session.id,
                status: SessionStatus::Running,
                unseen: true,
                since: Timestamp::from_millis(7),
            },
        );
        assert_eq!(state.sessions[0].status, SessionStatus::Running);
        assert!(state.sessions[0].unseen);
        apply(&mut state, Delta::LookedAt { looked_at: true });
        assert!(state.looked_at);
        apply(
            &mut state,
            Delta::Preferences(Preferences {
                notch: false,
                file: PathBuf::new(),
                ..Preferences::default()
            }),
        );
        assert!(!state.preferences.notch);
        // A project leaves with its workspaces and their sessions.
        apply(&mut state, Delta::ProjectRemoved { id: project.id });
        assert!(state.sessions.is_empty() && state.workspaces.is_empty());
    }
}
