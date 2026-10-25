//! Samples of the messages about spaces and free terminals (protocol 57,
//! `ARC-33`), each variant with the values inside it filled in, for
//! `protocol_round_trip`.

use std::path::PathBuf;

use muster_core::protocol::{Command, Delta, Reply, TermSize};
use muster_core::{Member, ProjectId, Session, SessionId, Space, SpaceId, Timestamp, WorkspaceId};

/// A space holding a project and a free terminal.
pub(crate) fn space() -> Space {
    Space {
        id: SpaceId::new(),
        name: "Work".to_owned(),
        members: vec![
            Member::Project(ProjectId::new()),
            Member::Session(SessionId::new()),
        ],
        created_at: Timestamp::from_millis(2),
    }
}

/// A free terminal, in no workspace, as `like` is otherwise.
pub(crate) fn free_terminal(like: &Session) -> Session {
    Session {
        id: SessionId::new(),
        workspace: None,
        harness: "shell".to_owned(),
        agent: None,
        ..like.clone()
    }
}

/// The commands about spaces and free terminals (`ARC-33`).
pub(crate) fn commands(
    project: ProjectId,
    workspace: WorkspaceId,
    session: SessionId,
) -> Vec<Command> {
    vec![
        Command::StartSession {
            workspace: None,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: None,
        },
        Command::CreateSpace {
            name: "Work".to_owned(),
        },
        Command::RenameSpace {
            space: SpaceId::new(),
            name: "Home".to_owned(),
        },
        Command::DeleteSpace {
            space: SpaceId::new(),
        },
        Command::MoveToSpace {
            space: SpaceId::new(),
            member: Member::Project(project),
        },
        Command::MoveToSpace {
            space: SpaceId::new(),
            member: Member::Session(session),
        },
        Command::ReadPlace { session },
        Command::FileSession { session, workspace },
        Command::AddProjectHere {
            session,
            space: Some(SpaceId::new()),
        },
        Command::RestartIn { session, workspace },
    ]
}

/// The replies about spaces and free terminals (`ARC-33`).
pub(crate) fn replies() -> Vec<Reply> {
    vec![
        Reply::SpaceCreated { id: SpaceId::new() },
        Reply::Place {
            folder: Some(PathBuf::from("/src/app/crates")),
            workspace: Some(WorkspaceId::new()),
            front: Some("npm".to_owned()),
        },
        Reply::Place {
            folder: None,
            workspace: None,
            front: None,
        },
    ]
}

/// The deltas about spaces.
pub(crate) fn deltas() -> Vec<Delta> {
    vec![
        Delta::SpaceUpserted(space()),
        Delta::SpaceRemoved { id: SpaceId::new() },
    ]
}
