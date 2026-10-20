//! Samples of the plugins' messages (protocol 41, a panel opened from
//! 43), each variant with the values inside it filled in, for
//! `protocol_round_trip`.

use std::path::PathBuf;

use muster_core::plugins::{Panel, PanelOn, PluginSource, TicketSetting, TicketSource};
use muster_core::protocol::plugins::{
    Beside, ChangedBy, PluginAction, PluginChange, PluginCommand, PluginDetails, PluginInfo,
    PluginList, PluginOrigin, PluginPreview, PluginProblem, PreviewKind, Required,
};
use muster_core::protocol::{Command, Delta, Reply, TermSize};
use muster_core::time::Timestamp;
use muster_core::{SessionId, WorkspaceId};

fn details() -> PluginDetails {
    PluginDetails {
        name: "zoetrope".to_owned(),
        version: "0.1.0".to_owned(),
        description: Some("Watch a session as a live flow graph.".to_owned()),
        muster: "0.4.0".to_owned(),
        requires: vec![Required {
            program: "zoe".to_owned(),
            found: true,
        }],
        how_to_get: Some("cargo install zoetrope".to_owned()),
        env: vec!["ZOE_THEME".to_owned()],
        secrets: vec!["ZOE_TOKEN".to_owned()],
        panels: vec![Panel {
            id: "graph".to_owned(),
            title: "graph".to_owned(),
            on: PanelOn::Session,
            command: vec!["zoe".to_owned(), "{transcript}".to_owned()],
            key: Some("Z".to_owned()),
        }],
        tickets: Some(TicketSource {
            name: "YouTrack".to_owned(),
            noun: "issue".to_owned(),
            command: vec!["./youtrack".to_owned()],
            settings: vec![TicketSetting {
                key: "site".to_owned(),
                label: "Site".to_owned(),
                required: true,
            }],
        }),
        sha256: "9b".repeat(32),
    }
}

/// A plugin of each source, the first with everything it can say.
pub(crate) fn plugins() -> Vec<PluginInfo> {
    let git = PluginInfo {
        id: "zoetrope".to_owned(),
        source: PluginSource::Git {
            url: "https://github.com/someone/zoetrope".to_owned(),
            git_ref: Some("v0.1".to_owned()),
            commit: "4f".repeat(20),
        },
        enabled: true,
        problem: Some(PluginProblem::Changed),
        details: Some(details()),
        changed: Some(PluginChange {
            what: PluginAction::TurnedOn,
            by: ChangedBy::CommandLine,
            at: Timestamp::from_millis(1_760_000_000_000),
        }),
    };
    let folder = PluginInfo {
        id: "my-panel".to_owned(),
        source: PluginSource::Folder {
            path: PathBuf::from("/Users/me/my-panel"),
        },
        enabled: false,
        problem: Some(PluginProblem::Missing {
            path: PathBuf::from("/Users/me/my-panel/muster-plugin.toml"),
        }),
        details: None,
        changed: None,
    };
    let own = PluginInfo {
        id: "jira".to_owned(),
        source: PluginSource::Builtin,
        enabled: true,
        problem: None,
        details: None,
        changed: None,
    };
    let problems = [
        PluginProblem::NotApproved,
        PluginProblem::Unreadable {
            message: "id is missing".to_owned(),
        },
        PluginProblem::Unknown,
    ];
    let mut all = vec![git, folder, own];
    all.extend(problems.into_iter().map(|problem| PluginInfo {
        id: "other".to_owned(),
        source: PluginSource::Builtin,
        enabled: false,
        problem: Some(problem),
        details: None,
        changed: Some(PluginChange {
            what: PluginAction::Installed,
            by: ChangedBy::Ui,
            at: Timestamp::from_millis(1_760_000_000_001),
        }),
    }));
    all
}

/// Each plugin command, inside `Command::Plugin`.
pub(crate) fn commands() -> Vec<Command> {
    [
        PluginCommand::List,
        PluginCommand::Preview {
            origin: PluginOrigin::Git {
                url: "https://github.com/someone/zoetrope".to_owned(),
                git_ref: Some("main".to_owned()),
            },
        },
        PluginCommand::Preview {
            origin: PluginOrigin::Folder {
                path: PathBuf::from("/Users/me/my-panel"),
            },
        },
        PluginCommand::Update {
            id: "zoetrope".to_owned(),
        },
        PluginCommand::Install {
            preview: "ab".repeat(16),
            by: ChangedBy::Ui,
        },
        PluginCommand::SetEnabled {
            id: "zoetrope".to_owned(),
            on: false,
            by: ChangedBy::CommandLine,
        },
        PluginCommand::Remove {
            id: "zoetrope".to_owned(),
        },
        PluginCommand::Log {
            id: "youtrack".to_owned(),
        },
        PluginCommand::OpenPanel {
            plugin: "zoetrope".to_owned(),
            panel: "graph".to_owned(),
            beside: Beside::Session(SessionId::new()),
            size: TermSize { cols: 80, rows: 24 },
        },
        PluginCommand::OpenPanel {
            plugin: "zoetrope".to_owned(),
            panel: "files".to_owned(),
            beside: Beside::Workspace(WorkspaceId::new()),
            size: TermSize { cols: 60, rows: 20 },
        },
    ]
    .into_iter()
    .map(Command::Plugin)
    .collect()
}

pub(crate) fn replies() -> Vec<Reply> {
    let mut previews: Vec<Reply> = [PreviewKind::Install, PreviewKind::Link, PreviewKind::Update]
        .into_iter()
        .map(|kind| {
            Reply::PluginPreview(Box::new(PluginPreview {
                preview: "cd".repeat(16),
                kind,
                id: "zoetrope".to_owned(),
                source: PluginSource::Git {
                    url: "file:///tmp/zoetrope".to_owned(),
                    git_ref: None,
                    commit: "5a".repeat(20),
                },
                details: details(),
                current: (kind == PreviewKind::Update).then(|| plugins().remove(0)),
            }))
        })
        .collect();
    previews.push(Reply::Plugins(PluginList {
        plugins: plugins(),
        now: Timestamp::from_millis(1_760_000_100_000),
        problem: Some("settings.toml: notch.shwo is not a key".to_owned()),
    }));
    previews.push(Reply::PluginLog {
        id: "youtrack".to_owned(),
        log: "listing APP\nsigned in as someone\n".to_owned(),
    });
    previews
}

pub(crate) fn deltas() -> Vec<Delta> {
    vec![Delta::PluginsChanged(plugins())]
}

/// The actions and who made them, each once, so a new one needs a sample.
pub(crate) fn every_action() -> Vec<PluginChange> {
    [
        PluginAction::Installed,
        PluginAction::Linked,
        PluginAction::Updated,
        PluginAction::TurnedOn,
        PluginAction::TurnedOff,
    ]
    .into_iter()
    .zip([ChangedBy::Ui, ChangedBy::CommandLine].into_iter().cycle())
    .map(|(what, by)| PluginChange {
        what,
        by,
        at: Timestamp::from_millis(1),
    })
    .collect()
}
