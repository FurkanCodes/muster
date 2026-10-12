//! The work done by jobs in a run's container: the building agent's turns,
//! the project's checks, and the reviewer. Each follows its job until it
//! ends, the container stops under it, or the run moves on.

use muster_state::run::{AgentOutcome, Feedback, RunEvent, Verdict};

use super::super::job::{Job, JobState};
use super::super::prompts::{self, Decision, ReviewVerdict};
use super::super::provision;
use super::super::{Report, Reports, RunContext};
use super::{DOCKER_PATIENCE, Halt, Inner, QUICK};
use crate::containers::RuntimeError;
use crate::harness::claude;
use crate::harness::claude_stream::{AgentEvent, Lines, TurnEnd, TurnOutcome, parse_events};

/// The most of the checks' output kept for the agent.
const REPORT_LINES: usize = 200;

/// A process killed outright, as the kernel does when memory runs out.
const KILLED: i32 = 137;

/// How a job ended, with what it wrote.
struct Ended {
    code: i32,
    errors: String,
}

/// Which agent a job runs: only the building agent's conversation is the
/// run's, to resume and to take over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Builder,
    Reviewer,
}

impl Inner {
    pub(super) async fn build(
        &self,
        context: &RunContext,
        generation: u64,
        attempt: u32,
        feedback: Option<&Feedback>,
        reports: &Reports,
    ) -> Result<AgentOutcome, Halt> {
        let container = self.container(context).await?;
        let secret = self.secret().await?;
        let job = Job::named(format!("{generation}-builder"));
        let base_cost = if job.started(&self.docker, &container).await? {
            // Asked again after a restart: it is under way; follow it.
            job.base_cost(&self.docker, &container)
                .await
                .unwrap_or(context.cost_micros)
        } else {
            self.sync(context, &container).await?;
            if context.overtaken(generation) {
                return Err(Halt::Overtaken);
            }
            self.start_builder(
                context, &container, &job, attempt, feedback, &secret, reports,
            )
            .await?;
            context.cost_micros
        };
        let (end, ended) = self
            .follow_agent(
                context,
                generation,
                &container,
                &job,
                reports,
                Role::Builder,
                base_cost,
            )
            .await;
        Ok(agent_outcome(end.as_ref(), &ended?))
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
        secret: &crate::runs::credentials::Secret,
        reports: &Reports,
    ) -> Result<(), Halt> {
        let resumable = match &context.agent_session {
            Some(session) if attempt > 1 => self.has_transcript(container, session).await,
            _ => false,
        };
        let session = match (&context.agent_session, resumable) {
            (Some(session), true) => session.clone(),
            (Some(session), false) if attempt == 1 => session.clone(),
            _ => {
                let session =
                    claude::new_session_id().map_err(|error| Halt::failed(error.to_string()))?;
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
            let base = self.base(container).await?;
            let fence = crate::trackers::new_fence().unwrap_or_else(|_| "fence".to_owned());
            let mut prompt = prompts::builder_first(
                &context.ticket,
                &context.branch,
                &base,
                &context.config.agents.instructions,
                &fence,
            );
            if context.from_branch {
                prompt.push_str(&prompts::carry_on(&base));
            }
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
        job.start(
            &self.docker,
            container,
            &argv,
            &env,
            prompt.as_bytes(),
            context.cost_micros,
        )
        .await?;
        Ok(())
    }

    /// The project's checks on the work, as committed on the run's branch:
    /// whether they passed, and the end of their output.
    pub(super) async fn check(
        &self,
        context: &RunContext,
        generation: u64,
    ) -> Result<(bool, String), Halt> {
        if context.config.checks.commands.is_empty() {
            // Nothing unverified reaches a person (`PRD-02`).
            return Err(Halt::failed(
                "the project names no checks: add `[checks] commands = [\"…\"]` to muster.toml",
            ));
        }
        let container = self.container(context).await?;
        let job = Job::named(format!("{generation}-checks"));
        if !job.started(&self.docker, &container).await? {
            self.sync(context, &container).await?;
            if context.overtaken(generation) {
                return Err(Halt::Overtaken);
            }
            let limit = context.config.checks.timeout.as_secs().max(1).to_string();
            let mut argv = vec!["sh", "-c", CHECKS, "sh", &context.branch, &limit];
            argv.extend(context.config.checks.commands.iter().map(String::as_str));
            job.start(&self.docker, &container, &argv, &[], b"", 0)
                .await?;
        }
        let mut output = Vec::new();
        let ended = self
            .follow(context, generation, &container, &job, &mut |chunk, _| {
                output.extend_from_slice(chunk);
            })
            .await?;
        if ended.code == KILLED {
            return Err(Halt::failed(
                "the checks were killed (exit 137), most likely for want of memory: raise \
                 `memory` in muster.toml",
            ));
        }
        let text = String::from_utf8_lossy(&output);
        let lines: Vec<&str> = text.lines().collect();
        let report = lines[lines.len().saturating_sub(REPORT_LINES)..].join("\n");
        Ok((ended.code == 0, report))
    }

    pub(super) async fn review(
        &self,
        context: &RunContext,
        generation: u64,
        reports: &Reports,
    ) -> Result<Verdict, Halt> {
        let container = self.container(context).await?;
        let secret = self.secret().await?;
        let Some(pr) = context.pr.clone() else {
            return Err(Halt::failed("there is no pull request to review"));
        };
        let base = self.base(&container).await?;
        let mut base_cost = context.cost_micros;
        // A reply without a usable verdict is asked for once more.
        for attempt in 1..=2 {
            if context.overtaken(generation) {
                return Err(Halt::Overtaken);
            }
            let job = Job::named(format!("{generation}-review-{attempt}"));
            if job.started(&self.docker, &container).await? {
                // Followed again after a restart.
                base_cost = job
                    .base_cost(&self.docker, &container)
                    .await
                    .unwrap_or(base_cost);
            } else {
                let fence = crate::trackers::new_fence().unwrap_or_else(|_| "fence".to_owned());
                let prompt = prompts::reviewer(&context.ticket, &context.branch, &base, &fence);
                let session =
                    claude::new_session_id().map_err(|error| Halt::failed(error.to_string()))?;
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
                job.start(
                    &self.docker,
                    &container,
                    &argv,
                    &env,
                    prompt.as_bytes(),
                    base_cost,
                )
                .await?;
            }
            let (end, ended) = self
                .follow_agent(
                    context,
                    generation,
                    &container,
                    &job,
                    reports,
                    Role::Reviewer,
                    base_cost,
                )
                .await;
            let ended = ended?;
            if let Some(cost) = end.as_ref().and_then(|end| end.cost_micros) {
                base_cost += cost;
            }
            let verdict = end
                .and_then(|end| end.result)
                .and_then(|reply| ReviewVerdict::find(&reply));
            let Some(verdict) = verdict else {
                if ended.code == KILLED {
                    return Err(Halt::failed(
                        "the reviewer was killed (exit 137), most likely for want of memory: \
                         raise `memory` in muster.toml",
                    ));
                }
                tracing::info!(run = %context.run, attempt, "the reviewer gave no usable verdict");
                continue;
            };
            // Overtaken meanwhile, the review is not posted: nobody asked
            // for it any more.
            if context.overtaken(generation) {
                return Err(Halt::Overtaken);
            }
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
            return Ok(match verdict.verdict {
                Decision::Approve => Verdict::Approve,
                Decision::RequestChanges => Verdict::RequestChanges {
                    comments: verdict.text(),
                },
            });
        }
        Err(Halt::failed("the reviewer gave no usable verdict, twice"))
    }

    /// Reads a headless agent's output as it comes, reporting its cost and,
    /// for the building agent, its conversation, until it ends.
    #[allow(clippy::too_many_arguments)]
    async fn follow_agent(
        &self,
        context: &RunContext,
        generation: u64,
        container: &str,
        job: &Job,
        reports: &Reports,
        role: Role,
        base_cost: u64,
    ) -> (Option<TurnEnd>, Result<Ended, Halt>) {
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
                            if role == Role::Builder
                                && known_session.as_deref() != Some(&session_id) =>
                        {
                            known_session = Some(session_id.clone());
                            send(Report::AgentSession(session_id));
                        }
                        AgentEvent::TurnFinished(turn) => {
                            if let Some(cost) = turn.cost_micros {
                                send(Report::Event(RunEvent::CostReported {
                                    micros: base_cost + cost,
                                }));
                            }
                            end = Some(turn);
                        }
                        _ => {}
                    }
                }
            }
        };
        let ended = self
            .follow(context, generation, container, job, &mut on_output)
            .await;
        (end, ended)
    }

    /// Looks at a job until it ends, handing on its output; stops when the
    /// run moves on, the container is gone or stopped under it, or docker
    /// stays out of reach.
    async fn follow(
        &self,
        context: &RunContext,
        generation: u64,
        container: &str,
        job: &Job,
        on_output: &mut impl FnMut(&[u8], bool),
    ) -> Result<Ended, Halt> {
        let mut offset: u64 = 0;
        let mut unreachable_since: Option<std::time::Instant> = None;
        loop {
            if context.overtaken(generation) {
                return Err(Halt::Overtaken);
            }
            match job.look(&self.docker, container, offset).await {
                Ok(progress) => {
                    unreachable_since = None;
                    offset += progress.output.len() as u64;
                    match progress.state {
                        JobState::Running => on_output(&progress.output, false),
                        JobState::Ended(code) => {
                            on_output(&progress.output, true);
                            let errors = job.errors(&self.docker, container).await;
                            return Ok(Ended { code, errors });
                        }
                        JobState::Lost => {
                            return Err(Halt::failed(
                                "the work stopped with the run's container (Docker or the \
                                 machine restarted under it)",
                            ));
                        }
                    }
                }
                Err(RuntimeError::NoContainer(_)) => {
                    return Err(Halt::Failed {
                        reason: "the run's container is gone".to_owned(),
                        container_lost: true,
                    });
                }
                Err(RuntimeError::Stopped(_)) => {
                    return Err(Halt::failed("the run's container stopped under its work"));
                }
                Err(error) => {
                    let since = *unreachable_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() > DOCKER_PATIENCE {
                        return Err(Halt::failed(format!(
                            "lost touch with the container: {error}"
                        )));
                    }
                }
            }
            tokio::time::sleep(self.poll).await;
        }
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
        None if ended.code == KILLED => AgentOutcome::Failed {
            reason: "it was killed (exit 137), most likely for want of memory: raise `memory` \
                     in muster.toml"
                .to_owned(),
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

/// The checks: `$1` is the run's branch, `$2` the time limit per command
/// in seconds, the rest the commands. The work must be on the run's
/// branch, which is what is pushed; what the agent left uncommitted there
/// is committed first, so what is checked is what is pushed; a branch with
/// nothing on it fails.
const CHECKS: &str = r#"branch="$1"; limit="$2"; shift 2
base="$(cat /tmp/muster/base)"
cd /work
if [ "$(git symbolic-ref --quiet HEAD)" != "refs/heads/$branch" ]; then
  echo "The work is not on the branch $branch, which is what goes to the pull request:"
  echo "check it out (\`git checkout $branch\`) and commit the work there."
  exit 1
fi
git add --all
git diff --cached --quiet || git commit --quiet --message "Work the agent left uncommitted"
if [ -z "$(git log --oneline "origin/$base..HEAD")" ]; then
  echo "The branch has no commits beyond $base: nothing was changed."
  exit 1
fi
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::SYNC;
    use super::CHECKS;
    use crate::containers::Docker;
    use crate::runs::job::tests::container;

    const LIMIT: Duration = Duration::from_secs(60);

    async fn sh(docker: &Docker, container: &str, script: &str, args: &[&str]) -> (i32, String) {
        let mut argv = vec!["sh", "-c", script, "sh"];
        argv.extend_from_slice(args);
        let output = docker.exec(container, &argv, &[], LIMIT).await.unwrap();
        (
            output.code.unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                output.stderr
            ),
        )
    }

    /// A clone in `/work` of a repository in the container, on `feature`
    /// made from `main`, as provisioning leaves it.
    const SETUP: &str = r#"set -e
git config --global user.name Test && git config --global user.email test@example.com
git config --global init.defaultBranch main
git init -q --bare /tmp/origin.git
git clone -q /tmp/origin.git /work 2>/dev/null
cd /work && echo base > base.txt && git add base.txt && git commit -q -m base
git push -q origin HEAD:main && git fetch -q origin
git checkout -q -b feature
mkdir -p /tmp/muster && printf main > /tmp/muster/base
"#;

    #[tokio::test]
    async fn real_docker_checks_only_the_run_s_branch_with_work_on_it() {
        if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
            return;
        }
        let docker = Docker::default();
        let guard = container(&docker).await;
        let name = guard.0.as_str();
        assert_eq!(sh(&docker, name, SETUP, &[]).await.0, 0);

        let (code, said) = sh(&docker, name, CHECKS, &["feature", "60", "true"]).await;
        assert_eq!(code, 1);
        assert!(
            said.contains("no commits beyond main: nothing was changed"),
            "{said}"
        );

        // Uncommitted work is committed, then checked.
        sh(&docker, name, "cd /work && echo fix > fix.txt", &[]).await;
        let (code, said) = sh(&docker, name, CHECKS, &["feature", "60", "test -f fix.txt"]).await;
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("All checks passed."));

        // Work on another branch is not what goes out: refused, saying so.
        sh(
            &docker,
            name,
            "cd /work && git checkout -q -b elsewhere",
            &[],
        )
        .await;
        let (code, said) = sh(&docker, name, CHECKS, &["feature", "60", "true"]).await;
        assert_eq!(code, 1);
        assert!(
            said.contains("The work is not on the branch feature"),
            "{said}"
        );
        sh(&docker, name, "cd /work && git checkout -q feature", &[]).await;

        let (code, said) = sh(&docker, name, CHECKS, &["feature", "1", "sleep 5"]).await;
        assert_ne!(code, 0);
        assert!(said.contains("(stopped after 1s)"), "{said}");
    }

    #[tokio::test]
    async fn real_docker_takes_in_what_the_branch_gained_elsewhere_or_says_it_conflicts() {
        if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
            return;
        }
        let docker = Docker::default();
        let guard = container(&docker).await;
        let name = guard.0.as_str();
        assert_eq!(sh(&docker, name, SETUP, &[]).await.0, 0);
        // The run's work, pushed; then a suggestion committed on GitHub.
        let pushed = r#"set -e
cd /work && echo run > run.txt && git add run.txt && git commit -q -m run && git push -q origin feature
git clone -q /tmp/origin.git /tmp/elsewhere 2>/dev/null && cd /tmp/elsewhere
git checkout -q feature && echo suggested > suggestion.txt && git add suggestion.txt
git commit -q -m suggestion && git push -q origin feature
"#;
        assert_eq!(sh(&docker, name, pushed, &[]).await.0, 0);
        // More work in the container, some of it left uncommitted.
        sh(&docker, name, "cd /work && echo more >> run.txt", &[]).await;

        let (code, said) = sh(&docker, name, SYNC, &["feature"]).await;
        assert_eq!(code, 0, "{said}");
        let (_, log) = sh(&docker, name, "cd /work && git log --format=%s", &[]).await;
        assert!(
            log.contains("suggestion") && log.contains("Work left uncommitted"),
            "{log}"
        );
        // Nothing new: nothing to do.
        assert_eq!(sh(&docker, name, SYNC, &["feature"]).await.0, 0);

        // A change elsewhere to the same line conflicts: undone, and said.
        let clash = r#"set -e
cd /tmp/elsewhere && git pull -q origin feature 2>/dev/null; echo theirs > run.txt
git commit -q -am theirs && git push -q origin feature
cd /work && echo ours > run.txt && git commit -q -am ours
"#;
        assert_eq!(sh(&docker, name, clash, &[]).await.0, 0);
        let (code, _) = sh(&docker, name, SYNC, &["feature"]).await;
        assert_eq!(code, 3);
        let (_, status) = sh(
            &docker,
            name,
            "cd /work && git status --porcelain && cat run.txt",
            &[],
        )
        .await;
        assert_eq!(status.trim(), "ours", "the merge was undone");
    }
}
