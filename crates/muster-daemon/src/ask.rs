//! Asking a coding agent one question (`muster_core::ask`), to name a chat
//! (`PRODUCT.md` §6): its program run as the person, with their own
//! sign-in (`PRD-03`), in an empty folder of Muster's own so no project's
//! files or instructions are read, its stdin closed so it never waits for
//! more, the variables a session never inherits taken away, within a time
//! limit, and only so much of its answer read. What it is asked and what
//! it answers are never logged (`PRD-06`).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use muster_core::ask::Ask;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// How long an agent has to answer: most answer in seconds.
const ANSWER_LIMIT: Duration = Duration::from_secs(45);

/// How long an agent has to list its models.
const MODELS_LIMIT: Duration = Duration::from_secs(15);

/// The most of an answer read.
const MOST_ANSWER: u64 = 8 * 1024;

/// The most of a list of models read.
const MOST_MODELS: u64 = 64 * 1024;

/// The most models kept from a list.
const MOST_LISTED: usize = 200;

/// Why an agent gave no answer: said without what it was asked.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AskError {
    #[error("{0} is not installed, or not on PATH")]
    Missing(String),
    #[error("{0} did not answer in time")]
    TooSlow(String),
    #[error("{program} failed ({status})")]
    Failed { program: String, status: String },
    #[error("{program} could not be run: {error}")]
    Io {
        program: String,
        error: std::io::Error,
    },
}

/// The empty folder agents are asked in, in the runtime directory
/// `runtime`.
pub(crate) fn folder(runtime: &Path) -> PathBuf {
    runtime.join("ask")
}

/// What the agent `spec` describes answers `question`, with `model` or
/// its own choice, asked in `folder`.
pub(crate) async fn ask(
    spec: &Ask,
    model: Option<&str>,
    question: &str,
    folder: &Path,
) -> Result<String, AskError> {
    let args = spec.arguments(question, model);
    run(&spec.program, &args, folder, ANSWER_LIMIT, MOST_ANSWER).await
}

/// The models the agent `spec` lists, one to a line; `None` where it lists
/// none.
pub(crate) async fn models(spec: &Ask, folder: &Path) -> Option<Result<Vec<String>, AskError>> {
    let args = spec.models.as_ref()?;
    Some(
        run(&spec.program, args, folder, MODELS_LIMIT, MOST_MODELS)
            .await
            .map(|listed| {
                listed
                    .lines()
                    .map(|line| muster_core::text::one_line(line).trim().to_owned())
                    .filter(|line| !line.is_empty())
                    .take(MOST_LISTED)
                    .collect()
            }),
    )
}

async fn run(
    program: &str,
    args: &[String],
    folder: &Path,
    limit: Duration,
    most: u64,
) -> Result<String, AskError> {
    let io = |error| AskError::Io {
        program: program.to_owned(),
        error,
    };
    tokio::fs::create_dir_all(folder).await.map_err(io)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for key in crate::harness::scrub::scrubbed(std::env::vars().map(|(key, _)| key)) {
        command.env_remove(key);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AskError::Missing(program.to_owned())
        } else {
            io(error)
        }
    })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io(std::io::Error::other("no output")))?;
    let answered = async {
        let mut answer = Vec::new();
        (&mut stdout)
            .take(most)
            .read_to_end(&mut answer)
            .await
            .map_err(io)?;
        let status = child.wait().await.map_err(io)?;
        Ok::<_, AskError>((status, answer))
    };
    let (status, answer) = tokio::time::timeout(limit, answered)
        .await
        .map_err(|_| AskError::TooSlow(program.to_owned()))??;
    if !status.success() {
        return Err(AskError::Failed {
            program: program.to_owned(),
            status: status.to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&answer).into_owned())
}
