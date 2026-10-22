//! Every protocol message survives the trip through a frame (`ARC-09`):
//! each variant of every frame, command, reply and delta, with the values
//! inside them filled in, is read back as it was sent.
//!
//! A variant added to one of these enums fails to build `variant` below
//! until it is numbered there; give it the next number, raise its count,
//! and add a sample of it here.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failed step is the test failing"
)]

mod plugin_samples;
mod variants;

use std::path::PathBuf;

use muster_core::changes::{Commit, LineCounts};
use muster_core::clashes::{Clash, ClashFile};
use muster_core::colours::{Rgb, TerminalColours};
use muster_core::explain::{
    Deadline, DeadlineKind, Explanation, HoldView, MachineView, OpenWait, SignalSeen,
};
use muster_core::frame::{FrameDecoder, encode};
use muster_core::layout::Layout;
use muster_core::protocol::agents::AgentFound;
use muster_core::protocol::{
    ClientFrame, Command, Delta, DetachReason, ErrorBody, ErrorCode, Health, Hello, Incompatible,
    MouseEncoding, MouseTracking, PixelSize, Reply, ResourceReport, ServerFrame, SessionUsage,
    Snapshot, TermSize, TerminalModes, Welcome,
};
use muster_core::session::Origin;
use muster_core::settings::{
    AddedAgent, Palette, PreferenceChange, Preferences, ProjectSettings, SessionLines,
    SettingChange, SettingSource, TicketsFrom, Toggle,
};
use muster_core::stacks::Stacked;
use muster_core::telling::{Kind, Sound, Tell, Telling};
use muster_core::tidy::{Tidy, TidyCheck};
use muster_core::time::Timestamp;
use muster_core::timeline::{BranchMove, Entry, EntryKind, Timeline, UndoPlan};
use muster_core::{
    Activity, ChangeKind, ChangePart, ChangeSummary, Changes, Checks, FileChange, FileDiff, Issue,
    Project, ProjectId, PullRequest, PullRequestState, PullRequestStatus, Review, Run, RunId,
    RunStage, RunTransition, Session, SessionId, SessionStatus, Spend, Ticket, TicketKey,
    WaitReason, Workspace, WorkspaceId, WorkspaceKind,
};

fn at(offset: i64) -> Timestamp {
    Timestamp::from_millis(1_760_000_000_000 + offset)
}

fn size() -> TermSize {
    TermSize {
        cols: 120,
        rows: 40,
    }
}

/// Repositories on GitHub, one with all it can say.
fn hosted() -> Vec<muster_core::protocol::repositories::HostedRepository> {
    use muster_core::protocol::repositories::HostedRepository;
    vec![
        HostedRepository {
            name: "o/app".to_owned(),
            description: Some("The app".to_owned()),
            private: true,
        },
        HostedRepository {
            name: "o/lib".to_owned(),
            description: None,
            private: false,
        },
    ]
}

fn key() -> TicketKey {
    TicketKey::parse("ENG-42").unwrap()
}

fn project() -> Project {
    Project {
        id: ProjectId::new(),
        name: "app".to_owned(),
        root: PathBuf::from("/src/app"),
        created_at: at(1),
    }
}

/// A workspace of each kind, the first with everything it can say.
fn workspaces() -> Vec<Workspace> {
    let full = Workspace {
        id: WorkspaceId::new(),
        project: ProjectId::new(),
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from("/w/app/work"),
            branch: "muster/issue-42-fix".to_owned(),
        },
        created_at: at(2),
        pull_request: Some(PullRequestStatus {
            number: 17,
            url: "https://github.com/o/app/pull/17".to_owned(),
            state: PullRequestState::Open,
            checks: Some(Checks::Failing),
            review: Some(Review::ChangesRequested),
        }),
        changes: Some(ChangeSummary {
            files: 5,
            added: 120,
            removed: 30,
        }),
        timeline: Some(9),
        clashes: vec![Clash {
            with: WorkspaceId::new(),
            files: 3,
            conflicts: 1,
            shown: vec![ClashFile {
                path: "src/view.rs".to_owned(),
                conflict: true,
            }],
        }],
        stacked_on: Some(Stacked {
            branch: "muster/base".to_owned(),
            at: "a1b2c3d".to_owned(),
        }),
        ticket: Some(key()),
    };
    let checkout = Workspace {
        id: WorkspaceId::new(),
        kind: WorkspaceKind::Checkout {
            path: PathBuf::from("/src/app"),
            branch: String::new(),
        },
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
        ticket: None,
        ..full.clone()
    };
    let container = Workspace {
        id: WorkspaceId::new(),
        kind: WorkspaceKind::Container {
            run: RunId::new(),
            branch: "muster/issue-7".to_owned(),
        },
        ..checkout.clone()
    };
    vec![full, checkout, container]
}

/// A session in each status, the first with everything it can say.
fn sessions() -> Vec<Session> {
    let full = Session {
        id: SessionId::new(),
        workspace: WorkspaceId::new(),
        harness: "claude".to_owned(),
        title: "Fix the login".to_owned(),
        status: SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        },
        unseen: true,
        resume_id: Some("0f8b".to_owned()),
        created_at: at(3),
        ended_at: Some(at(9)),
        status_since: at(4),
        activity: Some(Activity {
            text: "editing view.rs".to_owned(),
            since: at(5),
        }),
        spend: Some(Spend {
            tokens: 40_000,
            cost_micros: Some(420_000),
        }),
        tidy: Some(Tidy {
            commits: 5,
            from: "a1b2c3d".to_owned(),
            checked: Some(TidyCheck::Astray {
                reason: "the branch moved".to_owned(),
            }),
        }),
        beside: Some(SessionId::new()),
    };
    let statuses = [
        SessionStatus::Fresh,
        SessionStatus::Running,
        SessionStatus::Finished,
        SessionStatus::Exited { success: false },
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        },
    ];
    let mut all = vec![full.clone()];
    all.extend(statuses.into_iter().map(|status| Session {
        id: SessionId::new(),
        status,
        resume_id: None,
        ended_at: None,
        activity: None,
        spend: None,
        tidy: None,
        beside: None,
        ..full.clone()
    }));
    all
}

fn run() -> Run {
    Run {
        id: RunId::new(),
        project: ProjectId::new(),
        ticket: Ticket {
            number: 42,
            title: "Fix the login".to_owned(),
            url: "https://github.com/o/app/issues/42".to_owned(),
            body: "It fails.".to_owned(),
            labels: vec!["bug".to_owned()],
            author: Some("someone".to_owned()),
        },
        branch: "muster/issue-42".to_owned(),
        stage: RunStage::AgentReview,
        attempt: 2,
        max_attempts: 3,
        reason: Some("checks failed".to_owned()),
        pr: Some(PullRequest {
            number: 17,
            url: "https://github.com/o/app/pull/17".to_owned(),
        }),
        cost_micros: Some(1_250_000),
        created_at: at(6),
        ended_at: None,
    }
}

fn issue() -> Issue {
    Issue {
        key: key(),
        title: "Fix the login".to_owned(),
        url: "https://acme.atlassian.net/browse/ENG-42".to_owned(),
        labels: vec!["bug".to_owned()],
        author: None,
    }
}

fn report() -> ResourceReport {
    ResourceReport {
        host_memory_used: 8 << 30,
        host_memory_total: 16 << 30,
        sessions: vec![SessionUsage {
            session: SessionId::new(),
            memory_bytes: 300 << 20,
            cpu_permille: 1500,
        }],
    }
}

fn preferences() -> Preferences {
    Preferences {
        notch: true,
        file: PathBuf::from("/home/me/.config/muster/settings.toml"),
        agents: vec![AddedAgent {
            name: "my-agent".to_owned(),
            label: "My Agent".to_owned(),
            mark: "M".to_owned(),
            colour: None,
            takes_prompt: true,
            asks: true,
        }],
        problem: Some("agents.other: `program` is missing".to_owned()),
        side_by_side_from: 120,
        palette: Palette::Terminal,
        session_lines: SessionLines::One,
        panels: Layout::parse("session | list 30 | (changes / shell)").unwrap(),
        list_folded: true,
        naming: muster_core::settings::Naming {
            by_agent: false,
            agent: Some("claude".to_owned()),
            model: Some("sonnet".to_owned()),
        },
        keys: [("new_session".to_owned(), vec!["N".to_owned()])].into(),
        notifications: false,
        telling: Telling::default().with(
            Kind::Finish,
            Tell {
                on: false,
                sound: Sound::Named("Glass".to_owned()),
            },
        ),
        shared_builds: false,
    }
}

fn explanation() -> Explanation {
    Explanation {
        session: SessionId::new(),
        status: SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
        since: at(0),
        unseen: true,
        now: at(9),
        machine: Some(MachineView {
            watched: false,
            waits: vec![OpenWait {
                origin: Origin::Subagent("a1".to_owned()),
                reason: WaitReason::Question,
            }],
            subagents: vec!["a1".to_owned()],
            hold: Some(HoldView {
                since: at(1),
                drained_at: Some(at(2)),
            }),
            awaiting_start: false,
            next: Some(Deadline {
                at: at(3),
                kind: DeadlineKind::Quiet,
            }),
        }),
        signals: vec![SignalSeen {
            at: at(4),
            what: "hook: question asked by subagent a1".to_owned(),
            tool: Some("AskUserQuestion".to_owned()),
        }],
    }
}

fn tickets_from() -> TicketsFrom {
    TicketsFrom {
        tracker: "jira".to_owned(),
        fields: [
            ("site", "acme.atlassian.net"),
            ("project", "ENG"),
            ("jql", "assignee = currentUser()"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect(),
    }
}

fn project_settings() -> ProjectSettings {
    ProjectSettings {
        tickets_from: "Jira (acme.atlassian.net)".to_owned(),
        only_mine: Toggle {
            on: true,
            from: SettingSource::Yours,
        },
        file: PathBuf::from("/home/me/.config/muster/projects/app.toml"),
        tickets: Some(tickets_from()),
        tickets_set_by: SettingSource::Project,
        tickets_untold: true,
        muster_prefix: Some(Toggle {
            on: false,
            from: SettingSource::Default,
        }),
    }
}

fn changes() -> Changes {
    Changes {
        base: Some("main".to_owned()),
        uncommitted: vec![FileChange {
            path: "notes.md".to_owned(),
            from: None,
            kind: ChangeKind::Untracked,
            lines: None,
        }],
        commits: vec![Commit {
            id: "a1b2c3d".to_owned(),
            subject: "Fix the login".to_owned(),
        }],
        more_commits: 2,
        branch: vec![FileChange {
            path: "src/new.rs".to_owned(),
            from: Some("src/old.rs".to_owned()),
            kind: ChangeKind::Renamed,
            lines: Some(LineCounts {
                added: 12,
                removed: 3,
            }),
        }],
    }
}

fn timeline() -> Timeline {
    let turn = Entry {
        number: 4,
        kind: EntryKind::Turn {
            session: SessionId::new(),
            agent: "Claude Code".to_owned(),
        },
        started: at(7),
        ended: at(8),
        summary: Some(ChangeSummary {
            files: 1,
            added: 2,
            removed: 0,
        }),
        head_before: Some("a1b2c3d".to_owned()),
        head_after: Some("d4e5f6a".to_owned()),
    };
    let kinds = [
        EntryKind::Between,
        EntryKind::Undo { of: 2 },
        EntryKind::Moved {
            onto: "main".to_owned(),
        },
    ];
    let mut entries = vec![turn.clone()];
    entries.extend(kinds.into_iter().map(|kind| Entry {
        kind,
        summary: None,
        head_before: None,
        head_after: None,
        ..turn.clone()
    }));
    Timeline { entries }
}

#[expect(clippy::too_many_lines, reason = "a sample of each command")]
fn commands() -> Vec<Command> {
    let (project, workspace, session, run) = (
        ProjectId::new(),
        WorkspaceId::new(),
        SessionId::new(),
        RunId::new(),
    );
    let mut all = vec![
        Command::Ping,
        Command::AddProject {
            root: PathBuf::from("/src/app"),
            name: Some("App".to_owned()),
        },
        Command::RemoveProject { project },
        Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
            on: Some(workspace),
        },
        Command::RemoveWorktree {
            workspace,
            force: true,
        },
        Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: Some("Fix".to_owned()),
            size: size(),
            prompt: Some("Fix the login".to_owned()),
        },
        Command::StopSession { session },
        Command::CheckWorkspace {
            workspace,
            size: size(),
        },
        Command::ReviewWorkspace {
            workspace,
            size: size(),
        },
        Command::TidyCommits {
            workspace,
            harness: "codex".to_owned(),
            size: size(),
        },
        Command::MoveOnto { workspace },
        Command::OpenPullRequest { workspace },
        Command::RestartSession {
            session,
            size: size(),
        },
        Command::RemoveSession { session },
        Command::RenameSession {
            session,
            title: "Fix the login".to_owned(),
        },
        Command::ListIssues { project },
        Command::ViewIssue {
            project,
            key: key(),
        },
        Command::CloseIssue {
            project,
            key: TicketKey::number(7),
        },
        Command::ProjectSettings { project },
        Command::WorkspaceChanges { workspace },
        Command::FileDiff {
            workspace,
            part: ChangePart::Branch,
            path: "src/view.rs".to_owned(),
        },
        Command::Timeline { workspace },
        Command::EntryDiff {
            workspace,
            number: 3,
        },
        Command::UndoPlan {
            workspace,
            number: 3,
        },
        Command::Undo {
            workspace,
            number: 3,
            move_branch: true,
        },
        Command::Health,
        Command::ExplainSession { session },
        Command::ReadPreferences,
        Command::Agents,
        Command::ListRepositories,
        Command::CloneProject {
            repository: "o/app".to_owned(),
            into: PathBuf::from("/Users/you/code"),
        },
        Command::Reveal { session },
        Command::ChangePreference(PreferenceChange::Notch(false)),
        Command::ChangePreference(PreferenceChange::SideBySideFrom(0)),
        Command::ChangePreference(PreferenceChange::Palette(Palette::Terminal)),
        Command::ChangePreference(PreferenceChange::SessionLines(SessionLines::One)),
        Command::ChangePreference(PreferenceChange::Panels(Layout::default())),
        Command::ChangePreference(PreferenceChange::ListFolded(true)),
        Command::ChangePreference(PreferenceChange::NamesByAgent(false)),
        Command::ChangePreference(PreferenceChange::NamedBy {
            agent: Some("codex".to_owned()),
            model: None,
        }),
        Command::ListModels {
            agent: "opencode".to_owned(),
        },
        Command::ChangePreference(PreferenceChange::Keys {
            action: "leave_session".to_owned(),
            keys: Some(vec!["Q".to_owned(), "ctrl-]".to_owned()]),
        }),
        Command::ChangePreference(PreferenceChange::Keys {
            action: "rename".to_owned(),
            keys: None,
        }),
        Command::ChangePreference(PreferenceChange::AllKeysBack),
        Command::ChangePreference(PreferenceChange::Notifications(false)),
        Command::ChangePreference(PreferenceChange::Tell {
            kind: Kind::Error,
            tell: Tell {
                on: true,
                sound: Sound::Silent,
            },
        }),
        Command::ChangePreference(PreferenceChange::SharedBuilds(false)),
        Command::ChangeSetting {
            project,
            change: SettingChange::TicketsFrom(Some(tickets_from())),
        },
        Command::StartRun {
            project,
            number: 42,
            from: Some(workspace),
        },
        Command::CancelRun { run },
        Command::RetryRun {
            run,
            notes: Some("try the other API".to_owned()),
        },
        Command::TakeOverRun { run },
        Command::HandBackRun { run },
        Command::RequestChanges {
            run,
            comments: "Rename it.".to_owned(),
        },
        Command::RunHistory { run },
        Command::StartFromIssue {
            project,
            key: key(),
            harness: "claude".to_owned(),
            size: size(),
            another: true,
            on: Some(workspace),
            brief: None,
        },
    ];
    all.extend(plugin_samples::commands());
    all
}

fn comment() -> muster_core::IssueComment {
    muster_core::IssueComment {
        author: Some("octocat".to_owned()),
        at: Some(muster_core::Timestamp::from_millis(1_792_661_400_000)),
        body: "Seen on **main** too.".to_owned(),
    }
}

fn replies() -> Vec<Reply> {
    let mut all = vec![
        Reply::Pong,
        Reply::ProjectAdded {
            id: ProjectId::new(),
        },
        Reply::WorktreeCreated {
            id: WorkspaceId::new(),
        },
        Reply::SessionStarted {
            id: SessionId::new(),
        },
        Reply::Issues {
            issues: vec![issue()],
            tracker: "Jira (acme.atlassian.net)".to_owned(),
            mine: true,
            source: Some("jira acme.atlassian.net project = ENG".to_owned()),
        },
        Reply::IssueStarted {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
        },
        Reply::IssueViewed {
            issue: issue(),
            body: "It fails.".to_owned(),
            comments: vec![comment()],
        },
        Reply::RunStarted { id: RunId::new() },
        Reply::RunHistory {
            transitions: vec![RunTransition {
                from: RunStage::Building,
                to: RunStage::Checking,
                reason: "built".to_owned(),
                at: at(10),
            }],
        },
        Reply::PullRequestOpened {
            pr: PullRequest {
                number: 17,
                url: "https://github.com/o/app/pull/17".to_owned(),
            },
        },
        Reply::ProjectSettings(project_settings()),
        Reply::Health(Health {
            tasks: 40,
            work: 1,
            clients: 2,
            terminals: 3,
        }),
        Reply::Changes(changes()),
        Reply::Explanation(Box::new(explanation())),
        Reply::Models(vec!["haiku".to_owned(), "sonnet".to_owned()]),
        Reply::Repositories(hosted()),
        Reply::Agents(vec![
            AgentFound {
                harness: "claude".to_owned(),
                path: Some("/opt/homebrew/bin/claude".to_owned()),
                version: Some("2.1.303".to_owned()),
            },
            AgentFound {
                harness: "copilot".to_owned(),
                path: None,
                version: None,
            },
        ]),
        Reply::FileDiff(FileDiff {
            text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
            cut: true,
        }),
        Reply::Timeline(timeline()),
        Reply::UndoPlan(UndoPlan {
            number: 3,
            later: 1,
            branch: Some(BranchMove {
                branch: "work".to_owned(),
                to: "a1b2c3d".to_owned(),
                commits: 2,
                forward: false,
                refused: Some("its commits are pushed".to_owned()),
            }),
        }),
        Reply::Undone {
            entry: 5,
            moved_branch: true,
        },
        Reply::Moved {
            onto: "main".to_owned(),
            commits: 4,
            entry: 6,
        },
        Reply::RunTakenOver {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
        },
        Reply::Done,
    ];
    all.extend(plugin_samples::replies());
    all
}

fn deltas() -> Vec<Delta> {
    let mut all = vec![
        Delta::ProjectUpserted(project()),
        Delta::ProjectRemoved {
            id: ProjectId::new(),
        },
        Delta::WorkspaceUpserted(workspaces().remove(0)),
        Delta::WorkspaceRemoved {
            id: WorkspaceId::new(),
        },
        Delta::SessionUpserted(sessions().remove(0)),
        Delta::SessionRemoved {
            id: SessionId::new(),
        },
        Delta::SessionStatus {
            session: SessionId::new(),
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Action,
            },
            unseen: true,
            since: at(11),
        },
        Delta::SessionActivity {
            session: SessionId::new(),
            activity: None,
        },
        Delta::Resources(report()),
        Delta::RunUpserted(run()),
        Delta::LookedAt { looked_at: true },
        Delta::Preferences(preferences()),
        Delta::Notifier { ready: true },
    ];
    all.extend(plugin_samples::deltas());
    all
}

fn client_frames() -> Vec<ClientFrame> {
    let session = SessionId::new();
    let mut frames = vec![
        ClientFrame::Hello(Hello {
            min_version: 16,
            max_version: 28,
            client: "muster-tui 0.3.25".to_owned(),
            capabilities: vec!["later".to_owned()],
        }),
        ClientFrame::Subscribe,
        ClientFrame::Attach {
            session,
            from_seq: Some(1 << 40),
            watching: false,
        },
        ClientFrame::Detach { session },
        ClientFrame::Watch {
            session,
            watching: true,
        },
        ClientFrame::Input {
            session,
            data: b"\x1b[200~pasted\x1b[201~".to_vec(),
        },
        ClientFrame::Resize {
            session,
            size: size(),
            pixels: Some(PixelSize {
                width: 1200,
                height: 720,
            }),
        },
        ClientFrame::Looking { looking: true },
        ClientFrame::Colours(TerminalColours {
            foreground: Some(Rgb {
                red: 0xffff,
                green: 0x8080,
                blue: 0,
            }),
            background: None,
            palette: vec![
                None,
                Some(Rgb {
                    red: 0x1010,
                    green: 0x2020,
                    blue: 0x3030,
                }),
            ],
        }),
        ClientFrame::Keys {
            session: Some(session),
        },
        ClientFrame::Notifier { ready: true },
    ];
    let commands = commands().into_iter().enumerate();
    frames.extend(commands.map(|(req_id, command)| ClientFrame::Command {
        req_id: req_id as u64,
        command,
    }));
    frames
}

fn server_frames() -> Vec<ServerFrame> {
    let session = SessionId::new();
    let mut frames = vec![
        ServerFrame::Welcome(Welcome {
            version: 28,
            daemon_pid: 4242,
            daemon_version: "0.3.25".to_owned(),
        }),
        ServerFrame::Incompatible(Incompatible {
            min_version: 16,
            max_version: 28,
            daemon_version: "0.3.25".to_owned(),
            daemon_pid: 4242,
        }),
        ServerFrame::Snapshot(Snapshot {
            projects: vec![project()],
            workspaces: workspaces(),
            sessions: sessions(),
            runs: vec![run()],
            resources: Some(report()),
            looked_at: true,
            preferences: preferences(),
            notifier: true,
            plugins: plugin_samples::plugins(),
        }),
        ServerFrame::Error(ErrorBody::new(ErrorCode::BadRequest, "malformed")),
        ServerFrame::Reply {
            req_id: 9,
            outcome: Err(ErrorBody::new(ErrorCode::Conflict, "still running")),
        },
        ServerFrame::Replay {
            session,
            seq: 7,
            data: vec![0xff, 0x1b, b'['],
            reset: true,
            live_from: None,
            screen: Some(b"\x1b[?1049l\x1b[r\x1b[H\x1b[Jdrawn".to_vec()),
        },
        ServerFrame::Replay {
            session,
            seq: 10,
            data: b"\x1b]52;c;aGk=\x07".to_vec(),
            reset: false,
            live_from: Some(8),
            screen: None,
        },
        ServerFrame::Output {
            session,
            seq: 10,
            data: b"\x1b[31mred\x1b[0m".to_vec(),
        },
        ServerFrame::Modes {
            session,
            modes: TerminalModes {
                kitty_flags: 1,
                bracketed_paste: true,
                application_cursor: true,
                mouse: MouseTracking::ButtonMotion,
                mouse_encoding: MouseEncoding::Sgr,
                alternate_screen: true,
            },
        },
        ServerFrame::Detached {
            session,
            reason: DetachReason::Exited,
        },
        ServerFrame::Reveal { session },
    ];
    let replies = replies().into_iter().enumerate();
    frames.extend(replies.map(|(req_id, reply)| ServerFrame::Reply {
        req_id: req_id as u64,
        outcome: Ok(reply),
    }));
    frames.extend(deltas().into_iter().map(ServerFrame::Delta));
    frames
}

/// Every variant of `T` has a sample among `samples`.
fn assert_every_variant<T: variants::Variant>(name: &str, samples: &[T]) {
    let mut seen = vec![false; T::COUNT];
    for sample in samples {
        seen[sample.variant()] = true;
    }
    let missing: Vec<usize> = (0..T::COUNT).filter(|at| !seen[*at]).collect();
    assert!(
        missing.is_empty(),
        "{name} variants {missing:?} have no sample"
    );
}

/// `message` read back from its frame is `message`.
fn assert_round_trip<T>(message: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let mut decoder = FrameDecoder::new();
    decoder.push(&encode(message).unwrap());
    assert_eq!(decoder.next_frame::<T>().unwrap().as_ref(), Some(message));
    assert!(!decoder.has_partial_frame());
}

#[test]
fn every_protocol_message_reads_back_as_it_was_sent_arc_09() {
    assert_every_variant("command", &commands());
    assert_every_variant("reply", &replies());
    assert_every_variant("delta", &deltas());
    let (client, server) = (client_frames(), server_frames());
    assert_every_variant("client frame", &client);
    assert_every_variant("server frame", &server);
    for frame in &client {
        assert_round_trip(frame);
    }
    for frame in &server {
        assert_round_trip(frame);
    }
}

/// Each change to a plugin, and who made it, reads back as it was sent.
#[test]
fn every_change_to_a_plugin_reads_back_as_it_was_sent_arc_09() {
    for change in plugin_samples::every_action() {
        assert_round_trip(&change);
    }
}

/// A replay from a daemon before protocol 33, which says nothing of what
/// in it is live, reads as all history, and one before 46 with no screen
/// to draw after it (`ARC-09`).
#[test]
fn an_older_daemon_s_replay_reads_as_history_arc_09() {
    #[derive(serde::Serialize)]
    enum Older {
        Replay {
            session: SessionId,
            seq: u64,
            #[serde(with = "serde_bytes")]
            data: Vec<u8>,
            reset: bool,
        },
    }
    let session = SessionId::new();
    let older = Older::Replay {
        session,
        seq: 3,
        data: b"abc".to_vec(),
        reset: false,
    };
    let mut decoder = FrameDecoder::new();
    decoder.push(&encode(&older).unwrap());
    let read: ServerFrame = decoder.next_frame().unwrap().unwrap();
    let expected = ServerFrame::Replay {
        session,
        seq: 3,
        data: b"abc".to_vec(),
        reset: false,
        live_from: None,
        screen: None,
    };
    assert_eq!(read, expected);
}
