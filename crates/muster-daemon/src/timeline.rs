//! Workspaces' timelines (`PRODUCT.md` §6): the files of a workspace as
//! each agent's turn there begins and ends, kept so the person can see
//! what each turn changed and put the files back as they were before it
//! (`PRD-14`).
//!
//! Each workspace's timeline is a bare git repository of Muster's own, in
//! its data directory, that borrows the objects of the workspace's
//! repository (git's alternates), so a snapshot writes only what is new.
//! Snapshots are taken with an index of the store's own: the person's
//! index, refs and objects are only read. Git, finding there an object a
//! snapshot needs, refreshes that object's file time, as it does in any
//! repository that borrows another's objects; that also keeps it from
//! being pruned there while the timeline holds it.
//!
//! An entry is a commit under `refs/timeline/`, holding the files as the
//! entry left them, its one parent a commit with none holding the files
//! as it found them, so an entry let go takes nothing else with it. A
//! turn under way has its start under `refs/open/<session>`. The store is
//! the whole record: nothing of it is in the database, and it goes with
//! its workspace (`ARC-14`).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use muster_core::changes::{self, ChangeSummary, FileDiff, MAX_DIFF};
use muster_core::timeline::{self, Entry, EntryKind, MAX_ENTRIES, Timeline, is_object_id};
use muster_core::{SessionId, Timestamp};

use crate::git::{self, GitError};

mod undo;

/// How many entries past [`MAX_ENTRIES`] wait before the oldest go, so the
/// store is tidied now and then rather than after every turn.
const SLACK: usize = 20;

/// Untracked files larger than this are left out of snapshots: build
/// outputs and downloads that nothing ignores, not the person's work.
const MAX_UNTRACKED: u64 = 32 * 1024 * 1024;

/// The most of a listing read from git.
const MAX_LISTING: usize = 16 * 1024 * 1024;

/// Settings git's snapshot commands take whatever the person's own say:
/// none of them may make git write in the repository (a split index's
/// shared file, a file system monitor's socket) or talk.
const QUIET: &[&str] = &[
    "-c",
    "core.splitIndex=false",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "advice.addEmbeddedRepo=false",
    "-c",
    "commit.gpgSign=false",
];

/// Why the timeline could not do what it was asked.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TimelineError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("could not keep the timeline at {0}: {1}")]
    Io(String, std::io::Error),
    #[error("there is no entry {0} in this workspace's timeline")]
    NoEntry(u64),
    #[error("git no longer has everything entry {0} began from, so its files cannot come back")]
    Gone(u64),
    #[error("the files were not put back: {0}")]
    Refused(String),
    #[error("the branch stays where it is: {0}")]
    Branch(String),
    #[error("the files are already as they were before entry {0}")]
    AlreadyThere(u64),
}

/// Where the timeline of `workspace` is kept.
pub(crate) fn dir(data_dir: &Path, workspace: muster_core::WorkspaceId) -> PathBuf {
    data_dir.join("timelines").join(workspace.to_string())
}

/// Lets go of a workspace's timeline, as the workspace goes.
pub(crate) async fn remove(dir: &Path) {
    match tokio::fs::remove_dir_all(dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not remove a workspace's timeline"),
    }
}

/// The files of a workspace at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    tree: String,
    head: Option<String>,
}

/// An entry as the store keeps it.
#[derive(Clone, Debug)]
struct Kept {
    entry: Entry,
    /// The files as the entry left them.
    tree: String,
    commit: String,
    /// The commit holding the files as the entry found them.
    before: Option<String>,
}

/// A workspace's timeline store.
#[derive(Debug)]
pub(crate) struct Store {
    dir: PathBuf,
    worktree: PathBuf,
    /// The workspace's repository's objects, which the store borrows.
    borrowed: PathBuf,
}

impl Store {
    /// The timeline at `dir` of the workspace at `worktree`, made the
    /// first time, private to the person.
    pub(crate) async fn open(dir: &Path, worktree: &Path) -> Result<Self, TimelineError> {
        let common = git::checked(
            worktree,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .await?;
        let borrowed = PathBuf::from(common).join("objects");
        if !dir.join("HEAD").exists() {
            let parent = dir.parent().unwrap_or(dir);
            create_private(parent).await?;
            let dir_text = dir.to_string_lossy();
            git::checked(
                parent,
                &[
                    "-c",
                    "init.defaultBranch=timeline",
                    "init",
                    "--quiet",
                    "--bare",
                    "--template=",
                    &dir_text,
                ],
            )
            .await?;
            let store = Self {
                dir: dir.to_owned(),
                worktree: worktree.to_owned(),
                borrowed: borrowed.clone(),
            };
            store.inside(&["config", "gc.auto", "0"]).await?;
        }
        let alternates = dir.join("objects").join("info").join("alternates");
        let wanted = format!("{}\n", borrowed.display());
        let written = tokio::fs::read_to_string(&alternates).await.ok();
        if written.as_deref() != Some(wanted.as_str()) {
            tokio::fs::write(&alternates, wanted)
                .await
                .map_err(|error| TimelineError::Io(dir.display().to_string(), error))?;
        }
        Ok(Self {
            dir: dir.to_owned(),
            worktree: worktree.to_owned(),
            borrowed,
        })
    }

    /// Git in the store itself, never in the person's repository.
    async fn inside(&self, args: &[&str]) -> Result<String, GitError> {
        let dir = format!("--git-dir={}", self.dir.display());
        let mut all = vec![dir.as_str()];
        all.extend_from_slice(QUIET);
        all.extend_from_slice(args);
        git::checked(&self.dir, &all).await
    }

    /// [`Self::inside`], its output as it was printed.
    async fn inside_raw(&self, args: &[&str], limit: usize) -> Result<(Vec<u8>, bool), GitError> {
        let dir = format!("--git-dir={}", self.dir.display());
        let mut all = vec![dir.as_str()];
        all.extend_from_slice(QUIET);
        all.extend_from_slice(args);
        git::capped(&self.dir, &all, limit, &[0]).await
    }

    /// What points git at the workspace's files with the store's index
    /// and objects.
    fn vars(&self) -> Vec<(&'static str, OsString)> {
        vec![
            ("GIT_INDEX_FILE", self.dir.join("index").into_os_string()),
            (
                "GIT_OBJECT_DIRECTORY",
                self.dir.join("objects").into_os_string(),
            ),
            (
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                self.borrowed.clone().into_os_string(),
            ),
        ]
    }

    /// Git in the workspace, reading its files, with the store's index
    /// and objects.
    async fn over_files(&self, args: &[&str]) -> Result<String, GitError> {
        let vars = self.vars();
        let vars: Vec<(&str, &OsStr)> = vars
            .iter()
            .map(|(name, value)| (*name, value.as_os_str()))
            .collect();
        let mut all = QUIET.to_vec();
        all.extend_from_slice(args);
        git::checked_with(&self.worktree, &all, &vars).await
    }

    /// The workspace's files as they are now, and what is checked out.
    pub(crate) async fn snapshot(&self) -> Result<Snapshot, TimelineError> {
        let head = head(&self.worktree).await?;
        if !self.dir.join("index").exists()
            && let Some(head) = &head
        {
            // So the files git tracks are not all taken for untracked.
            self.over_files(&["read-tree", head]).await?;
        }
        let listing = {
            let vars = self.vars();
            let vars: Vec<(&str, &OsStr)> = vars
                .iter()
                .map(|(name, value)| (*name, value.as_os_str()))
                .collect();
            git::capped_with(
                &self.worktree,
                &["ls-files", "-z", "--others", "--exclude-standard"],
                MAX_LISTING,
                &[0],
                &vars,
            )
            .await?
            .0
        };
        let mut args = vec![
            "add".to_owned(),
            "-A".to_owned(),
            "--".to_owned(),
            ".".to_owned(),
        ];
        for path in changes::parse_paths(&String::from_utf8_lossy(&listing)) {
            let size = tokio::fs::symlink_metadata(self.worktree.join(&path))
                .await
                .map_or(0, |metadata| metadata.len());
            if size > MAX_UNTRACKED {
                args.push(format!(":(exclude,literal){path}"));
            }
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.over_files(&args).await?;
        let tree = self.over_files(&["write-tree"]).await?;
        Ok(Snapshot { tree, head })
    }

    /// Keeps `tree` as a commit, with `parent` if any, `message` and
    /// `at`'s time.
    async fn commit(
        &self,
        tree: &str,
        parent: Option<&str>,
        message: &str,
        at: Timestamp,
    ) -> Result<String, GitError> {
        let date = format!("@{} +0000", at.as_millis().div_euclid(1000));
        let mut args = vec!["commit-tree", "--no-gpg-sign", "-m", message];
        if let Some(parent) = parent {
            args.extend(["-p", parent]);
        }
        args.push(tree);
        let dir = format!("--git-dir={}", self.dir.display());
        let mut all = vec![dir.as_str()];
        all.extend_from_slice(QUIET);
        all.extend_from_slice(&args);
        let who = OsStr::new("Muster");
        let address = OsStr::new("muster@localhost");
        let date = OsString::from(date);
        git::checked_with(
            &self.dir,
            &all,
            &[
                ("GIT_AUTHOR_NAME", who),
                ("GIT_AUTHOR_EMAIL", address),
                ("GIT_COMMITTER_NAME", who),
                ("GIT_COMMITTER_EMAIL", address),
                ("GIT_AUTHOR_DATE", &date),
                ("GIT_COMMITTER_DATE", &date),
            ],
        )
        .await
    }

    /// A snapshot kept as a commit with no parent, for an entry to begin
    /// from.
    async fn keep(&self, snapshot: &Snapshot, at: Timestamp) -> Result<String, GitError> {
        let message = timeline::snapshot_message(at, snapshot.head.as_deref());
        self.commit(&snapshot.tree, None, &message, at).await
    }

    /// Every entry kept, newest first.
    async fn kept(&self) -> Result<Vec<Kept>, GitError> {
        let (out, _) = self
            .inside_raw(
                &[
                    "for-each-ref",
                    "--sort=-refname",
                    "--format=%(objectname)%00%(tree)%00%(parent)%00%(contents)%00",
                    "refs/timeline/",
                ],
                MAX_LISTING,
            )
            .await?;
        let text = String::from_utf8_lossy(&out);
        let fields: Vec<&str> = text.split('\0').collect();
        Ok(fields
            .as_chunks::<4>()
            .0
            .iter()
            .filter_map(|record| {
                let entry = timeline::parse_entry(record[3])?;
                let before = record[2].split_whitespace().next().map(str::to_owned);
                Some(Kept {
                    entry,
                    tree: record[1].trim().to_owned(),
                    commit: record[0].trim().to_owned(),
                    before,
                })
            })
            .collect())
    }

    /// The workspace's timeline, newest first.
    pub(crate) async fn timeline(&self) -> Result<Timeline, TimelineError> {
        Ok(Timeline {
            entries: self
                .kept()
                .await?
                .into_iter()
                .map(|kept| kept.entry)
                .collect(),
        })
    }

    /// The newest entry's number, if any.
    #[cfg(test)]
    pub(crate) async fn newest(&self) -> Result<Option<u64>, TimelineError> {
        Ok(self.kept().await?.first().map(|kept| kept.entry.number))
    }

    /// What entry `number` changed, as git prints it, cut at [`MAX_DIFF`].
    pub(crate) async fn diff(&self, number: u64) -> Result<FileDiff, TimelineError> {
        let kept = self.find(number).await?;
        let before = kept.before.ok_or(TimelineError::NoEntry(number))?;
        let (raw, cut) = self
            .inside_raw(
                &[
                    "diff",
                    "--no-color",
                    "--no-ext-diff",
                    "-M",
                    "--end-of-options",
                    &before,
                    &kept.commit,
                ],
                MAX_DIFF,
            )
            .await?;
        Ok(FileDiff::cleaned(&raw, cut))
    }

    async fn find(&self, number: u64) -> Result<Kept, TimelineError> {
        self.kept()
            .await?
            .into_iter()
            .find(|kept| kept.entry.number == number)
            .ok_or(TimelineError::NoEntry(number))
    }

    /// What changed in the files from `from` to `to`, two trees.
    async fn summary(&self, from: &str, to: &str) -> Result<Option<ChangeSummary>, GitError> {
        if from == to {
            return Ok(None);
        }
        let mut args = vec![
            "diff",
            "-z",
            "-M",
            "--no-ext-diff",
            "--name-status",
            "--end-of-options",
            from,
            to,
        ];
        let names = text(self.inside_raw(&args, MAX_LISTING).await?);
        args[4] = "--numstat";
        let counts = text(self.inside_raw(&args, MAX_LISTING).await?);
        let files = changes::file_changes(&names, &counts);
        if files.is_empty() {
            return Ok(None);
        }
        let (added, removed) = files
            .iter()
            .filter_map(|file| file.lines)
            .fold((0, 0), |(added, removed), lines| {
                (added + lines.added, removed + lines.removed)
            });
        Ok(Some(ChangeSummary {
            files: u64::try_from(files.len()).unwrap_or(u64::MAX),
            added,
            removed,
        }))
    }

    /// Keeps `entry`, its files going from `before` to `after`, as the
    /// newest.
    async fn record(
        &self,
        mut entry: Entry,
        before: &str,
        after: &Snapshot,
    ) -> Result<u64, TimelineError> {
        let before_tree = self
            .inside(&["rev-parse", &format!("{before}^{{tree}}")])
            .await?;
        entry.summary = self.summary(&before_tree, &after.tree).await?;
        entry.head_after.clone_from(&after.head);
        let message = timeline::entry_message(&entry);
        let commit = self
            .commit(&after.tree, Some(before), &message, entry.ended)
            .await?;
        self.inside(&["update-ref", &entry_ref(entry.number), &commit])
            .await?;
        self.tidy().await?;
        Ok(entry.number)
    }

    /// Lets the oldest entries go once there are more than
    /// [`MAX_ENTRIES`] and some slack, and what only they held.
    async fn tidy(&self) -> Result<(), GitError> {
        self.tidy_to(MAX_ENTRIES, SLACK).await
    }

    /// [`Self::tidy`], keeping `most` entries once there are more than
    /// `most` and `slack`.
    async fn tidy_to(&self, most: usize, slack: usize) -> Result<(), GitError> {
        let kept = self.kept().await?;
        if kept.len() <= most + slack {
            return Ok(());
        }
        for old in &kept[most..] {
            self.inside(&["update-ref", "-d", &entry_ref(old.entry.number)])
                .await?;
        }
        self.inside(&["gc", "--quiet", "--prune=now"])
            .await
            .map(drop)
    }

    /// The turn under way of `session`: its start and what it found.
    async fn open_turn(
        &self,
        session: SessionId,
    ) -> Result<Option<(String, Timestamp, Option<String>)>, GitError> {
        let reference = open_ref(session);
        let found = git::answer(
            &self.dir,
            &[
                &format!("--git-dir={}", self.dir.display()),
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{reference}^{{commit}}"),
            ],
        )
        .await?;
        let Some(commit) = found.filter(|id| is_object_id(id)) else {
            return Ok(None);
        };
        let message = self.inside(&["log", "-1", "--format=%B", &commit]).await?;
        Ok(timeline::parse_snapshot(&message).map(|(at, head)| (commit, at, head)))
    }

    /// The sessions with a turn under way, as the store has them.
    async fn open_sessions(&self) -> Result<Vec<SessionId>, GitError> {
        let listing = self
            .inside(&["for-each-ref", "--format=%(refname:lstrip=2)", "refs/open/"])
            .await?;
        Ok(listing
            .lines()
            .filter_map(|name| name.trim().parse().ok())
            .collect())
    }

    /// An agent's turn began in the workspace `at`. What changed since the
    /// newest entry is an entry of its own first, unless another agent
    /// still works there, whose turn it is part of. Turns under way of
    /// sessions not `working` now are let go.
    pub(crate) async fn began(
        &self,
        session: SessionId,
        working: &[SessionId],
        at: Timestamp,
    ) -> Result<Option<u64>, TimelineError> {
        for stale in self.open_sessions().await? {
            if stale == session || !working.contains(&stale) {
                self.inside(&["update-ref", "-d", &open_ref(stale)]).await?;
            }
        }
        let now = self.snapshot().await?;
        let mut recorded = None;
        let alone = working.iter().all(|other| *other == session);
        if let Some(newest) = self.kept().await?.into_iter().next().filter(|_| alone) {
            let moved = newest.tree != now.tree || newest.entry.head_after != now.head;
            if moved {
                let before = self
                    .keep(
                        &Snapshot {
                            tree: newest.tree.clone(),
                            head: newest.entry.head_after.clone(),
                        },
                        newest.entry.ended,
                    )
                    .await?;
                let entry = Entry {
                    number: newest.entry.number + 1,
                    kind: EntryKind::Between,
                    started: newest.entry.ended,
                    ended: at,
                    summary: None,
                    head_before: newest.entry.head_after.clone(),
                    head_after: None,
                };
                recorded = Some(self.record(entry, &before, &now).await?);
            }
        }
        let start = self.keep(&now, at).await?;
        self.inside(&["update-ref", &open_ref(session), &start])
            .await?;
        Ok(recorded)
    }

    /// An agent's turn ended `at`: what it changed is an entry, unless it
    /// changed nothing. A turn whose start was not seen is let go.
    pub(crate) async fn ended(
        &self,
        session: SessionId,
        agent: &str,
        at: Timestamp,
    ) -> Result<Option<u64>, TimelineError> {
        let Some((start, started, head_before)) = self.open_turn(session).await? else {
            return Ok(None);
        };
        let now = self.snapshot().await?;
        let start_tree = self
            .inside(&["rev-parse", &format!("{start}^{{tree}}")])
            .await?;
        let recorded = if start_tree == now.tree && head_before == now.head {
            None
        } else {
            let number = self.next_number().await?;
            let entry = Entry {
                number,
                kind: EntryKind::Turn {
                    session,
                    agent: agent.to_owned(),
                },
                started,
                ended: at,
                summary: None,
                head_before,
                head_after: None,
            };
            Some(self.record(entry, &start, &now).await?)
        };
        self.inside(&["update-ref", "-d", &open_ref(session)])
            .await?;
        Ok(recorded)
    }

    async fn next_number(&self) -> Result<u64, GitError> {
        Ok(self
            .kept()
            .await?
            .first()
            .map_or(1, |kept| kept.entry.number + 1))
    }
}

/// What is checked out in the workspace at `worktree`; `None` before its
/// repository's first commit.
async fn head(worktree: &Path) -> Result<Option<String>, GitError> {
    Ok(git::answer(
        worktree,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
    )
    .await?
    .filter(|id| is_object_id(id)))
}

/// Makes `dir` and those above it, `dir` itself readable by the person
/// alone: the timeline holds their code.
async fn create_private(dir: &Path) -> Result<(), TimelineError> {
    let mut builder = tokio::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(dir)
        .await
        .map_err(|error| TimelineError::Io(dir.display().to_string(), error))
}

fn entry_ref(number: u64) -> String {
    format!("refs/timeline/{number:010}")
}

fn open_ref(session: SessionId) -> String {
    format!("refs/open/{session}")
}

fn text((bytes, _cut): (Vec<u8>, bool)) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
