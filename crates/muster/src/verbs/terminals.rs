//! A workspace's terminals from a shell (`PRODUCT.md` §6b): `muster term
//! new` starts one, shown where the person is (`muster list terminals`
//! lists them); `muster rename` and `muster close` take any session.

use std::process::ExitCode;

use clap::Subcommand;
use muster_core::Paths;
use muster_core::protocol::{Command, Snapshot};

use super::targets::{self, workspace_name};
use super::workspaces;
use crate::commands::{self, shown};

#[derive(Debug, Subcommand)]
pub(crate) enum TermCommand {
    /// Start a terminal in the workspace you are in (or --in's), and show
    /// it here: in place of the Muster terminal you type in, else in this
    /// tab.
    New {
        /// Its name; the program in front in it names it otherwise.
        name: Option<String>,
        /// The workspace, as `muster list workspaces` names it.
        #[arg(long = "in", value_name = "WS")]
        in_workspace: Option<String>,
        /// Start it and return, without showing it.
        #[arg(long)]
        bg: bool,
    },
}

pub(crate) async fn run(paths: &Paths, command: TermCommand) -> super::Then {
    let ran = match command {
        TermCommand::New {
            name,
            in_workspace,
            bg,
        } => new(paths, name, in_workspace.as_deref(), bg).await,
    };
    super::Then::Exit(ran.unwrap_or_else(crate::fail))
}

/// The workspace a command is about: the one named, else the one the
/// person is in.
fn workspace_of<'a>(
    snapshot: &'a Snapshot,
    named: Option<&str>,
) -> Result<&'a muster_core::Workspace, String> {
    let folder = std::env::current_dir().unwrap_or_default();
    let here = targets::current_workspace(snapshot, super::started_by(), &folder);
    match named {
        Some(named) => targets::find_workspace(snapshot, named, here.map(|w| w.project)),
        None => here.ok_or_else(|| {
            "you are in no workspace: name one with --in (`muster list workspaces`)".to_owned()
        }),
    }
}

async fn new(
    paths: &Paths,
    name: Option<String>,
    named: Option<&str>,
    bg: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = crate::daemon_start::connect(paths).await?;
    let snapshot = connection.subscribe().await?;
    let workspace = workspace_of(&snapshot, named)?;
    let (id, place) = (workspace.id, workspace_name(&snapshot, workspace));
    let terminal = workspaces::start_terminal(&mut connection, id, name).await?;
    if bg {
        let snapshot = connection.subscribe().await?;
        let number = snapshot
            .sessions
            .iter()
            .find(|session| session.id == terminal)
            .and_then(|session| targets::number_of(&snapshot, session))
            .unwrap_or_default();
        println!(
            "Started terminal {number} in {}. `muster attach {}/{number}` shows it.",
            shown(&place),
            shown(&place)
        );
        return Ok(ExitCode::SUCCESS);
    }
    drop(connection);
    super::tab::show(paths, terminal).await
}

/// `muster rename <s> NAME`.
pub(crate) async fn rename(
    paths: &Paths,
    wanted: &str,
    name: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let title = name.join(" ");
    if title.trim().is_empty() {
        return Err("give it a name".into());
    }
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, wanted)?;
    connection
        .command(Command::RenameSession {
            session: session.id,
            title: title.clone(),
        })
        .await?;
    println!(
        "Renamed {} ({}) {}.",
        shown(&session.title),
        targets::short(session.id),
        shown(&title)
    );
    Ok(ExitCode::SUCCESS)
}

/// `muster close <s>`: stops it, after asking where it runs, and takes it
/// off the list.
pub(crate) async fn close(
    paths: &Paths,
    wanted: &str,
    yes: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, wanted)?;
    let (id, title) = (session.id, shown(&session.title));
    if !session.status.is_exited() {
        let question = format!("{title} is running: stop it and close it?");
        if !yes && !crate::person::agrees(&question)? {
            return Err("nothing was closed (--yes closes it without asking)".into());
        }
        connection
            .command(Command::StopSession { session: id })
            .await?;
        workspaces::stopped(&mut connection, &[id]).await?;
    }
    connection
        .command(Command::RemoveSession { session: id })
        .await?;
    println!("Closed {title} ({}).", targets::short(id));
    Ok(ExitCode::SUCCESS)
}
