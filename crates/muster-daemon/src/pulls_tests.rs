//! Each branch's pull request, shown on its workspace (`PRODUCT.md` §6):
//! looked at through a stand-in git host on a real repository.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::protocol::{Command, Delta, Reply};
use muster_core::{
    Checks, ProjectId, PullRequest, PullRequestState, PullRequestStatus, Review, Timestamp,
    WorkspaceId,
};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::clock::Clock;
use crate::git::tests::repository;
use crate::githost::choose::Hosts;
use crate::githost::{GitHost, PrState};
use crate::service::{BoxFuture, ProjectPlace, ServiceError};
use crate::store::{Store, StoreHandle};
use crate::supervisor::{self, SupervisorHandle};

#[derive(Debug)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(42)
    }
}

/// A git host that knows each branch's pull request, or fails as told,
/// and remembers which branches it was asked about.
#[derive(Debug, Default)]
struct FakeHost {
    pulls: Mutex<HashMap<String, PullRequestStatus>>,
    failing: Mutex<Option<String>>,
    /// Never answer, as a host whose answer is lost.
    silent: std::sync::atomic::AtomicBool,
    asked: Mutex<Vec<String>>,
}

impl FakeHost {
    fn set(&self, branch: &str, status: Option<PullRequestStatus>) {
        let mut pulls = self.pulls.lock().unwrap();
        match status {
            Some(status) => pulls.insert(branch.to_owned(), status),
            None => pulls.remove(branch),
        };
    }

    fn fail(&self, error: Option<&str>) {
        *self.failing.lock().unwrap() = error.map(str::to_owned);
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

fn not_here<T: Send + 'static>() -> BoxFuture<'static, Result<T, ServiceError>> {
    Box::pin(async { Err(ServiceError::Refused("not in these tests".to_owned())) })
}

impl GitHost for FakeHost {
    fn open_draft<'a>(
        &'a self,
        _root: &'a Path,
        _branch: &'a str,
        _base: &'a str,
        _title: &'a str,
        _body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, ServiceError>> {
        not_here()
    }

    fn mark_ready<'a>(&'a self, _: &'a Path, _: u64) -> BoxFuture<'a, Result<(), ServiceError>> {
        not_here()
    }

    fn mark_draft<'a>(&'a self, _: &'a Path, _: u64) -> BoxFuture<'a, Result<(), ServiceError>> {
        not_here()
    }

    fn review_comment<'a>(
        &'a self,
        _: &'a Path,
        _: u64,
        _: &'a str,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        not_here()
    }

    fn state<'a>(&'a self, _: &'a Path, _: u64) -> BoxFuture<'a, Result<PrState, ServiceError>> {
        not_here()
    }

    fn pull_request_of<'a>(
        &'a self,
        _root: &'a Path,
        branch: &'a str,
    ) -> BoxFuture<'a, Result<Option<PullRequestStatus>, ServiceError>> {
        self.asked.lock().unwrap().push(branch.to_owned());
        if self.silent.load(std::sync::atomic::Ordering::SeqCst) {
            return Box::pin(std::future::pending());
        }
        let answer = match self.failing.lock().unwrap().clone() {
            Some(error) => Err(ServiceError::Unavailable(error)),
            None => Ok(self.pulls.lock().unwrap().get(branch).cloned()),
        };
        Box::pin(async move { answer })
    }
}

#[derive(Debug)]
struct FakeHosts(Arc<FakeHost>);

impl Hosts for FakeHosts {
    fn for_project(
        &self,
        _place: ProjectPlace,
    ) -> BoxFuture<'_, Result<Arc<dyn GitHost>, ServiceError>> {
        let host: Arc<dyn GitHost> = self.0.clone();
        Box::pin(async move { Ok(host) })
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    host: Arc<FakeHost>,
    supervisor: SupervisorHandle,
    /// The worktree on `feature`.
    worktree: WorkspaceId,
}

/// A supervisor whose project, a real repository on `main`, has a worktree
/// on `feature`.
async fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let root = repository(dir.path(), "app");
    let host = Arc::new(FakeHost::default());
    let supervisor = supervisor::spawn(supervisor::Settings {
        store: StoreHandle::spawn(Store::open_in_memory().unwrap()).unwrap(),
        clock: Arc::new(FixedClock),
        worktrees_dir: dir.path().join("worktrees"),
        hook_socket: dir.path().join("hooks.sock"),
        hook_queue: tokio::sync::mpsc::channel(1).1,
        delta_capacity: 64,
        history_every: 1,
        timing: muster_state::session::Timing::default(),
        work: Arc::new(crate::runs::NotYetBuilt),
        trackers: Arc::new(crate::trackers::choose::ServiceTrackers {
            secrets: Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
        config_dir: dir.path().join("config"),
        credentials: Arc::new(crate::runs::credentials::FixedCredentials::default()),
        docker: "docker".to_owned(),
        hosts: Arc::new(FakeHosts(host.clone())),
        adopted: Vec::new(),
    })
    .await
    .unwrap();
    let Reply::ProjectAdded { id: project } = supervisor
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let worktree = create_worktree(&supervisor, project, "feature").await;
    Setup {
        _dir: dir,
        host,
        supervisor,
        worktree,
    }
}

async fn create_worktree(
    supervisor: &SupervisorHandle,
    project: ProjectId,
    branch: &str,
) -> WorkspaceId {
    let Reply::WorktreeCreated { id } = supervisor
        .command(Command::CreateWorktree {
            project,
            branch: Some(branch.to_owned()),
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    id
}

fn status(number: u64, state: PullRequestState) -> PullRequestStatus {
    PullRequestStatus {
        number,
        url: format!("https://github.com/o/app/pull/{number}"),
        state,
        checks: None,
        review: None,
    }
}

/// Ticks the pull request timer until `workspace` is shown with a
/// different pull request, and returns it with the branch it is on.
async fn next_shown(
    supervisor: &SupervisorHandle,
    deltas: &mut broadcast::Receiver<Delta>,
    workspace: WorkspaceId,
) -> (String, Option<PullRequestStatus>) {
    timeout(Duration::from_secs(10), async {
        loop {
            assert!(supervisor.check_pull_requests().await);
            let waited = timeout(Duration::from_millis(100), async {
                loop {
                    if let Delta::WorkspaceUpserted(shown) = deltas.recv().await.unwrap()
                        && shown.id == workspace
                    {
                        return (shown.kind.branch().to_owned(), shown.pull_request);
                    }
                }
            })
            .await;
            if let Ok(shown) = waited {
                return shown;
            }
        }
    })
    .await
    .expect("the workspace's pull request was never shown")
}

/// What `workspace` shows now.
async fn shown_now(
    supervisor: &SupervisorHandle,
    workspace: WorkspaceId,
) -> Option<PullRequestStatus> {
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    snapshot
        .workspaces
        .into_iter()
        .find(|found| found.id == workspace)
        .unwrap()
        .pull_request
}

#[tokio::test]
async fn a_branch_s_pull_request_is_shown_on_its_worktree_and_kept_through_a_failed_look() {
    let Setup {
        host,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let (snapshot, mut deltas) = supervisor.subscribe().await.unwrap();
    assert!(
        snapshot.workspaces.iter().all(|w| w.pull_request.is_none()),
        "nothing is known before the first look"
    );
    let mut draft = status(42, PullRequestState::Draft);
    draft.checks = Some(Checks::Failing);
    host.set("feature", Some(draft.clone()));
    assert_eq!(
        next_shown(&supervisor, &mut deltas, worktree).await,
        ("feature".to_owned(), Some(draft.clone()))
    );
    // The checkout is on main, which pull requests merge into: never asked.
    assert!(
        host.asked().iter().all(|branch| branch == "feature"),
        "{:?}",
        host.asked()
    );

    // A look that fails keeps what the last one found.
    host.fail(Some("could not reach github.com"));
    let asked = host.asked().len();
    assert!(supervisor.check_pull_requests().await);
    timeout(Duration::from_secs(5), async {
        while host.asked().len() == asked {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(shown_now(&supervisor, worktree).await, Some(draft));
    host.fail(None);

    // Then reviewed and passing; then gone from the host.
    let mut open = status(42, PullRequestState::Open);
    open.checks = Some(Checks::Passing);
    open.review = Some(Review::Approved);
    host.set("feature", Some(open.clone()));
    assert_eq!(
        next_shown(&supervisor, &mut deltas, worktree).await,
        ("feature".to_owned(), Some(open))
    );
    host.set("feature", None);
    assert_eq!(
        next_shown(&supervisor, &mut deltas, worktree).await,
        ("feature".to_owned(), None)
    );
}

#[tokio::test]
async fn another_branch_checked_out_lets_go_of_the_last_one_s_pull_request() {
    let Setup {
        host,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let (_, mut deltas) = supervisor.subscribe().await.unwrap();
    let merged = status(7, PullRequestState::Merged);
    host.set("feature", Some(merged.clone()));
    assert_eq!(
        next_shown(&supervisor, &mut deltas, worktree).await.1,
        Some(merged)
    );
    assert!(
        supervisor
            .record_branches(vec![(worktree, "next".to_owned())])
            .await
    );
    let shown = timeout(Duration::from_secs(5), async {
        loop {
            if let Delta::WorkspaceUpserted(shown) = deltas.recv().await.unwrap()
                && shown.id == worktree
            {
                return shown;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        (shown.kind.branch(), shown.pull_request),
        ("next", None),
        "at once, not at the next look"
    );
}

#[tokio::test]
async fn while_no_client_watches_only_every_tenth_tick_looks() {
    let Setup {
        host, supervisor, ..
    } = setup().await;
    host.set("feature", Some(status(3, PullRequestState::Open)));
    for _ in 0..9 {
        assert!(supervisor.check_pull_requests().await);
    }
    // Each tick handled, and anything they started given time to ask.
    supervisor.workspaces().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(host.asked().is_empty(), "{:?}", host.asked());
    assert!(supervisor.check_pull_requests().await);
    timeout(Duration::from_secs(5), async {
        while host.asked().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the tenth tick looks");
}

#[tokio::test]
async fn a_look_that_never_comes_back_holds_the_next_ones_back_ten_ticks_only() {
    let Setup {
        host, supervisor, ..
    } = setup().await;
    let (_, _deltas) = supervisor.subscribe().await.unwrap();
    host.silent.store(true, std::sync::atomic::Ordering::SeqCst);
    let asked = || host.asked().len();
    assert!(supervisor.check_pull_requests().await);
    timeout(Duration::from_secs(5), async {
        while asked() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // While it is under way, no other look starts beside it.
    for _ in 0..9 {
        assert!(supervisor.check_pull_requests().await);
    }
    supervisor.workspaces().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(asked(), 1);
    // Ten ticks on, it is taken as lost.
    assert!(supervisor.check_pull_requests().await);
    timeout(Duration::from_secs(5), async {
        while asked() == 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a look after the lost one");
}
