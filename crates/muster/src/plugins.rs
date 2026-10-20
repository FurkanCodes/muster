//! `muster plugin`: the plugins the person installed (`ARCHITECTURE.md`
//! §7a, `PRD-15`). Installing, linking, updating and turning a plugin on
//! show everything it would run and ask first, on a terminal; without one
//! they are refused unless given `--yes`, and inside a Muster session they
//! are refused outright, so an agent cannot add a plugin by running a
//! command (`PRD-01`). The person does those on the Settings screen.

mod json;
mod shown;

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};

use clap::Subcommand;
use muster_core::plugins::{PLUGINS_PROTOCOL, PluginSource, TICKET_PLUGINS_PROTOCOL};
use muster_core::protocol::plugins::{
    ChangedBy, PluginCommand as Asked, PluginInfo, PluginList, PluginOrigin, PreviewKind,
};
use muster_core::protocol::{Command, Reply};

use crate::commands::{Outcome, connect, shown, unexpected};

#[derive(Debug, Subcommand)]
pub(crate) enum PluginCommand {
    /// List the plugins, Muster's own and those you added: where each is
    /// from, whether it is on, what it runs, and why one cannot run.
    List {
        /// Print them as one JSON document, for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Install a plugin from a git repository: its address (https://,
    /// ssh://, user@host:path or file://) or a repository's folder. Shows
    /// everything it would run, and asks.
    Install {
        source: String,
        /// The branch or tag to install, rather than its default branch;
        /// updates follow it.
        #[arg(long = "ref", value_name = "REF")]
        git_ref: Option<String>,
        /// Install it without asking, as when no terminal can ask.
        #[arg(long)]
        yes: bool,
    },
    /// Link FOLDER as a plugin where it is, for its author: Muster reads it
    /// there and never writes to it. Link it again to approve its manifest
    /// after changing it.
    Link {
        folder: PathBuf,
        /// Link it without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Turn plugin ID on, after showing what it runs.
    Enable {
        id: String,
        /// Turn it on without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Turn plugin ID off.
    Disable { id: String },
    /// Update plugin ID: fetch its repository again (a linked folder is
    /// read again as it is), show what it would run now, and ask.
    Update {
        id: String,
        /// Update it without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Remove plugin ID and its files in Muster's data folder, after
    /// asking. A linked folder stays where it is.
    Remove {
        id: String,
        /// Remove it without asking.
        #[arg(long)]
        yes: bool,
    },
    /// Show the end of plugin ID's log: what its program wrote on stderr
    /// while the daemon ran, its secrets hidden.
    Log { id: String },
}

pub(crate) async fn run(socket: &Path, command: PluginCommand) -> Outcome {
    match command {
        PluginCommand::List { json } => list(socket, json).await,
        PluginCommand::Install {
            source,
            git_ref,
            yes,
        } => {
            may_approve(yes)?;
            let origin = PluginOrigin::Git {
                url: address(&source)?,
                git_ref,
            };
            approve(socket, Asked::Preview { origin }, yes).await
        }
        PluginCommand::Link { folder, yes } => {
            may_approve(yes)?;
            let path = std::fs::canonicalize(&folder)
                .map_err(|error| format!("{}: {error}", shown(&folder.display().to_string())))?;
            let origin = PluginOrigin::Folder { path };
            approve(socket, Asked::Preview { origin }, yes).await
        }
        PluginCommand::Update { id, yes } => {
            may_approve(yes)?;
            approve(socket, Asked::Update { id }, yes).await
        }
        PluginCommand::Enable { id, yes } => {
            may_approve(yes)?;
            enable(socket, &id, yes).await
        }
        PluginCommand::Disable { id } => {
            let mut connection = open(socket).await?;
            let asked = Asked::SetEnabled {
                id: id.clone(),
                on: false,
                by: ChangedBy::CommandLine,
            };
            done(connection.command(Command::Plugin(asked)).await?)?;
            println!("Turned {} off.", shown(&id));
            Ok(())
        }
        PluginCommand::Remove { id, yes } => remove(socket, &id, yes).await,
        PluginCommand::Log { id } => log(socket, &id).await,
    }
}

/// `muster plugin log`.
async fn log(socket: &Path, id: &str) -> Outcome {
    let mut connection = open(socket).await?;
    let version = connection.welcome().version;
    if version < TICKET_PLUGINS_PROTOCOL {
        return Err(format!(
            "the daemon speaks protocol {version} and keeps no plugin's log, which needs \
             {TICKET_PLUGINS_PROTOCOL}; `muster reload` moves it onto the version installed now"
        )
        .into());
    }
    let asked = Asked::Log { id: id.to_owned() };
    match connection.command(Command::Plugin(asked)).await? {
        Reply::PluginLog { log, .. } if log.trim().is_empty() => {
            println!(
                "{} has written nothing since the daemon started.",
                shown(id)
            );
        }
        // What a plugin wrote is cleaned of control characters before a
        // terminal sees it.
        Reply::PluginLog { log, .. } => print!("{}", muster_core::text::multi_line(&log)),
        reply => return Err(unexpected(&reply)),
    }
    Ok(())
}

/// Refuses to install, approve or turn on a plugin inside a Muster
/// session, where an agent could have run the command, or without a
/// terminal to ask on unless `yes`.
fn may_approve(yes: bool) -> Result<(), String> {
    if std::env::var_os("MUSTER_SESSION").is_some_and(|session| !session.is_empty()) {
        return Err(
            "this runs inside a Muster session, where an agent could run it as well: \
                    install, update and turn on plugins on Muster's Settings screen (Plugins), \
                    or with `muster plugin` in a terminal of your own"
                .to_owned(),
        );
    }
    may_ask(yes)
}

/// Refuses to go on without a terminal to ask on, unless `yes`.
fn may_ask(yes: bool) -> Result<(), String> {
    if !yes && !std::io::stdin().is_terminal() {
        return Err(
            "there is no terminal to ask you on: run it in one, or add --yes once you \
                    have read what the plugin runs"
                .to_owned(),
        );
    }
    Ok(())
}

/// A repository's address as the daemon is given it: a folder's made
/// whole, so it is the same wherever the daemon runs.
fn address(source: &str) -> Result<String, String> {
    let path = Path::new(source);
    let looks_local = source.starts_with('.') || source.starts_with('/');
    if looks_local || (!source.contains(':') && path.exists()) {
        let whole =
            std::fs::canonicalize(path).map_err(|error| format!("{}: {error}", shown(source)))?;
        return Ok(whole.display().to_string());
    }
    Ok(source.to_owned())
}

/// Connected to a daemon that knows plugins.
async fn open(socket: &Path) -> Result<muster_client::Connection, Box<dyn std::error::Error>> {
    let connection = connect(socket).await?;
    let version = connection.welcome().version;
    if version < PLUGINS_PROTOCOL {
        return Err(format!(
            "the daemon speaks protocol {version} and knows no plugins, which need \
             {PLUGINS_PROTOCOL}; `muster reload` moves it onto the version installed now"
        )
        .into());
    }
    Ok(connection)
}

async fn plugins(
    connection: &mut muster_client::Connection,
) -> Result<PluginList, Box<dyn std::error::Error>> {
    match connection.command(Command::Plugin(Asked::List)).await? {
        Reply::Plugins(list) => Ok(list),
        reply => Err(unexpected(&reply)),
    }
}

fn done(reply: Reply) -> Outcome {
    match reply {
        Reply::Done => Ok(()),
        reply => Err(unexpected(&reply)),
    }
}

/// `muster plugin list`.
async fn list(socket: &Path, as_json: bool) -> Outcome {
    let mut connection = open(socket).await?;
    let list = plugins(&mut connection).await?;
    if as_json {
        json::print(&list)?;
        return Ok(());
    }
    print!("{}", shown::list(&list));
    Ok(())
}

/// Asks the daemon for a preview with `asked`, shows it, asks the person
/// (unless `yes`), and installs, links or updates what it showed.
async fn approve(socket: &Path, asked: Asked, yes: bool) -> Outcome {
    let mut connection = open(socket).await?;
    let preview = match connection.command(Command::Plugin(asked)).await? {
        Reply::PluginPreview(preview) => *preview,
        reply => return Err(unexpected(&reply)),
    };
    print!("{}", shown::preview(&preview));
    let question = match preview.kind {
        PreviewKind::Install => format!("Install {} and turn it on?", shown(&preview.id)),
        PreviewKind::Link if preview.current.is_some() => {
            format!("Approve {} as its folder holds it now?", shown(&preview.id))
        }
        PreviewKind::Link => format!("Link {} and turn it on?", shown(&preview.id)),
        PreviewKind::Update => format!("Update {}?", shown(&preview.id)),
    };
    if !confirmed(&question, yes)? {
        println!("Nothing changed.");
        return Ok(());
    }
    let install = Asked::Install {
        preview: preview.preview.clone(),
        by: ChangedBy::CommandLine,
    };
    done(connection.command(Command::Plugin(install)).await?)?;
    println!("{}", shown::approved(&preview));
    Ok(())
}

/// `muster plugin enable`.
async fn enable(socket: &Path, id: &str, yes: bool) -> Outcome {
    let mut connection = open(socket).await?;
    let list = plugins(&mut connection).await?;
    let plugin = find(&list, id)?;
    if let Some(problem) = &plugin.problem {
        return Err(format!("{} cannot be turned on: {problem}", shown(id)).into());
    }
    if plugin.enabled {
        println!("{} is on already.", shown(id));
        return Ok(());
    }
    print!("{}", shown::plugin(plugin, list.now));
    if !confirmed(&format!("Turn {} on?", shown(id)), yes)? {
        println!("Nothing changed.");
        return Ok(());
    }
    let asked = Asked::SetEnabled {
        id: id.to_owned(),
        on: true,
        by: ChangedBy::CommandLine,
    };
    done(connection.command(Command::Plugin(asked)).await?)?;
    println!("Turned {} on.", shown(id));
    Ok(())
}

/// `muster plugin remove`.
async fn remove(socket: &Path, id: &str, yes: bool) -> Outcome {
    may_ask(yes)?;
    let mut connection = open(socket).await?;
    let list = plugins(&mut connection).await?;
    let plugin = find(&list, id)?;
    let question = match &plugin.source {
        PluginSource::Folder { path } => format!(
            "Remove {}? Its folder, {}, stays where it is.",
            shown(id),
            shown(&path.display().to_string())
        ),
        _ => format!(
            "Remove {} and its files in Muster's data folder?",
            shown(id)
        ),
    };
    if !confirmed(&question, yes)? {
        println!("Nothing changed.");
        return Ok(());
    }
    let asked = Asked::Remove { id: id.to_owned() };
    done(connection.command(Command::Plugin(asked)).await?)?;
    println!("Removed {}.", shown(id));
    Ok(())
}

fn find<'a>(list: &'a PluginList, id: &str) -> Result<&'a PluginInfo, String> {
    list.plugins
        .iter()
        .find(|plugin| plugin.id == id)
        .ok_or_else(|| {
            let ids: Vec<&str> = list
                .plugins
                .iter()
                .map(|plugin| plugin.id.as_str())
                .collect();
            match ids.as_slice() {
                [] => format!("there is no plugin called {}, nor any other", shown(id)),
                ids => format!(
                    "there is no plugin called {}; there are {}",
                    shown(id),
                    ids.join(", ")
                ),
            }
        })
}

/// Whether the person says yes to `question` (`[y/N]`); yes at once with
/// `yes`.
fn confirmed(question: &str, yes: bool) -> std::io::Result<bool> {
    if yes {
        println!("{question} Yes, as --yes says.");
        return Ok(true);
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
