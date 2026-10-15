//! Which image a run's container uses (`ARCHITECTURE.md` §8): the
//! project's own (`project.dockerfile` in `muster.toml`), or Muster's base
//! image, built once: Node, git and Claude Code, in the version the person
//! has, run as an unprivileged user, since Claude Code refuses to skip its
//! permission prompts as root. (`MUSTER_RUN_IMAGE` names a ready image
//! instead.) Checked by hand against Claude Code 2.1.296: it builds, runs
//! as `muster` with the state and work directories writable, and accepts
//! `--dangerously-skip-permissions` there.

use std::path::Path;
use std::time::Duration;

use crate::containers::Docker;

/// How long building an image may take: the base image downloads Node and
/// Claude Code.
const BUILD_LIMIT: Duration = Duration::from_secs(30 * 60);

/// Muster's base image recipe; `{CLAUDE}` is the Claude Code version.
const BASE: &str = r#"FROM node:22-bookworm-slim
RUN apt-get update \
 && apt-get install --yes --no-install-recommends git ca-certificates curl procps util-linux \
 && rm -rf /var/lib/apt/lists/*
RUN npm install --global @anthropic-ai/claude-code@{CLAUDE} && npm cache clean --force
RUN useradd --create-home --shell /bin/bash muster \
 && mkdir -p /work /cache && chown muster:muster /work /cache
USER muster
WORKDIR /work
# The version is the person's; an unprivileged user cannot update it anyway.
ENV DISABLE_AUTOUPDATER=1
"#;

/// The image for a run of the project at `root`, built if need be.
pub(crate) async fn image_for(
    docker: &Docker,
    root: &Path,
    dockerfile: Option<&Path>,
    claude_version: &str,
) -> Result<String, String> {
    let (tag, recipe, context) = match dockerfile {
        Some(relative) => {
            let recipe = tokio::fs::read_to_string(root.join(relative))
                .await
                .map_err(|error| format!("cannot read {}: {error}", relative.display()))?;
            let tag = format!(
                "muster-project:{}",
                short_hash(&format!("{}{recipe}", root.display()))
            );
            (tag, recipe, Some(root))
        }
        None => {
            let recipe = BASE.replace("{CLAUDE}", claude_version);
            (format!("muster-base:{}", short_hash(&recipe)), recipe, None)
        }
    };
    if docker
        .has_image(&tag)
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok(tag);
    }
    tracing::info!(%tag, "building a run image");
    docker
        .build(&tag, &recipe, context, BUILD_LIMIT)
        .await
        .map_err(|error| format!("could not build the image {tag}: {error}"))?;
    Ok(tag)
}

/// The Claude Code version on the person's machine, to match in images;
/// `latest` if it cannot be told, in time or at all (`ARC-13`).
pub(crate) async fn claude_version() -> String {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new("claude")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    output
        .ok()
        .and_then(Result::ok)
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .filter(|version| version.chars().all(|c| c.is_ascii_digit() || c == '.'))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "latest".to_owned())
}

/// A short, stable name for `text`: FNV-1a, in hex.
fn short_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")[..12].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_names_follow_their_recipe() {
        assert_eq!(short_hash("a"), short_hash("a"));
        assert_ne!(short_hash("a"), short_hash("b"));
        assert_eq!(short_hash("x").len(), 12);
        let recipe = BASE.replace("{CLAUDE}", "2.1.296");
        assert!(recipe.contains("@anthropic-ai/claude-code@2.1.296"));
        assert!(
            recipe.contains("USER muster"),
            "agents must not run as root"
        );
        assert!(recipe.contains("ENV DISABLE_AUTOUPDATER=1"));
    }
}
