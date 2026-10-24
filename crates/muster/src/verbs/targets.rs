//! What a verb is about: the session a person names in a few letters, and
//! the project and workspace the folder they stand in belongs to.
//!
//! A workspace is a worktree, or a project's own checkout (`PRODUCT.md`
//! §2), named as `muster list workspaces` shows it. Its sessions are numbered
//! oldest first, so `2` is the second session of the workspace the person
//! is in (the one of the Muster terminal they type in, else the one of the
//! folder they stand in), and `fix-login/2` another workspace's.

use std::path::{Path, PathBuf};

use muster_core::protocol::Snapshot;
use muster_core::{Project, ProjectId, Session, SessionId, Workspace, WorkspaceId};

/// How many of a session's id's last letters name it in short.
const SHORT: usize = 6;

/// A session's id in short: the last letters of its ULID, which are its
/// random part (the first are the time it was made, shared by sessions
/// made close together).
pub(crate) fn short(id: SessionId) -> String {
    let ulid = id.ulid().to_string().to_lowercase();
    ulid[ulid.len() - SHORT..].to_owned()
}

/// The session `wanted` names: its number in the workspace the person is
/// in, a workspace's name and a number (`fix-login/2`), its id, the end of
/// its id (four letters or more), its name, or the branch of its worktree
/// where one agent works there. Refused, saying why, where none or several
/// match.
pub(crate) fn find_session<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
) -> Result<&'a Session, String> {
    let folder = std::env::current_dir().unwrap_or_default();
    let here = current_workspace(snapshot, super::started_by(), &folder).map(|w| w.id);
    find_session_in(snapshot, wanted, here)
}

/// [`find_session`], the person in workspace `here`.
pub(crate) fn find_session_in<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
    here: Option<WorkspaceId>,
) -> Result<&'a Session, String> {
    if let Some(found) = by_number(snapshot, wanted, here)? {
        return Ok(found);
    }
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
        "no session is {wanted:?}: name one by its id, its name or its branch (`muster list`)"
    ))
}

/// The session numbered in `wanted`: `2` in workspace `here`, or
/// `fix-login/2`; `None` where `wanted` is no number.
fn by_number<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
    here: Option<WorkspaceId>,
) -> Result<Option<&'a Session>, String> {
    let number = |text: &str| {
        (!text.is_empty() && text.len() <= 3 && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<usize>().ok())
            .flatten()
    };
    let (workspace, n) = if let Some(n) = number(wanted) {
        let Some(here) = here else {
            return Err(format!(
                "{wanted} is a session's number in the workspace you are in, and you are in \
                 none: write the workspace's name first (`fix-login/{wanted}`)"
            ));
        };
        (here, n)
    } else if let Some((name, n)) = wanted.rsplit_once('/').and_then(|(name, n)| {
        let n = number(n)?;
        Some((name, n))
    }) {
        match find_workspace(snapshot, name, None) {
            Ok(workspace) => (workspace.id, n),
            // A branch with a number after its last `/` names an agent.
            Err(_) => return Ok(None),
        }
    } else {
        return Ok(None);
    };
    let sessions = numbered(snapshot, workspace);
    match n.checked_sub(1).and_then(|i| sessions.get(i)) {
        Some(found) => Ok(Some(found)),
        None => Err(format!(
            "{} has {} (`muster list`)",
            workspace_label(snapshot, workspace),
            crate::commands::count(sessions.len(), "session")
        )),
    }
}

/// The sessions of `workspace` in the order they are numbered, from 1:
/// oldest first. A plugin's panel is none: it stands by its chat.
pub(crate) fn numbered(snapshot: &Snapshot, workspace: WorkspaceId) -> Vec<&Session> {
    let mut sessions: Vec<&Session> = snapshot
        .sessions
        .iter()
        .filter(|session| session.workspace == workspace && !session.kind().is_panel())
        .collect();
    sessions.sort_by_key(|session| (session.created_at, session.id));
    sessions
}

/// A session's number in its workspace, from 1.
pub(crate) fn number_of(snapshot: &Snapshot, session: &Session) -> Option<usize> {
    numbered(snapshot, session.workspace)
        .iter()
        .position(|found| found.id == session.id)
        .map(|i| i + 1)
}

/// A workspace's name, as `muster list workspaces` shows it and the verbs
/// take it: a worktree's branch, without the `muster/` Muster puts first;
/// a project's own checkout, or its folder, by the project's name.
pub(crate) fn workspace_name(snapshot: &Snapshot, workspace: &Workspace) -> String {
    if workspace.kind.is_worktree() {
        let branch = workspace.kind.branch();
        return branch.strip_prefix("muster/").unwrap_or(branch).to_owned();
    }
    snapshot
        .projects
        .iter()
        .find(|project| project.id == workspace.project)
        .map_or_else(|| workspace.id.to_string(), |project| project.name.clone())
}

/// [`workspace_name`] by the workspace's id, for a message.
fn workspace_label(snapshot: &Snapshot, workspace: WorkspaceId) -> String {
    snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .map_or_else(
            || workspace.to_string(),
            |found| workspace_name(snapshot, found),
        )
}

/// The workspace `wanted` names: its id, its name, its branch, or its
/// folder's name; where several projects have one of that name, the one
/// in `prefer`. Refused, saying why, where none or several match.
pub(crate) fn find_workspace<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
    prefer: Option<ProjectId>,
) -> Result<&'a Workspace, String> {
    let matches = |workspace: &&Workspace| {
        workspace.id.to_string() == wanted
            || workspace_name(snapshot, workspace) == wanted
            || (workspace.kind.in_git() && workspace.kind.branch() == wanted)
            || workspace
                .kind
                .host_path()
                .and_then(Path::file_name)
                .is_some_and(|name| name == wanted)
    };
    let found: Vec<&Workspace> = snapshot.workspaces.iter().filter(matches).collect();
    let found = match (found.as_slice(), prefer) {
        ([_, _, ..], Some(project)) => {
            let in_project: Vec<&Workspace> = found
                .iter()
                .copied()
                .filter(|workspace| workspace.project == project)
                .collect();
            if in_project.is_empty() {
                found
            } else {
                in_project
            }
        }
        _ => found,
    };
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no workspace is {wanted:?}: name one as `muster list workspaces` shows it"
        )),
        several => Err(format!(
            "{wanted:?} names {} workspaces; name one by its branch or its id",
            several.len()
        )),
    }
}

/// The workspace the person is in: that of the Muster session they type
/// in (`inside`, `MUSTER_SESSION`), else the one of the folder they stand
/// in, else the checkout of the project it is in.
pub(crate) fn current_workspace<'a>(
    snapshot: &'a Snapshot,
    inside: Option<SessionId>,
    folder: &Path,
) -> Option<&'a Workspace> {
    let of_session = inside.and_then(|inside| {
        let session = snapshot.sessions.iter().find(|found| found.id == inside)?;
        snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == session.workspace)
    });
    of_session.or_else(|| {
        let here = here(snapshot, folder)?;
        here.workspace
            .or_else(|| checkout_of(snapshot, here.project.id))
    })
}

/// A project's own checkout, or its folder.
pub(crate) fn checkout_of(snapshot: &Snapshot, project: ProjectId) -> Option<&Workspace> {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == project && workspace.kind.is_checkout())
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
    // This machine's folder is none of another's (`--on`).
    if crate::ssh::remote().is_some() {
        return None;
    }
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
            agent: None,
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
                .contains("muster list")
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

    #[test]
    fn a_session_is_numbered_in_its_workspace_prd_6b() {
        let mut world = world();
        world.sessions[2].created_at = Timestamp::from_millis(1);
        let tree = world.workspaces[1].id;
        let (fix, shell) = (world.sessions[1].id, world.sessions[2].id);
        let found = |wanted: &str, here| find_session_in(&world, wanted, here).map(|s| s.id);
        assert_eq!(found("1", Some(tree)), Ok(fix));
        assert_eq!(found("2", Some(tree)), Ok(shell));
        assert_eq!(found("fix-login/2", None), Ok(shell));
        assert_eq!(found("muster/fix-login/1", None), Ok(fix));
        assert_eq!(found("app/1", Some(tree)), Ok(world.sessions[0].id));
        assert!(found("3", Some(tree)).unwrap_err().contains("2 sessions"));
        assert!(found("1", None).unwrap_err().contains("fix-login/1"));
    }

    #[test]
    fn a_workspace_is_named_by_its_branch_without_muster_s_prefix_or_its_project() {
        let world = world();
        let (checkout, tree) = (&world.workspaces[0], &world.workspaces[1]);
        assert_eq!(workspace_name(&world, tree), "fix-login");
        assert_eq!(workspace_name(&world, checkout), "app");
        for wanted in ["fix-login", "muster/fix-login", "fix"] {
            assert_eq!(
                find_workspace(&world, wanted, None).map(|w| w.id),
                Ok(tree.id),
                "{wanted}"
            );
        }
        assert!(find_workspace(&world, "nowhere", None).is_err());
        let inside = Some(world.sessions[1].id);
        assert_eq!(
            current_workspace(&world, inside, Path::new("/elsewhere")).map(|w| w.id),
            Some(tree.id)
        );
        assert_eq!(
            current_workspace(&world, None, Path::new("/nowhere/app/src")).map(|w| w.id),
            Some(checkout.id)
        );
    }
}
