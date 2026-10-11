//! Runs' work, done for real (`ARCHITECTURE.md` §8): the mirror, the
//! container, headless agents and checks as jobs in it, commits out as a
//! bundle and pushed from the host, the pull request and its review.
//! Each piece runs on a task of its own and reports back; each can be
//! asked for again after a restart and finds what was already started.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::{ProjectId, PullRequest, SessionId};
use muster_state::run::{AgentOutcome, Feedback, RunEffect, RunEvent, Verdict};

use super::credentials::{Credentials, MISSING, Secret};
use super::job::{self, Job};
use super::prompts::{self, Decision, ReviewVerdict};
use super::provision::{self, STATE_DIR};
use super::{Report, Reports, RunContext, RunWork, image, mirror};
use crate::containers::{Docker, LABEL_RUN, RuntimeError};
use crate::githost::{GitHost, PrState};
use crate::harness::claude;
use crate::harness::claude_stream::{AgentEvent, Lines, TurnEnd, TurnOutcome, parse_events};

/// How long a quick question to a container may take.
const QUICK: Duration = Duration::from_secs(60);
/// How long docker may be unreachable before a job's work is given up.
const DOCKER_PATIENCE: Duration = Duration::from_secs(10 * 60);
/// The most of the checks' output kept for the agent.
const REPORT_LINES: usize = 200;

/// Does runs' work with Docker, git and the git host.
#[derive(Clone, Debug)]
pub(crate) struct DockerWork {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    docker: Docker,
    data_dir: PathBuf,
    host: Arc<dyn GitHost>,
    credentials: Arc<dyn Credentials>,
    /// One change to a project's mirror at a time.
    mirrors: Mutex<HashMap<ProjectId, Arc<tokio::sync::Mutex<()>>>>,
    claude_version: tokio::sync::OnceCell<String>,
    /// How often a job's output is looked at.
    poll: Duration,
    /// An image to use instead of building one (`MUSTER_RUN_IMAGE`).
    image: Option<String>,
}

impl DockerWork {
    /// The work, with an `image` to use instead of building one, if given.
    pub(crate) fn new(
        data_dir: PathBuf,
        host: Arc<dyn GitHost>,
        credentials: Arc<dyn Credentials>,
        poll: Duration,
        image: Option<String>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                docker: Docker::default(),
                data_dir,
                host,
                credentials,
                mirrors: Mutex::new(HashMap::new()),
                claude_version: tokio::sync::OnceCell::new(),
                poll,
                image,
            }),
        }
    }
}

impl RunWork for DockerWork {
    fn start(&self, context: RunContext, effect: RunEffect, reports: Reports) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let run = context.run;
            let send = |report| {
                let _ = reports.send((run, report));
            };
            match effect {
                RunEffect::StartContainer { generation } => {
                    match inner.start_container(&context).await {
                        Ok(container) => {
                            send(Report::Container(container));
                            send(Report::Event(RunEvent::ContainerReady { generation }));
                        }
                        Err(reason) => send(failed(generation, reason, false)),
                    }
                }
                RunEffect::StartBuilder {
                    generation,
                    attempt,
                    feedback,
                } => {
                    inner
                        .build(&context, generation, attempt, feedback.as_ref(), &reports)
                        .await;
                }
                RunEffect::RunChecks { generation } => {
                    inner.check(&context, generation, &reports).await
                }
                RunEffect::PublishBranch { generation } => {
                    match inner.publish(&context, generation).await {
                        Ok(pr) => {
                            let number = pr.number;
                            send(Report::PullRequest(pr));
                            send(Report::Event(RunEvent::PrOpened { generation, number }));
                        }
                        Err(reason) => send(failed(generation, reason, false)),
                    }
                }
                RunEffect::StartReviewer { generation } => {
                    inner.review(&context, generation, &reports).await;
                }
                RunEffect::MarkReady => inner.mark(&context, true).await,
                RunEffect::MarkDraft => inner.mark(&context, false).await,
                RunEffect::StopWork { before } => {
                    if let Some(container) = &context.container
                        && let Err(error) = job::stop_before(&inner.docker, container, before).await
                    {
                        tracing::warn!(%run, %error, "could not stop a run's work");
                    }
                }
                RunEffect::StartInteractive => {
                    // The Supervisor starts the person's session itself.
                    tracing::warn!(%run, "asked to start an agent for the person");
                }
                RunEffect::RemoveContainer => {
                    let container = context
                        .container
                        .clone()
                        .unwrap_or_else(|| provision::container_name(run));
                    if let Err(error) = inner.docker.remove(&container).await {
                        tracing::warn!(%run, %error, "could not remove a run's container");
                    }
                }
                RunEffect::Transition { .. } => {}
            }
        });
    }

    fn stop_session(&self, container: String, session: SessionId) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let script = format!(
                "{}{}",
                crate::harness::IN_CONTAINER,
                crate::harness::STOP_IN_CONTAINER
            );
            let session = session.to_string();
            let stopped = inner
                .docker
                .exec(
                    &container,
                    &["sh", "-c", &script, "sh", &session],
                    &[],
                    QUICK,
                )
                .await;
            if let Err(error) = stopped {
                tracing::debug!(%session, %error, "could not stop a session in its container");
            }
        });
    }

    fn check_pull_request(&self, context: RunContext, reports: Reports) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let Some(pr) = &context.pr else {
                return;
            };
            match inner.host.state(&context.root, pr.number).await {
                Ok(PrState::Merged) => {
                    let _ = reports.send((context.run, Report::Event(RunEvent::PrMerged)));
                }
                Ok(PrState::Closed) => {
                    let _ = reports.send((context.run, Report::Event(RunEvent::PrClosed)));
                }
                Ok(PrState::Open) => {}
                Err(error) => {
                    tracing::debug!(run = %context.run, %error, "could not look at a pull request")
                }
            }
        });
    }
}

fn failed(generation: u64, reason: String, container_lost: bool) -> Report {
    Report::Event(RunEvent::InfraFailed {
        generation,
        reason,
        container_lost,
    })
}

/// How a job ended, with what it wrote.
struct Ended {
    code: i32,
    errors: String,
}

impl Inner {
    fn mirror_lock(&self, project: ProjectId) -> Arc<tokio::sync::Mutex<()>> {
        let mut mirrors = self
            .mirrors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirrors.entry(project).or_default().clone()
    }

    async fn start_container(&self, context: &RunContext) -> Result<String, String> {
        let mirror = {
            let lock = self.mirror_lock(context.project);
            let _held = lock.lock().await;
            mirror::refresh(&self.data_dir, context.project, &context.root)
                .await
                .map_err(|error| {
                    format!("could not update Muster's copy of the repository: {error}")
                })?
        };
        let image = match &self.image {
            Some(image) => image.clone(),
            None => {
                let version = self.claude_version.get_or_init(image::claude_version).await;
                image::image_for(
                    &self.docker,
                    &context.root,
                    context.config.dockerfile.as_deref(),
                    version,
                )
                .await?
            }
        };
        let identity = provision::identity(&context.root).await;
        let provisioned =
            provision::provision(&self.docker, context, &mirror, &image, &identity).await?;
        Ok(provisioned.container)
    }

    /// The run's container, running; `Err` says why there is none.
    async fn container(&self, context: &RunContext) -> Result<String, (String, bool)> {
        let lost = || ("the run's container is gone".to_owned(), true);
        let Some(container) = context.container.clone() else {
            return Err(lost());
        };
        let found = self
            .docker
            .find(LABEL_RUN, &context.run.to_string())
            .await
            .map_err(|error| (error.to_string(), false))?;
        match found.into_iter().find(|candidate| {
            container.starts_with(&candidate.id) || candidate.id.starts_with(&container)
        }) {
            Some(running) if running.running => Ok(container),
            Some(_) => {
                self.docker
                    .restart(&container)
                    .await
                    .map_err(|error| (error.to_string(), false))?;
                Ok(container)
            }
            None => Err(lost()),
        }
    }

    async fn secret(&self) -> Result<Secret, String> {
        self.credentials
            .claude()
            .await
            .ok_or_else(|| MISSING.to_owned())
    }

    async fn build(
        &self,
        context: &RunContext,
        generation: u64,
        attempt: u32,
        feedback: Option<&Feedback>,
        reports: &Reports,
    ) {
        let send = |report| {
            let _ = reports.send((context.run, report));
        };
        let container = match self.container(context).await {
            Ok(container) => container,
            Err((reason, lost)) => return send(failed(generation, reason, lost)),
        };
        let secret = match self.secret().await {
            Ok(secret) => secret,
            Err(reason) => return send(failed(generation, reason, false)),
        };
        let job = Job::named(format!("{generation}-builder"));
        match job.started(&self.docker, &container).await {
            // Asked again after a restart: it is under way; follow it.
            Ok(true) => {}
            Ok(false) => {
                if let Err(reason) = self
                    .start_builder(
                        context, &container, &job, attempt, feedback, &secret, reports,
                    )
                    .await
                {
                    return send(failed(generation, reason, false));
                }
            }
            Err(error) => return send(failed(generation, error.to_string(), false)),
        }
        let (end, outcome) = self.follow_agent(context, &container, &job, reports).await;
        let outcome = match outcome {
            Err((reason, lost)) => return send(failed(generation, reason, lost)),
            Ok(ended) => agent_outcome(end.as_ref(), &ended),
        };
        send(Report::Event(RunEvent::AgentFinished {
            generation,
            outcome,
        }));
    }

    /// Starts the building agent's turn: in its conversation where the
    /// container still has it; otherwise (a first attempt, a new container)
    /// in a new one, given everything.
    #[allow(clippy::too_many_arguments)]
    async fn start_builder(
        &self,
        context: &RunContext,
        container: &str,
        job: &Job,
        attempt: u32,
        feedback: Option<&Feedback>,
        secret: &Secret,
        reports: &Reports,
    ) -> Result<(), String> {
        let container = container.to_owned();
        let resumable = match &context.agent_session {
            Some(session) if attempt > 1 => self.has_transcript(&container, session).await,
            _ => false,
        };
        let session = match (&context.agent_session, resumable) {
            (Some(session), true) => session.clone(),
            (Some(session), false) if attempt == 1 => session.clone(),
            _ => {
                let session = claude::new_session_id().map_err(|error| error.to_string())?;
                let _ = reports.send((context.run, Report::AgentSession(session.clone())));
                session
            }
        };
        let prompt = if resumable {
            feedback.map_or_else(
                || "Carry on with the work, then commit and reply with a short summary.".to_owned(),
                prompts::builder_again,
            )
        } else {
            let base = self.base(&container).await;
            let fence = crate::github::new_fence().unwrap_or_else(|_| "fence".to_owned());
            let mut prompt = prompts::builder_first(
                &context.ticket,
                &context.branch,
                &base,
                &context.config.agents.instructions,
                &fence,
            );
            if let Some(feedback) = feedback {
                prompt.push_str("\n\nThis is not the first attempt. ");
                prompt.push_str(&prompts::builder_again(feedback));
            }
            prompt
        };
        let session_flag = if resumable {
            "--resume"
        } else {
            "--session-id"
        };
        let argv = [
            "claude",
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--dangerously-skip-permissions",
            session_flag,
            &session,
        ];
        let env = [(secret.variable, secret.value())];
        job.start(&self.docker, &container, &argv, &env, prompt.as_bytes())
            .await
            .map_err(|error| error.to_string())
    }

    /// Reads a headless agent's output as it comes, reporting its session
    /// and cost, until it ends.
    async fn follow_agent(
        &self,
        context: &RunContext,
        container: &str,
        job: &Job,
        reports: &Reports,
    ) -> (Option<TurnEnd>, Result<Ended, (String, bool)>) {
        let send = |report| {
            let _ = reports.send((context.run, report));
        };
        let mut lines = Lines::default();
        let mut end = None;
        let mut known_session = context.agent_session.clone();
        let mut on_output = |output: &[u8], finished: bool| {
            let mut whole = lines.push(output);
            if finished {
                whole.extend(lines.finish());
            }
            for line in whole {
                for event in parse_events(&line) {
                    match event {
                        AgentEvent::SessionStarted { session_id, .. }
                            if known_session.as_deref() != Some(&session_id) =>
                        {
                            known_session = Some(session_id.clone());
                            send(Report::AgentSession(session_id));
                        }
                        AgentEvent::TurnFinished(turn) => {
                            if let Some(cost) = turn.cost_micros {
                                send(Report::Event(RunEvent::CostReported {
                                    micros: context.cost_micros + cost,
                                }));
                            }
                            end = Some(turn);
                        }
                        _ => {}
                    }
                }
            }
        };
        let ended = self.follow(container, job, &mut on_output).await;
        (end, ended)
    }

    /// Looks at a job until it ends, handing on its output.
    async fn follow(
        &self,
        container: &str,
        job: &Job,
        on_output: &mut impl FnMut(&[u8], bool),
    ) -> Result<Ended, (String, bool)> {
        let mut offset: u64 = 0;
        let mut unreachable_since: Option<std::time::Instant> = None;
        loop {
            match job.look(&self.docker, container, offset).await {
                Ok(progress) => {
                    unreachable_since = None;
                    offset += progress.output.len() as u64;
                    let finished = progress.exit.is_some();
                    if let Some(code) = progress.exit {
                        // The exit code is written last: what is left is all.
                        let rest = job
                            .look(&self.docker, container, offset)
                            .await
                            .map(|rest| rest.output)
                            .unwrap_or_default();
                        let mut all = progress.output;
                        all.extend(rest);
                        on_output(&all, finished);
                        let errors = job.errors(&self.docker, container).await;
                        return Ok(Ended { code, errors });
                    }
                    on_output(&progress.output, false);
                }
                Err(RuntimeError::Failed { stderr, .. })
                    if stderr.contains("No such container") =>
                {
                    return Err(("the run's container is gone".to_owned(), true));
                }
                Err(error) => {
                    let since = *unreachable_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() > DOCKER_PATIENCE {
                        return Err((format!("lost touch with the container: {error}"), false));
                    }
                }
            }
            tokio::time::sleep(self.poll).await;
        }
    }

    async fn check(&self, context: &RunContext, generation: u64, reports: &Reports) {
        let send = |report| {
            let _ = reports.send((context.run, report));
        };
        let container = match self.container(context).await {
            Ok(container) => container,
            Err((reason, lost)) => return send(failed(generation, reason, lost)),
        };
        let job = Job::named(format!("{generation}-checks"));
        let limit = context.config.checks.timeout.as_secs().max(1).to_string();
        let mut argv = vec!["sh", "-c", CHECKS, "sh", &limit];
        argv.extend(context.config.checks.commands.iter().map(String::as_str));
        if let Err(error) = job.start(&self.docker, &container, &argv, &[], b"").await {
            return send(failed(generation, error.to_string(), false));
        }
        let mut output = Vec::new();
        let ended = self
            .follow(&container, &job, &mut |chunk, _| {
                output.extend_from_slice(chunk)
            })
            .await;
        match ended {
            Err((reason, lost)) => send(failed(generation, reason, lost)),
            Ok(ended) => {
                let text = String::from_utf8_lossy(&output);
                let lines: Vec<&str> = text.lines().collect();
                let report = lines[lines.len().saturating_sub(REPORT_LINES)..].join("\n");
                send(Report::Event(RunEvent::ChecksFinished {
                    generation,
                    passed: ended.code == 0,
                    report,
                }));
            }
        }
    }

    async fn publish(&self, context: &RunContext, generation: u64) -> Result<PullRequest, String> {
        let container = self
            .container(context)
            .await
            .map_err(|(reason, _)| reason)?;
        let base = self.base(&container).await;
        let branch = &context.branch;
        let bundle = self
            .docker
            .exec(
                &container,
                &["sh", "-c", BUNDLE, "sh", branch, &base],
                &[],
                QUICK * 5,
            )
            .await
            .map_err(|error| error.to_string())?;
        if !bundle.success() || bundle.stdout.is_empty() {
            return Err(format!(
                "could not take the commits out of the container: {}",
                provision::last_lines(&bundle.stderr, 4)
            ));
        }
        let file = self
            .data_dir
            .join("bundles")
            .join(format!("{}-{generation}.bundle", context.run));
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| error.to_string())?;
        }
        tokio::fs::write(&file, &bundle.stdout)
            .await
            .map_err(|error| error.to_string())?;
        let pushed = async {
            let lock = self.mirror_lock(context.project);
            let _held = lock.lock().await;
            let mirror = mirror::Mirror {
                path: mirror::path(&self.data_dir, context.project),
            };
            mirror.take_bundle(&file, branch).await?;
            mirror.push(branch).await
        }
        .await;
        let _ = tokio::fs::remove_file(&file).await;
        pushed.map_err(|error| format!("could not push {branch}: {error}"))?;
        let title = format!("{} (#{})", context.ticket.title, context.ticket.number);
        let body = format!(
            "Closes #{number}.\n\nMade by a Muster run on {url}; the project's checks passed:\n\n{checks}\n\nA reviewer agent reviews it next; a person decides.",
            number = context.ticket.number,
            url = context.ticket.url,
            checks = context
                .config
                .checks
                .commands
                .iter()
                .map(|command| format!("- `{command}`"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        self.host
            .open_draft(&context.root, branch, &base, &title, &body)
            .await
            .map_err(|error| format!("could not open the pull request: {error}"))
    }

    async fn review(&self, context: &RunContext, generation: u64, reports: &Reports) {
        let send = |report| {
            let _ = reports.send((context.run, report));
        };
        let container = match self.container(context).await {
            Ok(container) => container,
            Err((reason, lost)) => return send(failed(generation, reason, lost)),
        };
        let secret = match self.secret().await {
            Ok(secret) => secret,
            Err(reason) => return send(failed(generation, reason, false)),
        };
        let Some(pr) = context.pr.clone() else {
            return send(failed(
                generation,
                "there is no pull request to review".to_owned(),
                false,
            ));
        };
        let base = self.base(&container).await;
        // A reply without a usable verdict is asked for once more.
        for attempt in 1..=2 {
            let job = Job::named(format!("{generation}-review-{attempt}"));
            let fence = crate::github::new_fence().unwrap_or_else(|_| "fence".to_owned());
            let prompt = prompts::reviewer(&context.ticket, &context.branch, &base, &fence);
            let session = match claude::new_session_id() {
                Ok(session) => session,
                Err(error) => return send(failed(generation, error.to_string(), false)),
            };
            // Started already (asked again after a restart) or not, a job
            // starts once.
            let argv = [
                "claude",
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--session-id",
                &session,
                "--allowedTools",
                "Read",
                "Grep",
                "Glob",
                "Bash(git diff:*)",
                "Bash(git log:*)",
                "Bash(git show:*)",
                "Bash(git status:*)",
            ];
            let env = [(secret.variable, secret.value())];
            if let Err(error) = job
                .start(&self.docker, &container, &argv, &env, prompt.as_bytes())
                .await
            {
                return send(failed(generation, error.to_string(), false));
            }
            let reviewer_context = RunContext {
                // The reviewer's session is its own, not the builder's.
                agent_session: Some(session),
                ..context.clone()
            };
            let (end, ended) = self
                .follow_agent(&reviewer_context, &container, &job, reports)
                .await;
            if let Err((reason, lost)) = ended {
                return send(failed(generation, reason, lost));
            }
            let verdict = end
                .and_then(|end| end.result)
                .and_then(|reply| ReviewVerdict::find(&reply));
            let Some(verdict) = verdict else {
                tracing::info!(run = %context.run, attempt, "the reviewer gave no usable verdict");
                continue;
            };
            let heading = match verdict.verdict {
                Decision::Approve => "**Muster's reviewer agent approves this change.**",
                Decision::RequestChanges => "**Muster's reviewer agent asks for changes.**",
            };
            let body = format!("{heading}\n\n{}", verdict.text());
            if let Err(error) = self
                .host
                .review_comment(&context.root, pr.number, &body)
                .await
            {
                // The verdict still counts; GitHub only did not get it.
                tracing::warn!(run = %context.run, %error, "could not post the review");
            }
            let verdict = match verdict.verdict {
                Decision::Approve => Verdict::Approve,
                Decision::RequestChanges => Verdict::RequestChanges {
                    comments: verdict.text(),
                },
            };
            return send(Report::Event(RunEvent::ReviewSubmitted {
                generation,
                verdict,
            }));
        }
        send(failed(
            generation,
            "the reviewer gave no usable verdict, twice".to_owned(),
            false,
        ));
    }

    async fn mark(&self, context: &RunContext, ready: bool) {
        let Some(pr) = &context.pr else {
            return;
        };
        let marked = if ready {
            self.host.mark_ready(&context.root, pr.number).await
        } else {
            self.host.mark_draft(&context.root, pr.number).await
        };
        if let Err(error) = marked {
            tracing::warn!(run = %context.run, %error, ready, "could not change the pull request");
        }
    }

    /// The branch the run started from, as provisioning recorded it.
    async fn base(&self, container: &str) -> String {
        let file = format!("{STATE_DIR}/base");
        self.docker
            .exec(container, &["cat", &file], &[], QUICK)
            .await
            .ok()
            .filter(crate::containers::ExecOutput::success)
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .filter(|base| !base.is_empty())
            .unwrap_or_else(|| "main".to_owned())
    }

    /// Whether the container keeps the transcript of `session`, which
    /// resuming it needs.
    async fn has_transcript(&self, container: &str, session: &str) -> bool {
        self.docker
            .exec(
                container,
                &[
                    "sh",
                    "-c",
                    "ls \"$HOME\"/.claude/projects/*/\"$1\".jsonl >/dev/null 2>&1",
                    "sh",
                    session,
                ],
                &[],
                QUICK,
            )
            .await
            .is_ok_and(|output| output.success())
    }
}

/// How an agent's turn went, from its last result and its exit.
fn agent_outcome(end: Option<&TurnEnd>, ended: &Ended) -> AgentOutcome {
    match end.map(|end| &end.outcome) {
        Some(TurnOutcome::Success) => AgentOutcome::Done,
        Some(TurnOutcome::MaxTurns) => AgentOutcome::Failed {
            reason: "it ran out of turns".to_owned(),
        },
        Some(TurnOutcome::Error { message, .. }) => AgentOutcome::Failed {
            reason: message.clone(),
        },
        None => AgentOutcome::Failed {
            reason: format!(
                "it exited with code {} without finishing its turn: {}",
                ended.code,
                provision::last_lines(&ended.errors, 4)
            ),
        },
    }
}

/// The checks: `$1` is the time limit per command in seconds, the rest the
/// commands. Work the agent left uncommitted is committed first, so what
/// is checked is what is pushed; a branch with nothing on it fails.
const CHECKS: &str = r#"base="$(cat /tmp/muster/base)"
cd /work
git add --all
git diff --cached --quiet || git commit --quiet --message "Work the agent left uncommitted"
if [ -z "$(git log --oneline "origin/$base..HEAD")" ]; then
  echo "The branch has no commits beyond $base: nothing was changed."
  exit 1
fi
limit="$1"; shift
for command in "$@"; do
  echo "\$ $command"
  timeout "$limit" sh -c "$command" 2>&1
  code=$?
  if [ "$code" -ne 0 ]; then
    if [ "$code" -eq 124 ] || [ "$code" -eq 143 ]; then echo "(stopped after ${limit}s)"; else echo "(exit $code)"; fi
    exit "$code"
  fi
done
echo "All checks passed."
"#;

/// The run's commits as a bundle on stdout: `$1` the branch, `$2` the base.
const BUNDLE: &str = r#"cd /work
git add --all
git diff --cached --quiet || git commit --quiet --message "Work the agent left uncommitted"
git bundle create - "refs/heads/$1" "^origin/$2"
"#;

#[cfg(test)]
mod tests;
