//! Plugins' hooks, run (`PRD-15`, `ARC-23`, `ARCHITECTURE.md` §7a): each
//! event the Supervisor hands over, for each hook that takes it, is a run
//! of the hook's program, told of the event, under the task tracker
//! (`ARC-13`). Bounded every way (`ARC-14`): at most four of a plugin's
//! run at once and 32 wait, the oldest dropped with a line in its log;
//! each run has its deadline, its output is capped and appended to its
//! plugin's log; a plugin turned off, gone or no longer approved has what
//! waits dropped and what runs cut. As the daemon stops or restarts in
//! place, runs under way have a grace, then are cut, each said in the log.

mod run;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use muster_core::plugins::{Event, Hook};
use muster_core::protocol::plugins::{HookOutcome, HookRun};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::sync::{Notify, oneshot};

use crate::secrets::Secrets;
use crate::tasks::Tasks;
use crate::trackers::plugin::streams::Log;

/// How many of a plugin's hooks run at once, and wait, and how much of
/// what one says is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) running: usize,
    pub(crate) waiting: usize,
    /// The most of a run's stdout and stderr kept, together.
    pub(crate) output: usize,
    /// How long a run past its deadline has, told to stop, before it is
    /// killed.
    pub(crate) stop_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            running: 4,
            waiting: 32,
            output: 16 * 1024,
            stop_grace: Duration::from_secs(1),
        }
    }
}

/// One hook to run for one event.
#[derive(Debug)]
pub(crate) struct Job {
    pub(crate) plugin: String,
    pub(crate) hook: Hook,
    pub(crate) event: Event,
    /// The plugin's folder, where its hooks run.
    pub(crate) folder: PathBuf,
    /// Its manifest's SHA-256 as approved: a manifest that no longer
    /// hashes so runs nothing (`PRD-15`).
    pub(crate) sha256: String,
    /// The person's variables its manifest names.
    pub(crate) env: Vec<String>,
    /// The secrets it names.
    pub(crate) secrets: Vec<String>,
    /// Why it cannot run, found as it was raised: a program it needs is
    /// not on `PATH`.
    pub(crate) refused: Option<String>,
    /// Told what it came to, for `muster plugin hook`.
    pub(crate) done: Option<oneshot::Sender<HookRun>>,
}

impl Job {
    /// Ends it unrun, saying why to its plugin's log and whoever waits.
    fn not_run(self, why: String, log: &Mutex<Log>) {
        let outcome = HookOutcome::NotRun { why };
        lock(log).push(run::entry(&self.hook.id, self.event.event, &outcome, 0, "").as_bytes());
        if let Some(done) = self.done {
            let _ = done.send(HookRun {
                hook: self.hook.id,
                outcome,
                output: String::new(),
                millis: 0,
            });
        }
    }
}

/// The hooks' runs, shared by the Supervisor, which hands them events,
/// and the daemon, which cuts them as it stops.
#[derive(Clone, Debug)]
pub(crate) struct Hooks {
    held: Arc<Mutex<Held>>,
    /// Told each time a run ends.
    ended: Arc<Notify>,
    tasks: Tasks,
    /// Each plugin's log, shared with its ticket process.
    logs: crate::trackers::plugin::Plugins,
    secrets: Arc<dyn Secrets>,
    limits: Limits,
}

#[derive(Debug, Default)]
struct Held {
    /// Each plugin's runs, by its id.
    plugins: HashMap<String, Queue>,
    /// The key the last run was given.
    next: u64,
    /// Stopping: nothing more runs.
    closed: bool,
}

#[derive(Debug, Default)]
struct Queue {
    running: HashMap<u64, Running>,
    waiting: VecDeque<Job>,
}

/// A run under way.
#[derive(Debug)]
struct Running {
    hook: String,
    /// Its process group, once started.
    group: Option<Pid>,
    /// Set as it is cut, so it is told as cut, not as ended by a signal.
    cut: Arc<AtomicBool>,
}

impl Hooks {
    pub(crate) fn new(
        tasks: Tasks,
        logs: crate::trackers::plugin::Plugins,
        secrets: Arc<dyn Secrets>,
    ) -> Self {
        Self {
            held: Arc::default(),
            ended: Arc::default(),
            tasks,
            logs,
            secrets,
            limits: Limits::default(),
        }
    }

    /// Hooks no test raises, for a Supervisor under test.
    #[cfg(test)]
    pub(crate) fn idle() -> Self {
        Self::new(
            Tasks::default(),
            crate::trackers::plugin::Plugins::new("muster".into()),
            Arc::new(crate::secrets::Kept::default()),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `jobs`, or keeps them waiting their plugin's turn.
    pub(crate) fn raise(&self, jobs: Vec<Job>) {
        for job in jobs {
            self.take(job);
        }
    }

    fn take(&self, job: Job) {
        let log = self.logs.log_of(&job.plugin);
        if let Some(why) = job.refused.clone() {
            return job.not_run(why, &log);
        }
        let mut held = self.held();
        if held.closed {
            drop(held);
            return job.not_run("Muster is stopping".to_owned(), &log);
        }
        let queue = held.plugins.entry(job.plugin.clone()).or_default();
        if queue.running.len() < self.limits.running {
            self.start(&mut held, job);
            return;
        }
        queue.waiting.push_back(job);
        if queue.waiting.len() > self.limits.waiting
            && let Some(oldest) = queue.waiting.pop_front()
        {
            drop(held);
            tracing::warn!(plugin = %oldest.plugin, hook = %oldest.hook.id, "dropped a hook's run: too many waited");
            let why = format!(
                "dropped: {} of its plugin's runs waited already",
                self.limits.waiting
            );
            oldest.not_run(why, &log);
        }
    }

    /// Starts `job` now, under the task tracker.
    fn start(&self, held: &mut Held, job: Job) {
        held.next += 1;
        let key = held.next;
        let cut = Arc::new(AtomicBool::new(false));
        let plugin = job.plugin.clone();
        held.plugins
            .entry(plugin.clone())
            .or_default()
            .running
            .insert(
                key,
                Running {
                    hook: job.hook.id.clone(),
                    group: None,
                    cut: cut.clone(),
                },
            );
        let hooks = self.clone();
        self.tasks.spawn(async move {
            let log = hooks.logs.log_of(&plugin);
            let started = |group: Pid| hooks.started(&plugin, key, group);
            let given = run::Given {
                secrets: hooks.secrets.as_ref(),
                limits: hooks.limits,
                log: &log,
                cut: &cut,
                started: &started,
            };
            run::run(job, &given).await;
            hooks.finished(&plugin, key);
        });
    }

    /// The run `key` of `plugin` started its process group `group`.
    fn started(&self, plugin: &str, key: u64, group: Pid) {
        let mut held = self.held();
        if let Some(running) = held
            .plugins
            .get_mut(plugin)
            .and_then(|queue| queue.running.get_mut(&key))
        {
            running.group = Some(group);
            // Cut before its process started: it goes at once.
            if running.cut.load(Ordering::SeqCst) {
                let _ = killpg(group, Signal::SIGKILL);
            }
        }
    }

    /// The run `key` of `plugin` ended: the next waiting starts.
    fn finished(&self, plugin: &str, key: u64) {
        let mut held = self.held();
        let closed = held.closed;
        let next = held.plugins.get_mut(plugin).and_then(|queue| {
            queue.running.remove(&key);
            if closed {
                None
            } else {
                queue.waiting.pop_front()
            }
        });
        match next {
            Some(job) => self.start(&mut held, job),
            None => {
                if held
                    .plugins
                    .get(plugin)
                    .is_some_and(|queue| queue.running.is_empty() && queue.waiting.is_empty())
                {
                    held.plugins.remove(plugin);
                }
            }
        }
        drop(held);
        self.ended.notify_waiters();
    }

    /// Takes `runnable` as the plugins whose hooks may run, as the registry
    /// reads now: every other's waiting runs are dropped and its runs under
    /// way cut, as a plugin turned off, removed or changed since its
    /// approval leaves with all it runs (`ARC-14`).
    pub(crate) fn publish(&self, runnable: &[String]) {
        let mut held = self.held();
        let gone: Vec<String> = held
            .plugins
            .keys()
            .filter(|plugin| !runnable.contains(plugin))
            .cloned()
            .collect();
        let mut dropped = Vec::new();
        for plugin in gone {
            let Some(queue) = held.plugins.get_mut(&plugin) else {
                continue;
            };
            dropped.extend(queue.waiting.drain(..));
            for running in queue.running.values() {
                cut(&plugin, running, "its plugin can no longer run");
            }
        }
        drop(held);
        for job in dropped {
            let log = self.logs.log_of(&job.plugin);
            job.not_run("dropped: its plugin can no longer run".to_owned(), &log);
        }
    }

    /// As the daemon stops or restarts in place: nothing more runs, what
    /// waits is dropped, and runs under way have `grace` to end before
    /// they are cut, each said in the daemon's log and its plugin's.
    pub(crate) async fn cut(&self, grace: Duration) {
        let dropped: Vec<Job> = {
            let mut held = self.held();
            held.closed = true;
            held.plugins
                .values_mut()
                .flat_map(|queue| queue.waiting.drain(..))
                .collect()
        };
        for job in dropped {
            let log = self.logs.log_of(&job.plugin);
            job.not_run("dropped: Muster is stopping".to_owned(), &log);
        }
        let _ = tokio::time::timeout(grace, async {
            loop {
                let ended = self.ended.notified();
                if self.running() == 0 {
                    return;
                }
                ended.await;
            }
        })
        .await;
        let held = self.held();
        for (plugin, queue) in &held.plugins {
            for running in queue.running.values() {
                cut(plugin, running, "Muster stopped or restarted in place");
            }
        }
    }

    /// Runs again after a restart in place that did not happen.
    pub(crate) fn reopen(&self) {
        self.held().closed = false;
    }

    /// How many runs are under way.
    pub(crate) fn running(&self) -> usize {
        self.held()
            .plugins
            .values()
            .map(|queue| queue.running.len())
            .sum()
    }

    /// How many runs wait, for tests.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.held()
            .plugins
            .values()
            .map(|queue| queue.waiting.len())
            .sum()
    }
}

/// Cuts `running`, a run of `plugin`'s, saying why in the daemon's log.
fn cut(plugin: &str, running: &Running, why: &str) {
    running.cut.store(true, Ordering::SeqCst);
    tracing::warn!(plugin, hook = %running.hook, why, "cut a hook's run");
    if let Some(group) = running.group {
        let _ = killpg(group, Signal::SIGKILL);
    }
}

fn lock(log: &Mutex<Log>) -> std::sync::MutexGuard<'_, Log> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
