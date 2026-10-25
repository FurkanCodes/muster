//! What runs in front in each terminal (`PRODUCT.md` §6, `ARC-28`).
//!
//! On a cheap tick, every [`DaemonConfig::front_interval`](crate::DaemonConfig)
//! while the person looks at a client and every [`UNWATCHED_EVERY`]th one
//! otherwise, the watcher asks each running terminal for its foreground
//! process group. Only where the group changed, or every [`READ_AGAIN`]th
//! tick (a program can `exec` another in its place), does it read the
//! group leader's name and arguments, through [`Processes`] on a blocking
//! thread, and decide what that is (`muster_core::front`). The Supervisor
//! hears only what changed: the terminal's name, and the agent in front.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use muster_core::SessionId;
use muster_core::front::{Front, ProcessFacts};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::supervisor::SupervisorHandle;

/// While nobody looks at a client, one tick in this many looks.
const UNWATCHED_EVERY: u32 = 4;

/// A terminal whose foreground group stays the same is read again after
/// this many of its looks.
const READ_AGAIN: u32 = 4;

/// Reads processes (`ARC-03`): the system's, or a table in tests.
pub(crate) trait Processes: Send + 'static {
    /// Process `pid`'s name and arguments; `None` where it has gone.
    fn facts(&mut self, pid: u32) -> Option<ProcessFacts>;

    /// The folder process `pid` is in; `None` where it has gone or cannot
    /// be read.
    fn cwd(&mut self, pid: u32) -> Option<PathBuf>;
}

/// The system's processes, through sysinfo: one process read at a time,
/// its name and arguments only.
pub(crate) struct SystemProcesses {
    system: System,
}

impl SystemProcesses {
    pub(crate) fn new() -> Self {
        Self {
            system: System::new(),
        }
    }
}

impl std::fmt::Debug for SystemProcesses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SystemProcesses")
    }
}

impl Processes for SystemProcesses {
    fn facts(&mut self, pid: u32) -> Option<ProcessFacts> {
        let pid = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
        );
        let process = self.system.process(pid)?;
        Some(ProcessFacts {
            name: process.name().to_string_lossy().into_owned(),
            argv: process
                .cmd()
                .iter()
                .map(|word| word.to_string_lossy().into_owned())
                .collect(),
        })
    }

    fn cwd(&mut self, pid: u32) -> Option<PathBuf> {
        let pid = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always),
        );
        self.system.process(pid)?.cwd().map(PathBuf::from)
    }
}

/// What process group leader `pid` runs, as a person would name it: what
/// a terminal's shell would end were the terminal closed.
pub(crate) async fn program(pid: u32) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        let facts = SystemProcesses::new().facts(pid)?;
        Some(muster_core::front::identify(&facts, []).program)
    })
    .await
    .ok()
    .flatten()
}

/// The folder a terminal's shell, process `pid`, is in now: read only when
/// asked, on a blocking thread, never on the watcher's tick (`ARC-33`).
pub(crate) async fn shell_folder(pid: u32) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || SystemProcesses::new().cwd(pid))
        .await
        .ok()
        .flatten()
}

/// What the watcher last saw in one terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    group: u32,
    /// Looks since its leader was read.
    looks: u32,
    front: Option<Front>,
}

/// What the watcher knows of every terminal between looks: which to read,
/// and which of what it read is news.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    seen: HashMap<SessionId, Seen>,
}

impl Tracker {
    /// Forgets the terminals not among `running`.
    pub(crate) fn keep(&mut self, running: &[SessionId]) {
        self.seen.retain(|session, _| running.contains(session));
    }

    /// Whether `session`'s foreground, group `group` now, is to be read.
    pub(crate) fn due(&mut self, session: SessionId, group: u32) -> bool {
        match self.seen.get_mut(&session) {
            Some(seen) if seen.group == group && seen.looks + 1 < READ_AGAIN => {
                seen.looks += 1;
                false
            }
            Some(seen) => {
                seen.group = group;
                seen.looks = 0;
                true
            }
            None => {
                self.seen.insert(
                    session,
                    Seen {
                        group,
                        looks: 0,
                        front: None,
                    },
                );
                true
            }
        }
    }

    /// `front` read in `session`: itself where it is news, `None` where
    /// it is what was there before.
    pub(crate) fn found(&mut self, session: SessionId, front: Front) -> Option<Front> {
        let seen = self.seen.get_mut(&session)?;
        if seen.front.as_ref() == Some(&front) {
            return None;
        }
        seen.front = Some(front.clone());
        Some(front)
    }
}

/// Watches what runs in front in every terminal until the Supervisor
/// stops.
pub(crate) async fn run(supervisor: SupervisorHandle, every: Duration, processes: impl Processes) {
    let Some(mut looked_at) = supervisor.looked_at().await else {
        return;
    };
    let mut processes = Some(processes);
    let mut tracker = Tracker::default();
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut unwatched = 0_u32;
    loop {
        ticks.tick().await;
        if !*looked_at.borrow_and_update() {
            unwatched += 1;
            if unwatched < UNWATCHED_EVERY {
                continue;
            }
        }
        unwatched = 0;
        let Some(terminals) = supervisor.terminals().await else {
            return;
        };
        let running: Vec<SessionId> = terminals.list.iter().map(|(id, _)| *id).collect();
        tracker.keep(&running);
        let mut to_read = Vec::new();
        for (session, pty) in terminals.list {
            if let Some(group) = pty.foreground().await
                && tracker.due(session, group)
            {
                to_read.push((session, group));
            }
        }
        if to_read.is_empty() {
            continue;
        }
        let Some(mut taken) = processes.take() else {
            return;
        };
        let added = terminals.added;
        let read = tokio::task::spawn_blocking(move || {
            let fronts: Vec<(SessionId, Front)> = to_read
                .into_iter()
                .filter_map(|(session, group)| {
                    let facts = taken.facts(group)?;
                    let added = added
                        .iter()
                        .map(|(name, program)| (name.as_str(), program.as_str()));
                    Some((session, muster_core::front::identify(&facts, added)))
                })
                .collect();
            (taken, fronts)
        })
        .await;
        let Ok((returned, fronts)) = read else {
            tracing::error!("reading what runs in the terminals failed");
            return;
        };
        processes = Some(returned);
        let news: Vec<(SessionId, Front)> = fronts
            .into_iter()
            .filter_map(|(session, front)| Some((session, tracker.found(session, front)?)))
            .collect();
        if !news.is_empty() && !supervisor.fronts(news).await {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(program: &str, agent: Option<&str>) -> Front {
        Front {
            program: program.to_owned(),
            agent: agent.map(str::to_owned),
        }
    }

    #[test]
    fn a_terminal_is_read_when_its_foreground_changes_and_now_and_then_arc_28() {
        let mut tracker = Tracker::default();
        let shell = SessionId::new();
        assert!(tracker.due(shell, 100), "first seen");
        for _ in 1..READ_AGAIN {
            assert!(!tracker.due(shell, 100), "the same group, not yet");
        }
        assert!(tracker.due(shell, 100), "read again in case it exec'd");
        assert!(tracker.due(shell, 200), "a new group at once");
        tracker.keep(&[]);
        assert!(tracker.due(shell, 200), "forgotten once it stopped");
    }

    #[test]
    fn only_what_changed_is_news() {
        let mut tracker = Tracker::default();
        let shell = SessionId::new();
        assert_eq!(tracker.found(shell, front("zsh", None)), None, "never due");
        assert!(tracker.due(shell, 100));
        assert_eq!(
            tracker.found(shell, front("zsh", None)),
            Some(front("zsh", None))
        );
        assert!(tracker.due(shell, 200));
        assert_eq!(
            tracker.found(shell, front("claude", Some("claude"))),
            Some(front("claude", Some("claude")))
        );
        assert_eq!(tracker.found(shell, front("claude", Some("claude"))), None);
    }

    #[test]
    fn this_process_is_read_as_the_system_has_it() {
        let mut processes = SystemProcesses::new();
        let facts = processes
            .facts(std::process::id())
            .expect("this process runs");
        assert!(!facts.argv.is_empty(), "{facts:?}");
        assert!(processes.facts(u32::MAX - 1).is_none());
        assert_eq!(
            processes.cwd(std::process::id()),
            std::env::current_dir().ok()
        );
        assert!(processes.cwd(u32::MAX - 1).is_none());
    }
}
