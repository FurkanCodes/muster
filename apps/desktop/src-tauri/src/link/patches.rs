//! Patches: what changed in the model since the webview was last told, as
//! whole entities, those gone, and what the model derives (`ARC-24`).
//!
//! The link keeps what it last sent and compares the model with it, so a
//! patch says each change once however the deltas came (a project removed
//! takes its workspaces and sessions with it; a session upserted and
//! removed between two patches is never sent), and a burst of deltas makes
//! one patch (`ARC-25`: at most one a frame, `PATCH_EVERY`).

use std::collections::{BTreeMap, HashMap};

use muster_core::protocol::ResourceReport;
use muster_core::protocol::plugins::PluginInfo;
use muster_core::settings::Preferences;
use muster_core::{Project, ProjectId, Session, SessionId, Workspace, WorkspaceId};
use muster_model::Model;

use crate::contract::{Counts, Derived, Gone, Patch, Row};

/// What the webview was last sent.
#[derive(Default)]
struct Sent {
    projects: HashMap<ProjectId, Project>,
    workspaces: HashMap<WorkspaceId, Workspace>,
    sessions: HashMap<SessionId, Session>,
    preferences: Preferences,
    plugins: Vec<PluginInfo>,
    resources: Option<ResourceReport>,
    notifier: bool,
    derived: Derived,
}

/// Builds the patches the webview is sent.
#[derive(Default)]
pub(super) struct Patcher {
    /// `None` until the first patch, and after a reset: the next is whole.
    sent: Option<Sent>,
}

impl Patcher {
    /// The next patch replaces everything the webview holds: after a
    /// (re)subscription, a snapshot, or a webview loaded again.
    pub(super) fn reset(&mut self) {
        self.sent = None;
    }

    /// What changed since the last patch, or `None` if nothing did. The
    /// model must have its state (`Model::loaded`).
    pub(super) fn patch(
        &mut self,
        model: &Model,
        questions: BTreeMap<SessionId, Vec<String>>,
    ) -> Option<Patch> {
        let derived = derive(model, questions);
        let Some(sent) = &mut self.sent else {
            let patch = whole(model, derived);
            self.sent = Some(Sent {
                projects: by_id(&patch.projects, |project| project.id),
                workspaces: by_id(&patch.workspaces, |workspace| workspace.id),
                sessions: by_id(&patch.sessions, |session| session.id),
                preferences: model.preferences().clone(),
                plugins: model.plugins().to_vec(),
                resources: model.resources().cloned(),
                notifier: model.notifier(),
                derived: patch.derived.clone(),
            });
            return Some(patch);
        };
        let mut patch = Patch {
            projects: changed(model.projects(), &mut sent.projects, |p| p.id),
            workspaces: changed(model.workspaces(), &mut sent.workspaces, |w| w.id),
            sessions: changed(model.sessions(), &mut sent.sessions, |s| s.id),
            gone: Gone {
                projects: gone(model.projects(), &mut sent.projects, |p| p.id),
                workspaces: gone(model.workspaces(), &mut sent.workspaces, |w| w.id),
                sessions: gone(model.sessions(), &mut sent.sessions, |s| s.id),
            },
            ..Patch::default()
        };
        if model.preferences() != &sent.preferences {
            sent.preferences = model.preferences().clone();
            patch.preferences = Some(sent.preferences.clone());
        }
        if model.plugins() != sent.plugins.as_slice() {
            sent.plugins = model.plugins().to_vec();
            patch.plugins = Some(sent.plugins.clone());
        }
        if model.resources() != sent.resources.as_ref() {
            sent.resources = model.resources().cloned();
            patch.resources.clone_from(&sent.resources);
        }
        if model.notifier() != sent.notifier {
            sent.notifier = model.notifier();
            patch.notifier = Some(sent.notifier);
        }
        let derived_changed = derived != sent.derived;
        sent.derived = derived;
        patch.derived = sent.derived.clone();
        let any = derived_changed
            || !patch.projects.is_empty()
            || !patch.workspaces.is_empty()
            || !patch.sessions.is_empty()
            || !patch.gone.projects.is_empty()
            || !patch.gone.workspaces.is_empty()
            || !patch.gone.sessions.is_empty()
            || patch.preferences.is_some()
            || patch.plugins.is_some()
            || patch.resources.is_some()
            || patch.notifier.is_some();
        any.then_some(patch)
    }
}

/// Everything the model holds, to replace what the webview holds.
fn whole(model: &Model, derived: Derived) -> Patch {
    Patch {
        reset: true,
        projects: model.projects().to_vec(),
        workspaces: model.workspaces().to_vec(),
        sessions: model.sessions().to_vec(),
        gone: Gone::default(),
        preferences: Some(model.preferences().clone()),
        plugins: Some(model.plugins().to_vec()),
        resources: model.resources().cloned(),
        notifier: Some(model.notifier()),
        derived,
    }
}

/// What the model derives, as the terminal UI shows it.
pub(super) fn derive(model: &Model, questions: BTreeMap<SessionId, Vec<String>>) -> Derived {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let counts = model.counts();
    Derived {
        needs_you: model.needs_you(),
        rows: model
            .rows()
            .into_iter()
            .map(|row| match row {
                muster_model::Row::Project(id) => Row::Project(id),
                muster_model::Row::Worktree(id) => Row::Workspace(id),
                muster_model::Row::Session(id) => Row::Session(id),
            })
            .collect(),
        counts: Counts {
            working: count(counts.working),
            waiting: count(counts.waiting),
            done: count(counts.done),
        },
        questions,
    }
}

fn by_id<T: Clone, K: std::hash::Hash + Eq>(items: &[T], id: impl Fn(&T) -> K) -> HashMap<K, T> {
    items.iter().map(|item| (id(item), item.clone())).collect()
}

/// The entities of `now` not as `sent` has them, which `sent` takes.
fn changed<T: Clone + PartialEq, K: std::hash::Hash + Eq>(
    now: &[T],
    sent: &mut HashMap<K, T>,
    id: impl Fn(&T) -> K,
) -> Vec<T> {
    let mut changed = Vec::new();
    for item in now {
        if sent.get(&id(item)) != Some(item) {
            sent.insert(id(item), item.clone());
            changed.push(item.clone());
        }
    }
    changed
}

/// The ids `sent` has that `now` no longer does, which `sent` lets go.
fn gone<T, K: std::hash::Hash + Eq + Copy>(
    now: &[T],
    sent: &mut HashMap<K, T>,
    id: impl Fn(&T) -> K,
) -> Vec<K> {
    if sent.len() == now.len() {
        // Every one `now` has was just put in `sent`: none is missing.
        return Vec::new();
    }
    let present: std::collections::HashSet<K> = now.iter().map(&id).collect();
    let gone: Vec<K> = sent
        .keys()
        .filter(|key| !present.contains(key))
        .copied()
        .collect();
    for key in &gone {
        sent.remove(key);
    }
    gone
}

#[cfg(test)]
#[path = "patches_tests.rs"]
mod tests;
