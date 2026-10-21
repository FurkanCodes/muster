//! Where each coding agent is installed, and the version it says it is
//! (`Command::Agents`, protocol 45), for Settings › Agents. The daemon
//! answers, as it is what runs the agents: each program is looked for on
//! its own `PATH`, as a session's would be, and run with `--version`.
//!
//! Only programs Muster already knows are run: its own agents' and those
//! of the agents the person added in Muster's own settings, never a name
//! from anywhere else. Each runs as [`crate::ask`] runs an agent, with no
//! shell, its stdin closed, in Muster's own empty folder, the variables a
//! session never inherits taken away, and within [`VERSION_LIMIT`], killed
//! past it; all at once, and only when a client asks (`ARC-13`, `ARC-25`).
//! Only so much of what it says is read, as text, and only the version
//! found in it goes on.

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use muster_core::protocol::agents::AgentFound;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// How long a program has to say its version.
pub(crate) const VERSION_LIMIT: Duration = Duration::from_secs(3);

/// The most of what a program says that is read.
const MOST_SAID: u64 = 4 * 1024;

/// The longest version kept: anything longer is no version.
const LONGEST_VERSION: usize = 40;

/// One agent to find: its harness, and its program as it is named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub(crate) harness: String,
    pub(crate) program: String,
}

/// Where `program` is found, as starting it would find it: a name with a
/// `/` in it as it is, where it is absolute; any other in each absolute
/// folder of `path` in turn, the first that holds a program to run.
/// `is_program` says whether a path is one.
pub(crate) fn locate(
    program: &str,
    path: Option<&OsStr>,
    is_program: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    if program.contains('/') {
        let named = PathBuf::from(program);
        return (named.is_absolute() && is_program(&named)).then_some(named);
    }
    std::env::split_paths(path?)
        .filter(|folder| folder.is_absolute())
        .map(|folder| folder.join(program))
        .find(|candidate| is_program(candidate))
}

/// Whether `path` is a file that can be run.
fn is_program(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The version in what a program said: its first word that is one, a
/// `v` before it taken off (`codex-cli 0.162.1` is `0.162.1`; `2.1.303
/// (Claude Code)` is `2.1.303`). A version is numbers with dots between,
/// at least two of them, and what follows a `-` or `+` after them.
pub(crate) fn version_in(said: &str) -> Option<String> {
    said.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\''))
        .map(|word| word.strip_prefix(['v', 'V']).unwrap_or(word))
        .map(|word| word.trim_end_matches(['.', ':']))
        .find(|word| is_version(word))
        .map(str::to_owned)
}

fn is_version(word: &str) -> bool {
    if word.is_empty() || word.len() > LONGEST_VERSION {
        return false;
    }
    let (numbers, rest) = word
        .find(['-', '+'])
        .map_or((word, ""), |at| word.split_at(at));
    let parts: Vec<&str> = numbers.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'+' | b'.'))
}

/// Each agent `wanted`, found on `path` and asked its version in `folder`,
/// all at once; in the order asked. Dropped, it stops every program it ran.
pub(crate) async fn find(
    wanted: Vec<Wanted>,
    path: Option<OsString>,
    folder: PathBuf,
) -> Vec<AgentFound> {
    let _ = tokio::fs::create_dir_all(&folder).await;
    let mut asking = tokio::task::JoinSet::new();
    for (at, wanted) in wanted.into_iter().enumerate() {
        let found = locate(&wanted.program, path.as_deref(), is_program);
        let folder = folder.clone();
        asking.spawn(async move {
            let version = match &found {
                Some(program) => version_of(program, &folder).await,
                None => None,
            };
            let found = AgentFound {
                harness: wanted.harness,
                // As text, as the person reads it.
                path: found.map(|path| path.to_string_lossy().into_owned()),
                version,
            };
            (at, found)
        });
    }
    let mut answers = Vec::new();
    while let Some(answer) = asking.join_next().await {
        if let Ok(answer) = answer {
            answers.push(answer);
        }
    }
    answers.sort_by_key(|(at, _)| *at);
    answers.into_iter().map(|(_, found)| found).collect()
}

/// What `program --version` says its version is, within [`VERSION_LIMIT`];
/// `None` where it says none, fails or takes longer, when it is killed.
async fn version_of(program: &Path, folder: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in crate::harness::scrub::scrubbed(std::env::vars().map(|(key, _)| key)) {
        command.env_remove(key);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let said = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut out_part = (&mut stdout).take(MOST_SAID);
        let mut err_part = (&mut stderr).take(MOST_SAID);
        let (read_out, read_err) = tokio::join!(
            out_part.read_to_end(&mut out),
            err_part.read_to_end(&mut err),
        );
        read_out.ok()?;
        read_err.ok()?;
        let status = child.wait().await.ok()?;
        status.success().then_some((out, err))
    };
    // Past the limit the child is dropped, and so killed.
    let (out, err) = tokio::time::timeout(VERSION_LIMIT, said).await.ok()??;
    version_in(&String::from_utf8_lossy(&out))
        .or_else(|| version_in(&String::from_utf8_lossy(&err)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_what_each_agent_says() {
        for (said, version) in [
            ("2.1.303 (Claude Code)\n", Some("2.1.303")),
            ("codex-cli 0.162.1\n", Some("0.162.1")),
            ("0.63.0", Some("0.63.0")),
            ("opencode v1.4.6\n", Some("1.4.6")),
            (
                "GitHub Copilot CLI 0.0.339.\nCommit: 4d5e6f7",
                Some("0.0.339"),
            ),
            (
                "my-agent version 1.2.0-beta.3+abc\n",
                Some("1.2.0-beta.3+abc"),
            ),
            ("built 2026 from main", None),
            ("ready.", None),
            ("1.", None),
            ("..", None),
            ("", None),
        ] {
            assert_eq!(version_in(said).as_deref(), version, "{said:?}");
        }
    }

    #[test]
    fn a_word_too_long_or_with_more_than_a_version_in_it_is_none() {
        assert_eq!(version_in(&format!("1.{}", "2".repeat(60))), None);
        assert_eq!(version_in("1.2.3-$(touch x)"), None);
        assert_eq!(version_in("\u{1b}[31m1.2.3"), None);
    }

    #[test]
    fn a_program_is_looked_for_in_each_absolute_folder_of_path_in_turn() {
        let programs = [
            "/home/p/.local/bin/claude",
            "/opt/homebrew/bin/claude",
            "/usr/bin/codex",
        ];
        let is = |path: &Path| programs.iter().any(|program| Path::new(program) == path);
        let path = OsStr::new("relative:/opt/homebrew/bin:/home/p/.local/bin:/usr/bin");
        assert_eq!(
            locate("claude", Some(path), is),
            Some(PathBuf::from("/opt/homebrew/bin/claude"))
        );
        assert_eq!(
            locate("codex", Some(path), is),
            Some(PathBuf::from("/usr/bin/codex"))
        );
        assert_eq!(locate("gemini", Some(path), is), None);
        assert_eq!(locate("claude", None, is), None);
        assert_eq!(locate("", Some(path), |_: &Path| true), None);
    }

    #[test]
    fn a_program_named_by_its_path_is_that_one_only_where_it_is_absolute() {
        let all = |_: &Path| true;
        let path = OsStr::new("/usr/bin");
        assert_eq!(
            locate("/opt/agents/pi", Some(path), all),
            Some(PathBuf::from("/opt/agents/pi"))
        );
        assert_eq!(locate("bin/pi", Some(path), all), None);
        assert_eq!(locate("/opt/agents/pi", Some(path), |_: &Path| false), None);
    }
}
