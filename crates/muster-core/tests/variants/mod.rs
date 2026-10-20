//! Each protocol message's variants, numbered, so a variant added fails
//! to build `protocol_round_trip` until it has a sample there (`ARC-09`).

use muster_core::protocol::{ClientFrame, Command, Delta, Reply, ServerFrame};

/// Each variant's number, so a variant added fails to build here.
pub(crate) trait Variant {
    /// How many variants there are.
    const COUNT: usize;
    fn variant(&self) -> usize;
}

impl Variant for ClientFrame {
    const COUNT: usize = 12;
    fn variant(&self) -> usize {
        match self {
            Self::Hello(_) => 0,
            Self::Subscribe => 1,
            Self::Command { .. } => 2,
            Self::Attach { .. } => 3,
            Self::Detach { .. } => 4,
            Self::Watch { .. } => 5,
            Self::Input { .. } => 6,
            Self::Resize { .. } => 7,
            Self::Looking { .. } => 8,
            Self::Colours(_) => 9,
            Self::Keys { .. } => 10,
            Self::Notifier { .. } => 11,
        }
    }
}

impl Variant for ServerFrame {
    const COUNT: usize = 11;
    fn variant(&self) -> usize {
        match self {
            Self::Welcome(_) => 0,
            Self::Incompatible(_) => 1,
            Self::Snapshot(_) => 2,
            Self::Delta(_) => 3,
            Self::Reply { .. } => 4,
            Self::Error(_) => 5,
            Self::Replay { .. } => 6,
            Self::Output { .. } => 7,
            Self::Modes { .. } => 8,
            Self::Detached { .. } => 9,
            Self::Reveal { .. } => 10,
        }
    }
}

impl Variant for Command {
    const COUNT: usize = 41;
    fn variant(&self) -> usize {
        match self {
            Self::Ping => 0,
            Self::AddProject { .. } => 1,
            Self::RemoveProject { .. } => 2,
            Self::CreateWorktree { .. } => 3,
            Self::RemoveWorktree { .. } => 4,
            Self::StartSession { .. } => 5,
            Self::StopSession { .. } => 6,
            Self::CheckWorkspace { .. } => 7,
            Self::ReviewWorkspace { .. } => 8,
            Self::TidyCommits { .. } => 9,
            Self::MoveOnto { .. } => 10,
            Self::OpenPullRequest { .. } => 11,
            Self::RestartSession { .. } => 12,
            Self::RemoveSession { .. } => 13,
            Self::ListIssues { .. } => 14,
            Self::ViewIssue { .. } => 15,
            Self::CloseIssue { .. } => 16,
            Self::ProjectSettings { .. } => 17,
            Self::WorkspaceChanges { .. } => 18,
            Self::FileDiff { .. } => 19,
            Self::Timeline { .. } => 20,
            Self::EntryDiff { .. } => 21,
            Self::UndoPlan { .. } => 22,
            Self::Undo { .. } => 23,
            Self::Health => 24,
            Self::Reveal { .. } => 25,
            Self::ChangePreference(_) => 26,
            Self::ChangeSetting { .. } => 27,
            Self::StartRun { .. } => 28,
            Self::CancelRun { .. } => 29,
            Self::RetryRun { .. } => 30,
            Self::TakeOverRun { .. } => 31,
            Self::HandBackRun { .. } => 32,
            Self::RequestChanges { .. } => 33,
            Self::RunHistory { .. } => 34,
            Self::StartFromIssue { .. } => 35,
            Self::ExplainSession { .. } => 36,
            Self::ReadPreferences => 37,
            Self::RenameSession { .. } => 38,
            Self::ListModels { .. } => 39,
            Self::Plugin(_) => 40,
        }
    }
}

impl Variant for Reply {
    const COUNT: usize = 25;
    fn variant(&self) -> usize {
        match self {
            Self::Pong => 0,
            Self::ProjectAdded { .. } => 1,
            Self::WorktreeCreated { .. } => 2,
            Self::SessionStarted { .. } => 3,
            Self::Issues { .. } => 4,
            Self::IssueStarted { .. } => 5,
            Self::IssueViewed { .. } => 6,
            Self::RunStarted { .. } => 7,
            Self::RunHistory { .. } => 8,
            Self::PullRequestOpened { .. } => 9,
            Self::ProjectSettings(_) => 10,
            Self::Health(_) => 11,
            Self::Changes(_) => 12,
            Self::FileDiff(_) => 13,
            Self::Timeline(_) => 14,
            Self::UndoPlan(_) => 15,
            Self::Undone { .. } => 16,
            Self::Moved { .. } => 17,
            Self::RunTakenOver { .. } => 18,
            Self::Done => 19,
            Self::Explanation(_) => 20,
            Self::Models(_) => 21,
            Self::Plugins(_) => 22,
            Self::PluginPreview(_) => 23,
            Self::PluginLog { .. } => 24,
        }
    }
}

impl Variant for Delta {
    const COUNT: usize = 14;
    fn variant(&self) -> usize {
        match self {
            Self::ProjectUpserted(_) => 0,
            Self::ProjectRemoved { .. } => 1,
            Self::WorkspaceUpserted(_) => 2,
            Self::WorkspaceRemoved { .. } => 3,
            Self::SessionUpserted(_) => 4,
            Self::SessionRemoved { .. } => 5,
            Self::SessionStatus { .. } => 6,
            Self::SessionActivity { .. } => 7,
            Self::Resources(_) => 8,
            Self::RunUpserted(_) => 9,
            Self::LookedAt { .. } => 10,
            Self::Preferences(_) => 11,
            Self::Notifier { .. } => 12,
            Self::PluginsChanged(_) => 13,
        }
    }
}
