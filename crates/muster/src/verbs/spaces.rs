//! `muster space add|rename|mv|rm` and `muster list spaces` (`PRODUCT.md`
//! §6b): the person's spaces, where every project and terminal in no
//! project lives, one space each (`ARC-33`), from a shell.

use std::fmt::Write as _;
use std::process::ExitCode;

use clap::Subcommand;
use muster_core::protocol::{Command, Reply, Snapshot};
use muster_core::spaces::Member;
use muster_core::{Paths, Space};
use serde::Serialize;

use crate::commands::{self, shown, unexpected};

#[derive(Debug, Subcommand)]
pub(crate) enum SpaceCommand {
    /// Make a space called NAME.
    Add { name: Vec<String> },
    /// Call space NAME something else.
    Rename { name: String, new: Vec<String> },
    /// Move WHAT (a project's name, id or folder, or a terminal in no
    /// project) to space SPACE.
    Mv { what: String, space: String },
    /// Remove space NAME, once nothing is in it.
    #[command(visible_alias = "remove")]
    Rm { name: String },
}

pub(crate) async fn run(paths: &Paths, command: SpaceCommand) -> super::Then {
    let ran = change(paths, command).await;
    super::Then::Exit(ran.unwrap_or_else(crate::fail))
}

async fn change(
    paths: &Paths,
    command: SpaceCommand,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let (sent, said) = match command {
        SpaceCommand::Add { name } => {
            let name = name.join(" ");
            let said = format!("Made space {}.", shown(name.trim()));
            (Command::CreateSpace { name }, said)
        }
        SpaceCommand::Rename { name, new } => {
            let space = find(&snapshot, &name)?.id;
            let new = new.join(" ");
            let said = format!("Renamed {} {}.", shown(&name), shown(new.trim()));
            (Command::RenameSpace { space, name: new }, said)
        }
        SpaceCommand::Mv { what, space } => {
            let to = find(&snapshot, &space)?;
            let said = format!("Moved {} to {}.", shown(&what), shown(&to.name));
            let member = member(&snapshot, &what)?;
            (
                Command::MoveToSpace {
                    space: to.id,
                    member,
                },
                said,
            )
        }
        SpaceCommand::Rm { name } => {
            let space = find(&snapshot, &name)?.id;
            let said = format!("Removed space {}.", shown(&name));
            (Command::DeleteSpace { space }, said)
        }
    };
    let reply = connection.command(sent).await?;
    if !matches!(reply, Reply::Done | Reply::SpaceCreated { .. }) {
        return Err(unexpected(&reply));
    }
    println!("{said}");
    Ok(ExitCode::SUCCESS)
}

/// What `wanted` names that a space holds: a project (its name, id or
/// folder), else a terminal in no project.
fn member(snapshot: &Snapshot, wanted: &str) -> Result<Member, Box<dyn std::error::Error>> {
    if let Ok(project) = commands::find_project(snapshot, wanted) {
        return Ok(Member::Project(project.id));
    }
    let session = crate::verbs::targets::find_session(snapshot, wanted)?;
    if !session.is_free() {
        return Err(format!(
            "{} is in a project, which is where it shows: move the project",
            shown(wanted)
        )
        .into());
    }
    Ok(Member::Session(session.id))
}

/// The space called `wanted` (any case), or its id.
pub(crate) fn find<'a>(snapshot: &'a Snapshot, wanted: &str) -> Result<&'a Space, String> {
    snapshot
        .spaces
        .iter()
        .find(|space| {
            space.id.to_string() == wanted || space.name.to_lowercase() == wanted.to_lowercase()
        })
        .ok_or_else(|| format!("there is no space called {wanted} (`muster list spaces`)"))
}

#[derive(Debug, Serialize)]
struct SpaceEntry {
    id: String,
    name: String,
    projects: Vec<String>,
    terminals: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SpaceList {
    format: u32,
    spaces: Vec<SpaceEntry>,
}

/// `muster list spaces`: each space and what is in it.
pub(crate) fn list(
    snapshot: &Snapshot,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let entries: Vec<SpaceEntry> = snapshot
        .spaces
        .iter()
        .map(|space| {
            let (mut projects, mut terminals) = (Vec::new(), Vec::new());
            for member in &space.members {
                match member {
                    Member::Project(id) => projects.extend(
                        snapshot
                            .projects
                            .iter()
                            .find(|project| project.id == *id)
                            .map(|project| project.name.clone()),
                    ),
                    Member::Session(id) => terminals.extend(
                        snapshot
                            .sessions
                            .iter()
                            .find(|session| session.id == *id)
                            .map(|session| session.title.clone()),
                    ),
                }
            }
            SpaceEntry {
                id: space.id.to_string(),
                name: space.name.clone(),
                projects,
                terminals,
            }
        })
        .collect();
    if json {
        crate::json::print(&SpaceList {
            format: crate::json::FORMAT,
            spaces: entries,
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    if entries.is_empty() {
        println!("No spaces. `muster space add <name>` makes one.");
    }
    for entry in entries {
        let names = |names: &[String]| {
            names
                .iter()
                .map(|name| shown(name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut line = shown(&entry.name);
        if !entry.projects.is_empty() {
            let _ = write!(line, "  projects: {}", names(&entry.projects));
        }
        if !entry.terminals.is_empty() {
            let _ = write!(line, "  terminals: {}", names(&entry.terminals));
        }
        if entry.projects.is_empty() && entry.terminals.is_empty() {
            line.push_str("  (empty)");
        }
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}
