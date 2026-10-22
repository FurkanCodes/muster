//! What a verb is about: the session a person names in a few letters, and
//! the project and workspace the folder they stand in belongs to.

use std::path::{Path, PathBuf};

use muster_core::protocol::Snapshot;
use muster_core::{Project, Session, SessionId, Workspace};

/// How many of a session's id's last letters name it in short.
const SHORT: usize = 6;

/// A session's id in short: the last letters of its ULID, which are its
/// random part (the first are the time it was made, shared by sessions
/// made close together).
pub(crate) fn short(id: SessionId) -> String {
    let ulid = id.ulid().to_string().to_lowercase();
    ulid[ulid.len() - SHORT..].to_owned()
}

/// The session `wanted` names: its id, the end of its id (four letters or
/// more), its name, or the branch of its worktree where one agent works
/// there. Refused, saying why, where none or several match.
pub(crate) fn find_session<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
) -> Result<&'a Session, String> {
    let lowered = wanted.to_lowercase();
    let by_id = |session: &&Session| {
        let ulid = session.id.ulid().to_string().to_lowercase();
        session.id.to_string() == wanted
            || ulid == lowered
            || (lowered.len() >= 4 && ulid.ends_with(&lowered))
    };
    let by_name = |session: &&Session| session.title.to_lowercase() == lowered;
    let by_branch = |session: &&Session| {
        session.kind().is_agent()
            && snapshot.workspaces.iter().any(|workspace| {
                workspace.id == session.workspace
                    && workspace.kind.in_git()
                    && workspace.kind.branch() == wanted
            })
    };
    let ways: [&dyn Fn(&&Session) -> bool; 3] = [&by_id, &by_name, &by_branch];
    for way in ways {
        let found: Vec<&Session> = snapshot.sessions.iter().filter(way).collect();
        match found.as_slice() {
            [] => {}
            [one] => return Ok(one),
            several => {
                let ids: Vec<String> = several.iter().map(|session| short(session.id)).collect();
                return Err(format!(
                    "{wanted:?} names {} sessions ({}); name one by its id",
                    several.len(),
                    ids.join(", ")
                ));
            }
        }
    }
    Err(format!(
        "no session is {wanted:?}: name one by its id, its name or its branch (`muster ls`)"
    ))
}

/// Where a command was run, as the daemon knows it.
#[derive(Debug)]
pub(crate) struct Here<'a> {
    pub(crate) project: &'a Project,
    /// The workspace the folder is in: the project's checkout or one of
    /// its worktrees; `None` in a project's folder Muster made no
    /// workspace for.
    pub(crate) workspace: Option<&'a Workspace>,
}

/// The project and workspace `folder` is in, the one nearest it where
/// several hold it (a worktree inside its project's folder).
pub(crate) fn here<'a>(snapshot: &'a Snapshot, folder: &Path) -> Option<Here<'a>> {
    let folder = canonical(folder);
    let workspace = snapshot
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace
                .kind
                .host_path()
                .is_some_and(|path| folder.starts_with(canonical(path)))
        })
        .max_by_key(|workspace| {
            workspace
                .kind
                .host_path()
                .map_or(0, |path| path.as_os_str().len())
        });
    let project = match workspace {
        Some(workspace) => snapshot
            .projects
            .iter()
            .find(|project| project.id == workspace.project)?,
        None => snapshot
            .projects
            .iter()
            .filter(|project| folder.starts_with(canonical(&project.root)))
            .max_by_key(|project| project.root.as_os_str().len())?,
    };
    Some(Here { project, workspace })
}

/// `path` with its links followed, as the daemon stores paths; as it is
/// where it cannot be.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::{ProjectId, SessionStatus, Timestamp, WorkspaceId, WorkspaceKind};

    fn project(root: &str) -> Project {
        Project {
            id: ProjectId::new(),
            name: "app".to_owned(),
            root: PathBuf::from(root),
            created_at: Timestamp::from_millis(0),
        }
    }

    fn workspace(project: &Project, kind: WorkspaceKind) -> Workspace {
        Workspace {
            id: WorkspaceId::new(),
            project: project.id,
            kind,
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
            ticket: None,
        }
    }

    fn session(workspace: &Workspace, harness: &str, title: &str) -> Session {
        Session {
            id: SessionId::new(),
            workspace: workspace.id,
            harness: harness.to_owned(),
            title: title.to_owned(),
            status: SessionStatus::Running,
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
        }
    }

    fn world() -> Snapshot {
        let app = project("/nowhere/app");
        let checkout = workspace(
            &app,
            WorkspaceKind::Checkout {
                path: PathBuf::from("/nowhere/app"),
                branch: "main".to_owned(),
            },
        );
        let worktree = workspace(
            &app,
            WorkspaceKind::Worktree {
                path: PathBuf::from("/nowhere/app/.trees/fix"),
                branch: "muster/fix-login".to_owned(),
            },
        );
        let sessions = vec![
            session(&checkout, "claude", "Tidy the README"),
            session(&worktree, "claude", "Fix login"),
            session(&worktree, "shell", "zsh"),
        ];
        Snapshot {
            projects: vec![app],
            workspaces: vec![checkout, worktree],
            sessions,
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_session_is_named_by_its_id_its_end_its_name_or_its_branch() {
        let world = world();
        let fix = &world.sessions[1];
        for wanted in [
            fix.id.to_string(),
            short(fix.id),
            short(fix.id).to_uppercase(),
            "fix LOGIN".to_owned(),
            "muster/fix-login".to_owned(),
        ] {
            assert_eq!(
                find_session(&world, &wanted).map(|s| s.id),
                Ok(fix.id),
                "{wanted}"
            );
        }
    }

    #[test]
    fn a_name_that_matches_nothing_or_several_says_so() {
        let mut world = world();
        assert!(
            find_session(&world, "nothing")
                .unwrap_err()
                .contains("muster ls")
        );
        let worktree = world.workspaces[1].clone();
        world
            .sessions
            .push(session(&worktree, "codex", "Fix login"));
        let several = find_session(&world, "fix login").unwrap_err();
        assert!(several.contains("2 sessions"), "{several}");
    }

    #[test]
    fn the_folder_says_the_project_and_the_nearest_workspace() {
        let world = world();
        let at_root = here(&world, Path::new("/nowhere/app/src")).unwrap();
        assert_eq!(
            at_root.workspace.map(|w| w.id),
            Some(world.workspaces[0].id)
        );
        let in_tree = here(&world, Path::new("/nowhere/app/.trees/fix/src")).unwrap();
        assert_eq!(
            in_tree.workspace.map(|w| w.id),
            Some(world.workspaces[1].id)
        );
        assert_eq!(in_tree.project.id, world.projects[0].id);
        assert!(here(&world, Path::new("/elsewhere")).is_none());
    }
}
