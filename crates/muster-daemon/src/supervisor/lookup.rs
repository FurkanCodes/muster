//! Finding what a command names in the state, refused the same way
//! wherever it is asked.

use muster_core::protocol::{ErrorBody, ErrorCode};
use muster_core::{Workspace, WorkspaceId};

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
}
