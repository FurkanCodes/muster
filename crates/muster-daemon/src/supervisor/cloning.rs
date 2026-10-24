//! Cloning a project from GitHub (`PRODUCT.md` §6, §6a): the repositories
//! the person can reach, listed off the loop, and one cloned into a folder
//! they chose, then added as any project is. The checks run in the loop;
//! `gh` works off it (`ARC-13`), as a clone takes as long as the
//! repository is big.

use std::path::{Path, PathBuf};

use muster_core::protocol::repositories::{folder_of, repository_name};
use muster_core::protocol::{ErrorBody, ErrorCode, Reply};
use tokio::sync::oneshot;

use super::{Request, Supervisor};
use crate::service::ServiceError;

/// A clone done off the loop, or not, on its way back to be added.
pub(super) struct Cloned {
    root: PathBuf,
    outcome: Result<(), ServiceError>,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl Supervisor {
    /// `ListRepositories`: answered from a task of its own, as `gh` talks to GitHub.
    pub(super) fn list_repositories(&self, reply: oneshot::Sender<Result<Reply, ErrorBody>>) {
        let repositories = self.systems.repositories.clone();
        self.tasks.spawn(async move {
            let listed = repositories.list().await;
            if let Err(error) = &listed {
                tracing::info!(command = "list repositories", %error, "command refused");
            }
            let _ = reply.send(
                listed
                    .map(Reply::Repositories)
                    .map_err(|error| error.body()),
            );
        });
    }

    /// `CloneProject`: refused where `repository` is no GitHub repository's
    /// name, `into` is no folder, or its folder of that name is there
    /// already; else cloned off the loop and added as a project.
    pub(super) fn clone_project(
        &self,
        repository: &str,
        into: &Path,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let (name, root) = match clone_plan(repository, into) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::info!(command = "clone project", %error, "command refused");
                let _ = reply.send(Err(error));
                return;
            }
        };
        let (repositories, requests) = (self.systems.repositories.clone(), self.requests.clone());
        self.tasks.spawn(async move {
            let outcome = repositories.clone_to(&name, &root).await;
            let cloned = Cloned {
                root,
                outcome,
                reply,
            };
            let _ = requests.send(Request::Cloned(Box::new(cloned))).await;
        });
    }

    /// A clone back from off the loop: added as a project, and answered.
    pub(super) async fn cloned(&mut self, cloned: Cloned) {
        let Cloned {
            root,
            outcome,
            reply,
        } = cloned;
        let added = match outcome {
            Ok(()) => self.add_project(root, None).await,
            Err(error) => Err(error.body()),
        };
        if let Err(error) = &added {
            tracing::info!(command = "clone project", %error, "command refused");
        }
        let _ = reply.send(added);
    }
}

/// The repository's name and the folder its clone goes in, or why not.
fn clone_plan(repository: &str, into: &Path) -> Result<(String, PathBuf), ErrorBody> {
    let Some(name) = repository_name(repository) else {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{repository:?} is no GitHub repository: write it as owner/name"),
        ));
    };
    if !into.is_absolute() || !into.is_dir() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is no folder to clone into", into.display()),
        ));
    }
    let root = into.join(folder_of(&name));
    if root.exists() {
        return Err(ErrorBody::new(
            ErrorCode::Conflict,
            format!(
                "{} is there already: add it as a folder, or clone into another",
                root.display()
            ),
        ));
    }
    Ok((name, root))
}
