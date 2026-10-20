//! Fetching a plugin to preview it, off the Supervisor's loop: a git
//! repository cloned into `.incoming`, or a folder read where it is.
//! Nothing of the plugin runs: git fetches it, Muster reads its manifest
//! (`PRD-15`), and no build or install step of a plugin's is run.

use std::path::{Path, PathBuf};

use muster_core::plugins::{Manifest, PluginSource};
use muster_core::protocol::plugins::{PluginDetails, PreviewKind};
use muster_core::protocol::{ErrorBody, Reply};
use tokio::sync::oneshot;

use super::{PluginError, files};

/// What to fetch.
#[derive(Debug)]
pub(crate) enum Job {
    /// A git repository, to install, or to update the plugin `update_of`.
    Clone {
        url: String,
        git_ref: Option<String>,
        update_of: Option<String>,
    },
    /// A folder, read where it is, to link, or to link the plugin
    /// `update_of` again.
    Folder {
        path: PathBuf,
        update_of: Option<String>,
    },
}

/// A plugin fetched and read, for a preview.
#[derive(Debug)]
pub(crate) struct Found {
    pub(crate) kind: PreviewKind,
    pub(crate) source: PluginSource,
    /// The clone, to move into place; `None` for a linked folder.
    pub(crate) clone: Option<PathBuf>,
    pub(crate) manifest: Manifest,
    pub(crate) details: PluginDetails,
    /// The plugin it updates or links again, as the person named it.
    pub(crate) update_of: Option<String>,
}

/// A fetch, back on the Supervisor's loop with the client to answer.
#[derive(Debug)]
pub(crate) struct Fetched {
    pub(crate) outcome: Result<Found, PluginError>,
    pub(crate) reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

/// Fetches what `job` names, a clone going into `incoming`.
pub(crate) async fn fetch(job: Job, incoming: PathBuf) -> Result<Found, PluginError> {
    match job {
        Job::Clone {
            url,
            git_ref,
            update_of,
        } => {
            muster_core::plugins::check_address(&url)?;
            if let Some(git_ref) = &git_ref
                && !muster_core::plugins::is_git_ref(git_ref)
            {
                return Err(PluginError::Ref(git_ref.clone()));
            }
            let clone = cloned(&url, git_ref.as_deref(), &incoming).await?;
            let read = read_clone(&clone).await;
            let (commit, manifest, hash) = match read {
                Ok(read) => read,
                Err(error) => {
                    files::remove_later(clone).await;
                    return Err(error);
                }
            };
            let kind = match &update_of {
                Some(_) => PreviewKind::Update,
                None => PreviewKind::Install,
            };
            let details = details(&manifest, hash).await;
            Ok(Found {
                kind,
                source: PluginSource::Git {
                    url,
                    git_ref,
                    commit,
                },
                clone: Some(clone),
                manifest,
                details,
                update_of,
            })
        }
        Job::Folder { path, update_of } => {
            if !path.is_absolute() {
                return Err(PluginError::NotAFolder { path });
            }
            let folder = path.clone();
            let (manifest, hash) = blocking(move || super::read_manifest(&folder)).await?;
            let details = details(&manifest, hash).await;
            Ok(Found {
                kind: PreviewKind::Link,
                source: PluginSource::Folder { path },
                clone: None,
                manifest,
                details,
                update_of,
            })
        }
    }
}

/// Clones `url` at `git_ref` into a new folder in `incoming`: `--depth 1`,
/// no shell, no prompt, git's `ext` transport refused, within git's time
/// limit. A repository's folder is cloned as `file://`, so the depth
/// holds.
async fn cloned(url: &str, git_ref: Option<&str>, incoming: &Path) -> Result<PathBuf, PluginError> {
    let dir = incoming.to_owned();
    blocking(move || {
        files::private_dir(&dir).map_err(PluginError::io("cannot make the folder for previews"))
    })
    .await?;
    let name = crate::harness::new_token().map_err(PluginError::io("cannot draw a random name"))?;
    let clone = incoming.join(&name[..16]);
    let from = if url.starts_with('/') {
        format!("file://{url}")
    } else {
        url.to_owned()
    };
    let into = clone.display().to_string();
    let mut args = vec![
        "-c",
        "protocol.ext.allow=never",
        "clone",
        "--depth",
        "1",
        "--quiet",
    ];
    if let Some(git_ref) = git_ref {
        args.extend(["--branch", git_ref]);
    }
    args.extend(["--", from.as_str(), into.as_str()]);
    if let Err(error) = crate::git::checked(incoming, &args).await {
        files::remove_later(clone).await;
        return Err(PluginError::Fetch(said(&error)));
    }
    Ok(clone)
}

/// What went wrong in git, without the command line, which names the
/// folders.
fn said(error: &crate::git::GitError) -> String {
    use crate::git::GitError;
    let said = match error {
        GitError::Failed { stderr, .. } => stderr.clone(),
        GitError::TimedOut(_) => "git took longer than two minutes".to_owned(),
        GitError::Spawn(error) => format!("could not run git: {error}"),
    };
    muster_core::text::one_line(&said)
}

/// The commit `clone` holds, its manifest and the manifest's hash.
async fn read_clone(clone: &Path) -> Result<(String, Manifest, String), PluginError> {
    let commit = crate::git::checked(clone, &["rev-parse", "HEAD"])
        .await
        .map_err(|error| PluginError::Fetch(said(&error)))?;
    if !muster_core::plugins::is_commit(&commit) {
        return Err(PluginError::Fetch(format!(
            "git named no commit for it: {}",
            muster_core::text::one_line(&commit)
        )));
    }
    let folder = clone.to_owned();
    let (manifest, hash) = blocking(move || super::read_manifest(&folder)).await?;
    Ok((commit, manifest, hash))
}

/// What `manifest` says, with which programs it needs are on `PATH`.
async fn details(manifest: &Manifest, hash: String) -> PluginDetails {
    let requires = manifest.requires.clone();
    let found: Vec<bool> = blocking(move || {
        Ok::<_, PluginError>(
            requires
                .iter()
                .map(|program| super::on_path(program))
                .collect(),
        )
    })
    .await
    .unwrap_or_default();
    PluginDetails::of(manifest, hash, |program| {
        manifest
            .requires
            .iter()
            .position(|required| required == program)
            .and_then(|at| found.get(at).copied())
            .unwrap_or(false)
    })
}

/// Runs `work`, which reads or writes files, off the async workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, PluginError> + Send + 'static,
) -> Result<T, PluginError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| PluginError::Fetch(format!("its reading stopped: {error}")))?
}
