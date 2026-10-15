//! Worktrees made or removed outside Muster: by hand, or by an agent
//! running `git worktree add`. Nebula keeps them in step; so does Muster.
//!
//! Every few seconds a task looks at each project's `.git/worktrees`
//! directory, which git changes whenever a worktree is added or removed,
//! and at whether Muster's worktrees still exist. Only when something
//! changed, and once a minute regardless, it asks git for the list
//! (`git worktree list --porcelain -z`) and hands it to the Supervisor,
//! which adopts worktrees it does not know and forgets those that are gone.
//! Projects that are plain folders are left out until they are repositories.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use muster_core::ProjectId;

use crate::git;
use crate::supervisor::SupervisorHandle;

/// How often git is asked even when nothing seemed to change.
const ANYWAY: Duration = Duration::from_secs(60);

/// One worktree as git lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) path: PathBuf,
    /// The branch checked out, or `detached at <commit>`.
    pub(crate) branch: String,
    /// Git knows its directory is gone.
    pub(crate) prunable: bool,
}

/// Whether git is still making one of the listed worktrees: listed, but
/// its HEAD not yet written (all zeros), so it has no branch to tell.
pub(crate) fn being_made(output: &str) -> bool {
    output
        .split('\0')
        .filter_map(|line| line.strip_prefix("HEAD "))
        .any(is_unwritten)
}

fn is_unwritten(head: &str) -> bool {
    !head.is_empty() && head.chars().all(|c| c == '0')
}

/// Reads `git worktree list --porcelain -z`: records of NUL-ended
/// `key value` lines, each record ended by an empty one. The main checkout
/// comes first and is left out; a bare repository's entry too, and a
/// worktree git is still making, which the next look finds whole.
pub(crate) fn parse(output: &str) -> Vec<Listed> {
    let mut listed = Vec::new();
    for (index, record) in output.split("\0\0").enumerate() {
        let mut path = None;
        let mut branch = None;
        let mut head = None;
        let (mut prunable, mut bare) = (false, false);
        for line in record.split('\0').filter(|line| !line.is_empty()) {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => path = Some(PathBuf::from(value)),
                "branch" => {
                    branch = Some(
                        value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value)
                            .to_owned(),
                    );
                }
                "HEAD" => head = Some(value.to_owned()),
                "prunable" => prunable = true,
                "bare" => bare = true,
                _ => {}
            }
        }
        let Some(path) = path else { continue };
        if index == 0 || bare || head.as_deref().is_some_and(is_unwritten) {
            continue;
        }
        let branch = branch
            .or_else(|| {
                head.filter(|head| head.len() >= 7)
                    .map(|head| format!("detached at {}", &head[..7]))
            })
            .unwrap_or_else(|| "?".to_owned());
        listed.push(Listed {
            path,
            branch,
            prunable,
        });
    }
    listed
}

/// What the sync task needs to know about a project.
#[derive(Clone, Debug)]
pub(crate) struct ProjectWorktrees {
    pub(crate) project: ProjectId,
    pub(crate) root: PathBuf,
    /// Where Muster's worktrees of the project are.
    pub(crate) known: Vec<PathBuf>,
}

/// Looks every `every` until the Supervisor stops.
pub(crate) async fn run(supervisor: SupervisorHandle, every: Duration) {
    let mut seen: HashMap<ProjectId, (Option<SystemTime>, Instant)> = HashMap::new();
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(projects) = supervisor.projects_for_sync().await else {
            return;
        };
        seen.retain(|project, _| projects.iter().any(|p| p.project == *project));
        for project in projects {
            let looked = {
                let project = project.clone();
                tokio::task::spawn_blocking(move || {
                    (
                        git::is_repository(&project.root),
                        changed_at(&project.root),
                        project.known.iter().any(|path| !path.exists()),
                    )
                })
                .await
            };
            // A plain folder has no worktrees. Git, asked there, would list
            // those of a repository the folder sits in, which are not its.
            let Ok((true, stamp, missing)) = looked else {
                continue;
            };
            let due = match seen.get(&project.project) {
                None => true,
                Some((last, at)) => {
                    *last != stamp
                        || missing
                        || crate::clock::monotonic().saturating_duration_since(*at) >= ANYWAY
                }
            };
            if !due {
                continue;
            }
            match git::list_worktrees(&project.root).await {
                Ok(output) => {
                    // One still being made is looked at again next time,
                    // though nothing in `.git/worktrees` will say so.
                    if being_made(&output) {
                        seen.remove(&project.project);
                    } else {
                        seen.insert(project.project, (stamp, crate::clock::monotonic()));
                    }
                    if !supervisor
                        .worktrees_listed(project.project, parse(&output))
                        .await
                    {
                        return;
                    }
                }
                Err(error) => {
                    tracing::debug!(project = %project.project, %error, "could not list worktrees");
                }
            }
        }
    }
}

/// When git last added or removed one of the repository's worktrees, as
/// the modification time of its `.git/worktrees` directory, if it has one.
fn changed_at(root: &Path) -> Option<SystemTime> {
    std::fs::metadata(root.join(".git").join("worktrees"))
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::{git, repository};

    #[test]
    fn a_worktree_git_is_still_making_waits_for_the_next_look() {
        let output = "worktree /r\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0\
                      worktree /half\0HEAD 0000000000000000000000000000000000000000\0detached\0\0\
                      worktree /whole\0HEAD 2222222222222222222222222222222222222222\0branch refs/heads/w\0\0";
        assert!(being_made(output));
        assert_eq!(
            parse(output),
            [Listed {
                path: PathBuf::from("/whole"),
                branch: "w".to_owned(),
                prunable: false,
            }]
        );
        assert!(!being_made(
            "worktree /r\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0"
        ));
    }

    #[test]
    fn the_porcelain_list_is_read_without_the_main_checkout() {
        let output = "worktree /r\0HEAD 1111111111111111111111111111111111111111\0branch refs/heads/main\0\0\
                      worktree /a b\0HEAD 2222222222222222222222222222222222222222\0branch refs/heads/feature/x\0\0\
                      worktree /d\0HEAD 3333333333333333333333333333333333333333\0detached\0\0\
                      worktree /gone\0HEAD 4444444444444444444444444444444444444444\0branch refs/heads/g\0prunable gitdir file points to non-existent location\0\0\
                      worktree /l\0HEAD 5555555555555555555555555555555555555555\0branch refs/heads/l\0locked\0\0";
        assert_eq!(
            parse(output),
            [
                Listed {
                    path: PathBuf::from("/a b"),
                    branch: "feature/x".to_owned(),
                    prunable: false,
                },
                Listed {
                    path: PathBuf::from("/d"),
                    branch: "detached at 3333333".to_owned(),
                    prunable: false,
                },
                Listed {
                    path: PathBuf::from("/gone"),
                    branch: "g".to_owned(),
                    prunable: true,
                },
                Listed {
                    path: PathBuf::from("/l"),
                    branch: "l".to_owned(),
                    prunable: false,
                },
            ]
        );
        assert!(parse("").is_empty());
        assert!(parse("worktree /bare\0bare\0\0").is_empty());
    }

    #[tokio::test]
    async fn real_git_is_read_and_its_directory_moves_when_worktrees_do() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        assert_eq!(changed_at(&repo), None, "no worktree yet");
        let tree = dir.path().join("side tree");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "side",
                tree.to_str().unwrap(),
            ],
        );
        let first = changed_at(&repo).expect("a worktrees directory");
        let listed = parse(&git::list_worktrees(&repo).await.unwrap());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, tree.canonicalize().unwrap());
        assert_eq!(listed[0].branch, "side");

        std::thread::sleep(Duration::from_millis(20));
        git(&repo, &["worktree", "remove", tree.to_str().unwrap()]);
        assert_ne!(changed_at(&repo), Some(first));
        assert!(parse(&git::list_worktrees(&repo).await.unwrap()).is_empty());
    }
}
