//! Adding projects.

use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{Project, ProjectId};

use super::{Supervisor, store_closed, store_error};

impl Supervisor {
    pub(super) async fn add_project(
        &mut self,
        root: PathBuf,
        name: Option<String>,
    ) -> Result<Reply, ErrorBody> {
        let root = repository_root(&root).await?;
        if self
            .state
            .projects
            .iter()
            .any(|project| project.root == root)
        {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is already a project", root.display()),
            ));
        }
        let name = name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                root.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| root.display().to_string());
        let project = Project {
            id: ProjectId::new(),
            name,
            root,
            created_at: self.clock.now(),
        };

        let stored = project.clone();
        self.store
            .call(move |store| store.insert_project(&stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;

        self.state.projects.push(project.clone());
        let id = project.id;
        self.broadcast(Delta::ProjectUpserted(project));
        Ok(Reply::ProjectAdded { id })
    }
}

/// The canonical path of the git repository at `path`.
async fn repository_root(path: &Path) -> Result<PathBuf, ErrorBody> {
    if !path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not an absolute path", path.display()),
        ));
    }
    let root = tokio::fs::canonicalize(path).await.map_err(|error| {
        ErrorBody::new(
            ErrorCode::BadRequest,
            format!("cannot open {}: {error}", path.display()),
        )
    })?;
    let is_repository = tokio::fs::try_exists(root.join(".git"))
        .await
        .unwrap_or(false);
    if !root.is_dir() || !is_repository {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not a git repository", root.display()),
        ));
    }
    Ok(root)
}
