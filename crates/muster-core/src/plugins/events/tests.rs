use std::path::PathBuf;

use super::*;
use crate::clashes::ClashFile;
use crate::{Activity, ProjectId, SessionId, WorkspaceId, WorkspaceKind};

fn project() -> Project {
    Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: PathBuf::from("/src/app"),
        created_at: Timestamp::from_millis(0),
    }
}

fn workspace(project: &Project, branch: &str) -> Workspace {
    Workspace {
        id: WorkspaceId::new(),
        project: project.id,
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from(format!("/w/app/{branch}")),
            branch: branch.to_owned(),
        },
        created_at: Timestamp::from_millis(0),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
    }
}

fn session(workspace: &Workspace, title: &str, status: SessionStatus) -> Session {
    Session {
        id: SessionId::new(),
        workspace: workspace.id,
        harness: "claude".to_owned(),
        title: title.to_owned(),
        status,
        unseen: true,
        resume_id: Some("4b1d-conversation".to_owned()),
        created_at: Timestamp::from_millis(0),
        ended_at: None,
        status_since: Timestamp::from_millis(0),
        activity: Some(Activity {
            text: "editing secret_plan.rs".to_owned(),
            since: Timestamp::from_millis(0),
        }),
        spend: None,
        tidy: None,
        beside: None,
        started_by: None,
    }
}

#[test]
fn every_kind_is_written_by_its_name_in_a_manifest_and_on_the_wire() {
    for kind in EventKind::ALL {
        assert_eq!(EventKind::named(kind.name()), Some(kind));
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            serde_json::Value::from(kind.name())
        );
        assert_eq!(kind.to_string(), kind.name());
    }
    assert_eq!(EventKind::named("session.done"), None);
}

#[test]
fn a_session_s_event_carries_names_and_ids_and_never_its_text() {
    let project = project();
    let mut worked = workspace(&project, "muster/issue-42-fix-login");
    worked.clashes = vec![Clash {
        with: WorkspaceId::new(),
        files: 1,
        conflicts: 1,
        shown: vec![ClashFile {
            path: "src/secret_plan.rs".to_owned(),
            conflict: true,
        }],
    }];
    let asking = session(
        &worked,
        "Use ghp_abcdefghijklmnopqrstuvwxyz0123 to fix the login",
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
    );
    let event = Event::new(
        EventKind::SessionNeedsYou,
        Timestamp::from_millis(1_760_000_000_000),
        Some(&project),
        Some(&worked),
    )
    .of_session(&asking);
    let json: serde_json::Value = serde_json::from_str(event.line().trim_end()).unwrap();
    assert_eq!(json["event"], "session.needs_you");
    assert_eq!(json["at"], 1_760_000_000_000_i64);
    assert_eq!(json["project"]["name"], "app");
    assert_eq!(json["project"]["root"], "/src/app");
    assert_eq!(json["workspace"]["branch"], "muster/issue-42-fix-login");
    assert_eq!(
        json["workspace"]["folder"],
        "/w/app/muster/issue-42-fix-login"
    );
    assert_eq!(json["ticket"], "42");
    assert_eq!(json["session"]["id"], asking.id.to_string());
    assert_eq!(json["session"]["harness"], "claude");
    assert_eq!(json["session"]["status"], "needs_you");
    assert_eq!(json["session"]["reason"], "permission");
    // Its chat's name, as the UI shows it: a credential hidden.
    assert_eq!(
        json["session"]["name"],
        "Use [hidden by Muster] to fix the login"
    );
    let line = event.line();
    for never in ["ghp_", "secret_plan", "4b1d-conversation", "\"sample\""] {
        assert!(!line.contains(never), "{never} in {line}");
    }
    assert!(line.ends_with('\n') && line.lines().count() == 1);
}

#[test]
fn an_ended_session_says_whether_it_ended_well() {
    let project = project();
    let worked = workspace(&project, "work");
    let failed = session(&worked, "x", SessionStatus::Exited { success: false });
    let event = Event::new(
        EventKind::SessionError,
        Timestamp::from_millis(0),
        Some(&project),
        Some(&worked),
    )
    .of_session(&failed);
    let told = event.session.unwrap();
    assert_eq!(
        (told.status.as_str(), told.success),
        ("exited", Some(false))
    );
    assert_eq!(told.reason, None);
    assert_eq!(event.ticket, None);
}

#[test]
fn a_pull_request_and_a_clash_are_told_by_their_numbers_never_their_files() {
    let project = project();
    let worked = workspace(&project, "work");
    let other = workspace(&project, "other");
    let pull = PullRequestStatus {
        number: 12,
        url: "https://github.com/o/app/pull/12".to_owned(),
        state: PullRequestState::Draft,
        checks: Some(Checks::Failing),
        review: None,
    };
    let clash = Clash {
        with: other.id,
        files: 3,
        conflicts: 1,
        shown: vec![ClashFile {
            path: "src/secret_plan.rs".to_owned(),
            conflict: true,
        }],
    };
    let event = Event::new(
        EventKind::ClashFound,
        Timestamp::from_millis(0),
        Some(&project),
        Some(&worked),
    )
    .with_pull_request(&pull)
    .with_clash(&clash, &other);
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["pull_request"]["number"], 12);
    assert_eq!(json["pull_request"]["state"], "draft");
    assert_eq!(json["pull_request"]["draft"], true);
    assert_eq!(json["pull_request"]["checks"], "failing");
    assert_eq!(json["clash"]["with"]["branch"], "other");
    assert_eq!(
        (
            json["clash"]["files"].clone(),
            json["clash"]["conflicts"].clone()
        ),
        (3.into(), 1.into())
    );
    assert!(!event.line().contains("secret_plan"));
    assert!(json.get("session").is_none());
}

#[test]
fn a_hook_s_command_is_filled_in_from_its_event_without_any_conversation() {
    let project = project();
    let worked = workspace(&project, "muster/issue-42-fix-login");
    let finished = session(&worked, "Fix it", SessionStatus::Finished);
    let context = Event::new(
        EventKind::SessionFinished,
        Timestamp::from_millis(0),
        Some(&project),
        Some(&worked),
    )
    .of_session(&finished)
    .context();
    assert_eq!(context.project.as_deref(), Some("app"));
    assert_eq!(context.branch.as_deref(), Some("muster/issue-42-fix-login"));
    assert_eq!(context.session, Some(finished.id.to_string()));
    assert_eq!(context.harness.as_deref(), Some("claude"));
    assert_eq!(context.ticket.as_deref(), Some("42"));
    assert_eq!((&context.resume, &context.transcript), (&None, &None));
    let filled = context
        .expand(&["./notify".to_owned(), "{project}:{branch}".to_owned()])
        .unwrap();
    assert_eq!(filled[1], "app:muster/issue-42-fix-login");
}

#[test]
fn a_sample_of_each_kind_has_what_an_event_of_its_kind_carries() {
    for kind in EventKind::ALL {
        let sample = Event::sample(kind, Timestamp::from_millis(0));
        assert!(sample.sample, "{kind}");
        assert_eq!(sample.session.is_some(), kind.is_a_session_s(), "{kind}");
        assert_eq!(
            sample.pull_request.is_some(),
            kind == EventKind::PullRequestChanged,
            "{kind}"
        );
        assert_eq!(
            sample.clash.is_some(),
            kind == EventKind::ClashFound,
            "{kind}"
        );
        assert_eq!(
            sample
                .session
                .as_ref()
                .and_then(|session| session.reason)
                .is_some(),
            kind.has_reason(),
            "{kind}"
        );
        assert!(sample.line().contains("\"sample\":true"));
        // Every placeholder a hook is told has a value in it.
        let told = sample.context();
        assert!(told.project.is_some() && told.branch.is_some() && told.workspace.is_some());
    }
}
