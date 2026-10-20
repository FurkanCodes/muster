//! The previews the daemon holds for the person to approve: at most
//! [`MOST`], each for [`FOR`], under a random id (`ARC-14`). A preview let
//! go has its clone removed.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::fetch::Found;

/// The most previews held at once; a new one lets go of the oldest.
pub(crate) const MOST: usize = 8;

/// How long a preview is held.
pub(crate) const FOR: Duration = Duration::from_mins(10);

/// The previews, oldest first.
#[derive(Debug, Default)]
pub(crate) struct Previews {
    held: Vec<Held>,
}

#[derive(Debug)]
struct Held {
    id: String,
    found: Found,
    made: Instant,
}

impl Previews {
    /// Holds `found` as preview `id`, made at `now`; the clones of those
    /// let go, to remove.
    pub(crate) fn hold(&mut self, id: String, found: Found, now: Instant) -> Vec<PathBuf> {
        let mut gone = self.expire(now);
        self.held.push(Held {
            id,
            found,
            made: now,
        });
        while self.held.len() > MOST {
            let oldest = self.held.remove(0);
            gone.extend(oldest.found.clone);
        }
        gone
    }

    /// Takes preview `id` if it is held at `now`, with the clones of those
    /// let go meanwhile.
    pub(crate) fn take(&mut self, id: &str, now: Instant) -> (Option<Found>, Vec<PathBuf>) {
        let gone = self.expire(now);
        let found = self
            .held
            .iter()
            .position(|held| held.id == id)
            .map(|at| self.held.remove(at).found);
        (found, gone)
    }

    /// Lets go of every preview of the plugin `plugin`; their clones.
    pub(crate) fn forget(&mut self, plugin: &str) -> Vec<PathBuf> {
        let (gone, kept): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|held| held.found.manifest.id == plugin);
        self.held = kept;
        gone.into_iter()
            .filter_map(|held| held.found.clone)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.held.len()
    }

    /// Lets go of those held longer than [`FOR`] at `now`; their clones.
    fn expire(&mut self, now: Instant) -> Vec<PathBuf> {
        let (gone, kept): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|held| now.saturating_duration_since(held.made) > FOR);
        self.held = kept;
        gone.into_iter()
            .filter_map(|held| held.found.clone)
            .collect()
    }
}
