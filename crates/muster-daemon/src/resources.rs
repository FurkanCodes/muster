//! Measuring what sessions use (`PRD-05`, `ARCHITECTURE.md` §9).
//!
//! Every few seconds the sampler asks the Supervisor which sessions are
//! running and under which process, measures each one's whole process tree
//! (an agent's tools and builds run as its children) and the host's memory,
//! and hands the report back. Measuring blocks for a few milliseconds, so it
//! runs on a blocking thread; the Supervisor only stores and broadcasts.

use std::collections::HashMap;
use std::time::Duration;

use muster_core::SessionId;
use muster_core::protocol::{ResourceReport, SessionUsage};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::supervisor::SupervisorHandle;

/// One process as measured: its id, its parent's, its memory and its CPU
/// in percent of one core.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProcessUse {
    pub(crate) pid: u32,
    pub(crate) parent: Option<u32>,
    pub(crate) memory_bytes: u64,
    pub(crate) cpu_percent: f32,
}

/// The memory and CPU (in thousandths of a core) of `root` and everything
/// below it in `table`.
pub(crate) fn tree_usage(table: &[ProcessUse], root: u32) -> (u64, u32) {
    let mut children: HashMap<u32, Vec<&ProcessUse>> = HashMap::new();
    for process in table {
        if let Some(parent) = process.parent {
            children.entry(parent).or_default().push(process);
        }
    }
    let Some(top) = table.iter().find(|process| process.pid == root) else {
        return (0, 0);
    };
    let (mut memory, mut cpu) = (0u64, 0f32);
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![top];
    while let Some(process) = stack.pop() {
        // A table read while processes come and go can be inconsistent.
        if !seen.insert(process.pid) {
            continue;
        }
        memory += process.memory_bytes;
        cpu += process.cpu_percent.max(0.0);
        stack.extend(children.get(&process.pid).into_iter().flatten());
    }
    (memory, (cpu * 10.0).round() as u32)
}

/// Measures processes and the host.
pub(crate) struct Sampler {
    system: System,
}

impl std::fmt::Debug for Sampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sampler")
    }
}

impl Sampler {
    pub(crate) fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    /// Measures each `(session, pid)` and the host. CPU is measured since
    /// the previous call, so the first report shows none.
    pub(crate) fn sample(&mut self, roots: &[(SessionId, u32)]) -> ResourceReport {
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
        let table: Vec<ProcessUse> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessUse {
                pid: pid.as_u32(),
                parent: process.parent().map(|parent| parent.as_u32()),
                memory_bytes: process.memory(),
                cpu_percent: process.cpu_usage(),
            })
            .collect();
        ResourceReport {
            host_memory_used: self.system.used_memory(),
            host_memory_total: self.system.total_memory(),
            sessions: roots
                .iter()
                .map(|(session, pid)| {
                    let (memory_bytes, cpu_permille) = tree_usage(&table, *pid);
                    SessionUsage {
                        session: *session,
                        memory_bytes,
                        cpu_permille,
                    }
                })
                .collect(),
        }
    }
}

/// Measures every `interval` until the Supervisor stops.
pub(crate) async fn run(supervisor: SupervisorHandle, interval: Duration) {
    let mut sampler = Some(Sampler::new());
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(roots) = supervisor.running_processes().await else {
            return;
        };
        let Some(mut taken) = sampler.take() else {
            return;
        };
        let measured = tokio::task::spawn_blocking(move || {
            let report = taken.sample(&roots);
            (taken, report)
        })
        .await;
        let Ok((returned, report)) = measured else {
            tracing::error!("measuring resources failed");
            return;
        };
        sampler = Some(returned);
        if !supervisor.record_resources(report).await {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: Option<u32>, megabytes: u64, cpu: f32) -> ProcessUse {
        ProcessUse {
            pid,
            parent,
            memory_bytes: megabytes * 1_000_000,
            cpu_percent: cpu,
        }
    }

    #[test]
    fn a_tree_counts_its_root_and_every_descendant_only() {
        let table = [
            process(1, None, 1, 0.0),
            process(10, Some(1), 100, 5.0),
            process(11, Some(10), 50, 20.0),
            process(12, Some(11), 25, 100.0),
            process(20, Some(1), 999, 99.0),
        ];
        assert_eq!(tree_usage(&table, 10), (175_000_000, 1250));
        assert_eq!(tree_usage(&table, 12), (25_000_000, 1000));
    }

    #[test]
    fn a_missing_root_uses_nothing_and_a_cycle_ends() {
        let cycle = [process(5, Some(6), 1, 0.0), process(6, Some(5), 2, 0.0)];
        assert_eq!(tree_usage(&cycle, 5), (3_000_000, 0));
        assert_eq!(tree_usage(&cycle, 99), (0, 0));
    }

    #[test]
    fn this_process_is_measured() {
        let pid = std::process::id();
        let report = Sampler::new().sample(&[(SessionId::new(), pid)]);
        assert!(report.host_memory_total > 0);
        assert!(report.host_memory_used > 0);
        assert!(report.sessions[0].memory_bytes > 0);
    }
}
