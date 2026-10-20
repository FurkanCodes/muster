//! Changing the plugins as the person asked (`PRD-15`): holding what was
//! fetched as a preview, installing, linking or updating what a preview
//! showed, turning a plugin on or off, removing one. Made on the
//! Supervisor's loop, so two changes never interleave in Muster's
//! settings; the folders a change lets go of are handed back, to remove
//! off it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use muster_core::config::edit::Written;
use muster_core::plugins::PluginSource;
use muster_core::protocol::plugins::{
    ChangedBy, PluginAction, PluginChange, PluginPreview, PreviewKind,
};
use muster_core::time::Timestamp;

use super::fetch::{Found, Job};
use super::{Known, PluginError, Registry, files};

/// What a change lets go of: folders to remove, off the loop.
pub(crate) type Gone = Vec<PathBuf>;

impl Registry {
    /// What fetching an update of the plugin `id` takes: one from git
    /// cloned again at its ref, a linked folder read again.
    pub(crate) fn update_job(&self, id: &str) -> Result<Job, PluginError> {
        let known = self.find(id)?;
        let update_of = Some(id.to_owned());
        match &known.info.source {
            PluginSource::Builtin => Err(PluginError::Own { id: id.to_owned() }),
            PluginSource::Git { url, git_ref, .. } => Ok(Job::Clone {
                url: url.clone(),
                git_ref: git_ref.clone(),
                update_of,
            }),
            PluginSource::Folder { path } => Ok(Job::Folder {
                path: path.clone(),
                update_of,
            }),
        }
    }

    /// Holds `found` as a preview at `now`, or refuses it: installing a
    /// plugin whose id is taken, updating one whose manifest calls it
    /// something else now.
    pub(crate) fn hold(
        &mut self,
        mut found: Found,
        now: Instant,
    ) -> (Result<PluginPreview, PluginError>, Gone) {
        let id = found.manifest.id.clone();
        let current = self.find(&id).ok().map(|known| known.info.clone());
        let refused = match (&found.update_of, found.kind, &current) {
            (Some(named), _, _) if *named != id => Some(PluginError::Renamed {
                id: named.clone(),
                found: id.clone(),
            }),
            (_, _, Some(current)) if current.source == PluginSource::Builtin => {
                Some(PluginError::Own { id: id.clone() })
            }
            (None, PreviewKind::Install, Some(_)) => {
                Some(PluginError::Installed { id: id.clone() })
            }
            (_, PreviewKind::Link, Some(current))
                if !matches!(current.source, PluginSource::Folder { .. }) =>
            {
                Some(PluginError::Installed { id: id.clone() })
            }
            _ => None,
        };
        if let Some(refused) = refused {
            return (Err(refused), found.clone.take().into_iter().collect());
        }
        let token = match crate::harness::new_token() {
            Ok(token) => token[..32].to_owned(),
            Err(error) => {
                let gone = found.clone.take().into_iter().collect();
                return (
                    Err(PluginError::io("cannot draw a preview's id")(error)),
                    gone,
                );
            }
        };
        let preview = PluginPreview {
            preview: token.clone(),
            kind: found.kind,
            id,
            source: found.source.clone(),
            details: found.details.clone(),
            current,
        };
        let gone = self.previews.hold(token, found, now);
        (Ok(preview), gone)
    }

    /// Installs, links or updates what the preview `preview` showed, as
    /// the person approved it at `at`, by `by`.
    pub(crate) fn install(
        &mut self,
        preview: &str,
        by: ChangedBy,
        now: Instant,
        at: Timestamp,
    ) -> (Result<(), PluginError>, Gone) {
        let (found, mut gone) = self.previews.take(preview, now);
        let Some(found) = found else {
            return (Err(PluginError::NoPreview), gone);
        };
        let id = found.manifest.id.clone();
        match self.apply(found, &mut gone) {
            Ok(what) => {
                self.changes.insert(id, PluginChange { what, by, at });
                (Ok(()), gone)
            }
            Err(error) => (Err(error), gone),
        }
    }

    /// Makes what `found` showed so, in the plugins' folder and Muster's
    /// settings; what it lets go of goes into `gone`.
    fn apply(&mut self, mut found: Found, gone: &mut Gone) -> Result<PluginAction, PluginError> {
        let id = found.manifest.id.clone();
        let current = self.find(&id).ok().cloned();
        let approved = found.details.sha256.clone();
        let clone = found.clone.take();
        match (found.kind, &found.source, current, clone) {
            (PreviewKind::Install, PluginSource::Git { .. }, Some(_), clone) => {
                gone.extend(clone);
                Err(PluginError::Installed { id })
            }
            (
                PreviewKind::Install,
                PluginSource::Git {
                    url,
                    git_ref,
                    commit,
                },
                None,
                Some(clone),
            ) => {
                let target = self.dir.join(&id).join(commit);
                if let Err(error) = files::place(&clone, &target) {
                    gone.push(clone);
                    return Err(PluginError::io("cannot put the plugin in place")(error));
                }
                let values = git(url, git_ref.as_deref(), commit, &approved, Some(true));
                if let Err(error) = self.write(&id, &values) {
                    gone.push(self.dir.join(&id));
                    return Err(error);
                }
                Ok(PluginAction::Installed)
            }
            (PreviewKind::Link, PluginSource::Folder { path }, current, _) => {
                self.link(&id, path, current.as_ref(), &approved)
            }
            (PreviewKind::Update, PluginSource::Git { .. }, Some(current), Some(clone)) => {
                self.update(&found, &current, clone, &approved, gone)
            }
            (_, _, _, clone) => {
                gone.extend(clone);
                Err(PluginError::NoPreview)
            }
        }
    }

    /// Links the folder `path` as the plugin `id`, or links it again,
    /// approving the manifest hashing to `approved`.
    fn link(
        &self,
        id: &str,
        path: &Path,
        current: Option<&Known>,
        approved: &str,
    ) -> Result<PluginAction, PluginError> {
        if let Some(current) = current
            && !matches!(current.info.source, PluginSource::Folder { .. })
        {
            return Err(PluginError::Installed { id: id.to_owned() });
        }
        let path = path.display().to_string();
        let mut values = vec![
            ("folder", Some(Written::Text(&path))),
            ("git", None),
            ("ref", None),
            ("commit", None),
            ("approved", Some(Written::Text(approved))),
        ];
        // Linked again, it keeps whether it was on.
        if current.is_none() {
            values.push(("enabled", Some(Written::Flag(true))));
        }
        self.write(id, &values)?;
        Ok(PluginAction::Linked)
    }

    /// Replaces the plugin `current` from git with the commit `found`
    /// fetched into `clone`, approving the manifest hashing to `approved`.
    /// The same commit again replaces Muster's copy of it, as the person
    /// approved it afresh.
    fn update(
        &self,
        found: &Found,
        current: &Known,
        clone: PathBuf,
        approved: &str,
        gone: &mut Gone,
    ) -> Result<PluginAction, PluginError> {
        let id = &current.info.id;
        let (
            PluginSource::Git {
                url,
                git_ref,
                commit,
            },
            PluginSource::Git { commit: was, .. },
        ) = (&found.source, &current.info.source)
        else {
            gone.push(clone);
            return Err(PluginError::Installed { id: id.clone() });
        };
        let target = self.dir.join(id).join(commit);
        let aside = files::incoming(&self.dir).join(format!("{commit}.old"));
        let set_aside = was == commit && target.exists();
        if set_aside {
            let _ = files::remove(&aside);
            if let Err(error) = std::fs::rename(&target, &aside) {
                gone.push(clone);
                return Err(PluginError::io("cannot set the old copy aside")(error));
            }
        }
        let put_back = || {
            if set_aside {
                let _ = files::remove(&target);
                let _ = std::fs::rename(&aside, &target);
            }
        };
        if let Err(error) = files::place(&clone, &target) {
            gone.push(clone);
            put_back();
            return Err(PluginError::io("cannot put the plugin in place")(error));
        }
        if let Err(error) = self.write(id, &git(url, git_ref.as_deref(), commit, approved, None)) {
            if !set_aside {
                gone.push(target.clone());
            }
            put_back();
            return Err(error);
        }
        if set_aside {
            gone.push(aside);
        } else {
            gone.push(self.dir.join(id).join(was));
        }
        Ok(PluginAction::Updated)
    }

    /// Turns the plugin `id` on or off, as `by` asked at `at`. One that
    /// cannot run is not turned on.
    pub(crate) fn set_enabled(
        &mut self,
        id: &str,
        on: bool,
        by: ChangedBy,
        at: Timestamp,
    ) -> Result<(), PluginError> {
        let known = self.find(id)?;
        if on && let Some(problem) = &known.info.problem {
            return Err(PluginError::CannotRun {
                id: id.to_owned(),
                problem: problem.clone(),
            });
        }
        self.write(id, &[("enabled", Some(Written::Flag(on)))])?;
        let what = if on {
            PluginAction::TurnedOn
        } else {
            PluginAction::TurnedOff
        };
        self.changes
            .insert(id.to_owned(), PluginChange { what, by, at });
        Ok(())
    }

    /// Removes the plugin `id`: its entry, its folder under the plugins'
    /// folder, its previews and what is known of its changes. A linked
    /// folder is left where it is; Muster's own are not removed.
    pub(crate) fn remove(&mut self, id: &str) -> (Result<(), PluginError>, Gone) {
        let known = match self.find(id) {
            Ok(known) => known,
            Err(error) => return (Err(error), Vec::new()),
        };
        if known.info.source == PluginSource::Builtin {
            return (Err(PluginError::Own { id: id.to_owned() }), Vec::new());
        }
        if let Err(error) = crate::config::change_plugin(&self.config_dir, id, None) {
            return (Err(error.into()), Vec::new());
        }
        self.changes.remove(id);
        let mut gone = self.previews.forget(id);
        gone.push(self.dir.join(id));
        (Ok(()), gone)
    }

    /// Writes `values` into the plugin `id`'s entry in Muster's settings.
    fn write(&self, id: &str, values: &[(&str, Option<Written<'_>>)]) -> Result<(), PluginError> {
        crate::config::change_plugin(&self.config_dir, id, Some(values)).map_err(Into::into)
    }
}

/// The entry of a plugin from git; `enabled` written only where given.
fn git<'a>(
    url: &'a str,
    git_ref: Option<&'a str>,
    commit: &'a str,
    approved: &'a str,
    enabled: Option<bool>,
) -> Vec<(&'static str, Option<Written<'a>>)> {
    let mut values = vec![
        ("git", Some(Written::Text(url))),
        ("ref", git_ref.map(Written::Text)),
        ("commit", Some(Written::Text(commit))),
        ("folder", None),
        ("approved", Some(Written::Text(approved))),
    ];
    if let Some(enabled) = enabled {
        values.push(("enabled", Some(Written::Flag(enabled))));
    }
    values
}
