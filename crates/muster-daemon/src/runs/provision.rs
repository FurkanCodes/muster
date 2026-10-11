//! Giving a run its container (`ARCHITECTURE.md` §8): the right image, a
//! container with the run's memory as its limit and the mirror mounted
//! read-only, and inside it a clone of the mirror on the run's branch.
//! Everything here can be done again: a container is found by its label
//! before one is made, a clone is made once, and a branch that already
//! has the run's work is not reset.

use std::path::Path;
use std::time::Duration;

use muster_core::RunId;

use super::RunContext;
use super::mirror::Mirror;
use crate::containers::{ContainerSpec, Docker, LABEL_RUN, Mount, MountSource};

/// Where, inside a container, the run's work keeps its state: prompts in,
/// output and exit codes out. Under `/tmp`, which any image lets anyone
/// write.
pub(crate) const STATE_DIR: &str = "/tmp/muster";
/// The project the container belongs to, for clearing its cache later.
const LABEL_PROJECT: &str = "muster.project";

/// How long making the clone may take.
const SETUP_LIMIT: Duration = Duration::from_secs(15 * 60);

/// A container ready for work, and the branch runs start from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Provisioned {
    pub(crate) container: String,
    pub(crate) base: String,
}

/// The container's name, one per run.
pub(crate) fn container_name(run: RunId) -> String {
    format!("muster-run-{}", run.ulid().to_string().to_lowercase())
}

/// The commit author for the run's work: the person's own git identity
/// in the project, as when they commit there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) name: String,
    pub(crate) email: String,
}

/// Makes, or finds again, the run's container on `image` and its clone.
pub(crate) async fn provision(
    docker: &Docker,
    context: &RunContext,
    mirror: &Mirror,
    image: &str,
    identity: &Identity,
) -> Result<Provisioned, String> {
    let base = mirror
        .base_branch(context.config.base_branch.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    if context.branch == base {
        // A run pushes its branch and asks to merge it into the base.
        return Err(format!(
            "{base} is the branch runs start from; a run needs a branch of its own"
        ));
    }
    let run = context.run.to_string();
    let found = docker
        .find(LABEL_RUN, &run)
        .await
        .map_err(|error| error.to_string())?;
    let container = match found.into_iter().next() {
        Some(existing) if existing.running => existing.id,
        Some(existing) => {
            docker
                .restart(&existing.id)
                .await
                .map_err(|error| error.to_string())?;
            existing.id
        }
        None => docker
            .start(&spec(context, mirror, image))
            .await
            .map_err(|error| error.to_string())?,
    };
    let output = docker
        .exec(
            &container,
            &["sh", "-c", SETUP],
            &[
                ("BRANCH", context.branch.as_str()),
                ("BASE", base.as_str()),
                ("GIT_NAME", identity.name.as_str()),
                ("GIT_EMAIL", identity.email.as_str()),
            ],
            SETUP_LIMIT,
        )
        .await
        .map_err(|error| error.to_string())?;
    if !output.success() {
        return Err(format!(
            "could not check out the code in the container: {}",
            last_lines(&output.stderr, 6)
        ));
    }
    Ok(Provisioned { container, base })
}

/// Clones the mirror once, checks the run's branch out unless it already
/// is, and makes the state directory. Values come in the environment.
const SETUP: &str = r#"set -e
mkdir -p /tmp/muster/out /tmp/muster/in
git config --global --add safe.directory '*'
git config --global user.name "$GIT_NAME"
git config --global user.email "$GIT_EMAIL"
if [ ! -d /work/.git ]; then git clone --quiet /mirror /work; fi
cd /work
git fetch --quiet origin
if [ "$(git symbolic-ref --quiet --short HEAD || true)" != "$BRANCH" ]; then
  if git rev-parse --quiet --verify "refs/remotes/origin/$BRANCH" >/dev/null; then
    git checkout --quiet -B "$BRANCH" "origin/$BRANCH"
  else
    git checkout --quiet -B "$BRANCH" "origin/$BASE"
  fi
fi
printf '%s' "$BASE" > /tmp/muster/base
"#;

fn spec(context: &RunContext, mirror: &Mirror, image: &str) -> ContainerSpec {
    ContainerSpec {
        name: container_name(context.run),
        image: image.to_owned(),
        labels: vec![
            (LABEL_RUN.to_owned(), context.run.to_string()),
            (LABEL_PROJECT.to_owned(), context.project.to_string()),
        ],
        memory_bytes: context.config.memory_bytes,
        millicpus: context.config.millicpus,
        mounts: vec![
            Mount {
                source: MountSource::Host(mirror.path.clone()),
                target: "/mirror".to_owned(),
                read_only: true,
            },
            Mount {
                source: MountSource::Volume(format!("muster-cache-{}", context.project)),
                target: "/cache".to_owned(),
                read_only: false,
            },
        ],
    }
}

/// The person's git identity in `root`, or Muster's when they have none.
pub(crate) async fn identity(root: &Path) -> Identity {
    let value = |key: &'static str| async move {
        crate::git::answer(root, &["config", key])
            .await
            .ok()
            .flatten()
            .filter(|value| !value.is_empty())
    };
    Identity {
        name: value("user.name")
            .await
            .unwrap_or_else(|| "Muster".to_owned()),
        email: value("user.email")
            .await
            .unwrap_or_else(|| "muster@localhost".to_owned()),
    }
}

/// The last `count` lines of `text`, on one line.
pub(crate) fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].join(" · ")
}

#[cfg(test)]
pub(crate) mod tests;
