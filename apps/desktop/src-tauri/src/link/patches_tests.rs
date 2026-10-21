use muster_core::protocol::{Delta, Snapshot};
use muster_core::{SessionStatus, Timestamp, WaitReason};
use muster_model::fixtures::{project, session, worktree};

use super::*;

/// A model of one project with one worktree and its two sessions.
fn world() -> (Model, Project, Workspace, Session, Session) {
    let app = project("app");
    let tree = worktree(app.id, "work");
    let one = session(tree.id, "one", 3);
    let mut shell = session(tree.id, "zsh", 4);
    shell.harness = "shell".to_owned();
    let mut model = Model::default();
    model.replace(Snapshot {
        projects: vec![app.clone()],
        workspaces: vec![tree.clone()],
        sessions: vec![one.clone(), shell.clone()],
        ..Snapshot::default()
    });
    (model, app, tree, one, shell)
}

fn status(session: SessionId, status: SessionStatus, unseen: bool) -> Delta {
    Delta::SessionStatus {
        session,
        status,
        unseen,
        since: Timestamp::from_millis(10),
    }
}

#[test]
fn the_first_patch_replaces_everything_and_the_next_says_only_what_changed() {
    let (mut model, app, tree, one, shell) = world();
    let mut patcher = Patcher::default();
    let first = patcher.patch(&model, BTreeMap::new()).unwrap();
    assert!(first.reset);
    assert_eq!(first.projects, std::slice::from_ref(&app));
    assert_eq!(first.workspaces, std::slice::from_ref(&tree));
    assert_eq!(first.sessions, [one.clone(), shell.clone()]);
    assert!(first.preferences.is_some() && first.plugins.is_some());
    assert_eq!(first.notifier, Some(false));
    assert_eq!(
        first.derived.rows,
        [
            Row::Project(app.id),
            Row::Workspace(tree.id),
            Row::Session(one.id),
            Row::Session(shell.id),
        ]
    );

    assert!(
        patcher.patch(&model, BTreeMap::new()).is_none(),
        "nothing changed"
    );

    model.apply(status(one.id, SessionStatus::Running, false));
    let next = patcher.patch(&model, BTreeMap::new()).unwrap();
    assert!(!next.reset);
    assert_eq!(next.sessions.len(), 1);
    assert_eq!(next.sessions[0].status, SessionStatus::Running);
    assert!(next.projects.is_empty() && next.workspaces.is_empty());
    assert!(next.preferences.is_none() && next.plugins.is_none() && next.notifier.is_none());
    assert_eq!(next.derived.counts.working, 1);
}

#[test]
fn deltas_between_two_patches_make_one_and_say_each_change_once() {
    let (mut model, _, tree, one, _) = world();
    let mut patcher = Patcher::default();
    patcher.patch(&model, BTreeMap::new());

    // A session comes and goes between two patches: never sent.
    let passing = session(tree.id, "passing", 5);
    model.apply(Delta::SessionUpserted(passing.clone()));
    model.apply(Delta::SessionRemoved { id: passing.id });
    // Another changes twice: sent once, as it is now.
    model.apply(status(one.id, SessionStatus::Running, false));
    model.apply(Delta::SessionActivity {
        session: one.id,
        activity: None,
    });
    model.apply(status(one.id, SessionStatus::Finished, true));
    let patch = patcher.patch(&model, BTreeMap::new()).unwrap();
    assert_eq!(patch.sessions.len(), 1);
    assert_eq!(patch.sessions[0].status, SessionStatus::Finished);
    assert!(patch.gone.sessions.is_empty());
    assert_eq!(patch.derived.needs_you, [one.id]);
}

#[test]
fn a_project_removed_takes_its_workspaces_and_sessions_with_it() {
    let (mut model, app, tree, one, shell) = world();
    let mut patcher = Patcher::default();
    patcher.patch(&model, BTreeMap::new());
    model.apply(Delta::ProjectRemoved { id: app.id });
    let patch = patcher.patch(&model, BTreeMap::new()).unwrap();
    assert_eq!(patch.gone.projects, [app.id]);
    assert_eq!(patch.gone.workspaces, [tree.id]);
    let mut gone = patch.gone.sessions.clone();
    gone.sort();
    let mut expected = vec![one.id, shell.id];
    expected.sort();
    assert_eq!(gone, expected);
    assert!(patch.derived.rows.is_empty());
}

#[test]
fn derived_is_the_model_s_needs_you_rows_and_counts_with_the_questions() {
    let (mut model, _, _, one, shell) = world();
    model.apply(status(
        one.id,
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
        true,
    ));
    // A shell is never an agent: never counted, never waiting.
    model.apply(status(shell.id, SessionStatus::Running, false));
    let questions = BTreeMap::from([(one.id, vec!["Allow it?".to_owned()])]);
    let derived = derive(&model, questions.clone());
    assert_eq!(derived.needs_you, [one.id]);
    assert_eq!(
        derived.counts,
        Counts {
            working: 0,
            waiting: 1,
            done: 0
        }
    );
    assert_eq!(derived.questions, questions);
    assert_eq!(derived.rows.len(), 4);
    // Nothing at work but the shell, which is no agent.
    assert!(derived.working.is_empty());
}

#[test]
fn arc_24_working_is_the_model_s_the_longest_at_work_first() {
    let (mut model, _, tree, one, _) = world();
    let mut two = session(tree.id, "two", 2);
    two.status = SessionStatus::Running;
    two.status_since = Timestamp::from_millis(5);
    model.apply(Delta::SessionUpserted(two.clone()));
    model.apply(status(one.id, SessionStatus::Running, false));
    let derived = derive(&model, BTreeMap::new());
    assert_eq!(derived.working, [two.id, one.id]);
    assert_eq!(derived.working, model.working());
}

#[test]
fn a_question_that_changes_alone_makes_a_patch_of_derived_only() {
    let (model, _, _, one, _) = world();
    let mut patcher = Patcher::default();
    patcher.patch(&model, BTreeMap::new());
    let asked = BTreeMap::from([(one.id, vec!["Which?".to_owned()])]);
    let patch = patcher.patch(&model, asked.clone()).unwrap();
    assert!(patch.sessions.is_empty());
    assert_eq!(patch.derived.questions, asked);
    assert!(patcher.patch(&model, asked).is_none());
}

#[test]
fn settings_plugins_resources_and_the_notifier_are_sent_when_they_change() {
    let (mut model, _, _, one, _) = world();
    let mut patcher = Patcher::default();
    patcher.patch(&model, BTreeMap::new());
    let report = ResourceReport {
        host_memory_used: 1,
        host_memory_total: 2,
        sessions: Vec::new(),
    };
    model.apply(Delta::Resources(report.clone()));
    model.apply(Delta::Notifier { ready: true });
    let mut preferences = Preferences::default();
    preferences.notch = !preferences.notch;
    model.apply(Delta::Preferences(preferences.clone()));
    let patch = patcher.patch(&model, BTreeMap::new()).unwrap();
    assert_eq!(patch.resources, Some(report));
    assert_eq!(patch.notifier, Some(true));
    assert_eq!(patch.preferences, Some(preferences));
    assert!(patch.plugins.is_none());
    assert!(patch.sessions.is_empty(), "{one:?} did not change");

    patcher.reset();
    assert!(patcher.patch(&model, BTreeMap::new()).unwrap().reset);
}

#[test]
fn a_patch_reads_as_the_webview_s_types_say() {
    let (model, _, tree, _, _) = world();
    let mut patcher = Patcher::default();
    let patch = patcher.patch(&model, BTreeMap::new()).unwrap();
    let json = serde_json::to_value(crate::contract::StateMessage::Patch {
        patch: Box::new(patch),
    })
    .unwrap();
    assert_eq!(json["kind"], "patch");
    assert_eq!(json["patch"]["reset"], true);
    assert_eq!(json["patch"]["derived"]["rows"][1]["kind"], "workspace");
    assert_eq!(
        json["patch"]["derived"]["rows"][1]["id"],
        serde_json::to_value(tree.id).unwrap()
    );
    assert_eq!(json["patch"]["derived"]["questions"], serde_json::json!({}));
    assert_eq!(json["patch"]["gone"]["sessions"], serde_json::json!([]));
}
