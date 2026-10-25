//! Finding what a command names in the state, refused the same way
//! wherever it is asked.

use muster_core::protocol::{ErrorBody, ErrorCode};
use muster_core::{Session, Workspace, WorkspaceId};

use super::Supervisor;

impl Supervisor {
    /// The workspace `id`, or why there is none.
    pub(super) fn workspace(&self, id: WorkspaceId) -> Result<&Workspace, ErrorBody> {
        self.state
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no workspace {id}")))
    }

    /// The workspace `session` runs in, or why it has none: a free
    /// terminal is in no project (`ARC-33`).
    pub(super) fn workspace_of(&self, session: &Session) -> Result<&Workspace, ErrorBody> {
        let id = session.workspace.ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::BadRequest,
                format!(
                    "{} is a terminal in no project",
                    muster_core::text::one_line(&session.title)
                ),
            )
        })?;
        self.workspace(id)
    }
}
