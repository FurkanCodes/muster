//! `muster run`: handing issues to the autopilot and following its runs
//! (`PRODUCT.md` §5).

use std::io::Read;
use std::path::Path;

use muster_core::protocol::{Command, Reply, Snapshot};
use muster_core::time::{between, span};
use muster_core::{Run, RunId, RunStage, RunTransition, Timestamp};

use crate::commands::{Outcome, connect, find_project, shown, unexpected};

pub(crate) async fn start(socket: &Path, project: &str, number: u64) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?;
    let name = project.name.clone();
    let reply = connection
        .command(Command::StartRun {
            project: project.id,
            number,
        })
        .await?;
    let Reply::RunStarted { id } = reply else {
        return Err(unexpected(&reply));
    };
    println!(
        "Picked up issue #{number} in {} as run {id}.\n`muster run show {number}` follows it.",
        shown(&name)
    );
    Ok(())
}

/// The runs going on, those waiting on the person first; with `all`, the
/// ended ones too.
pub(crate) async fn list(socket: &Path, all: bool) -> Outcome {
    let snapshot = connect(socket).await?.subscribe().await?;
    let now = now();
    let mut runs: Vec<&Run> = snapshot
        .runs
        .iter()
        .filter(|run| all || !run.stage.is_over())
        .collect();
    if runs.is_empty() {
        println!(
            "{}",
            if all || snapshot.runs.is_empty() {
                "No runs yet. `muster run start <project> <issue>` hands an issue to the autopilot."
            } else {
                "No runs going. `muster run list --all` shows the ended ones too."
            }
        );
        return Ok(());
    }
    runs.sort_by_key(|run| (order(run.stage), run.created_at));
    let rows: Vec<[String; 6]> = runs
        .iter()
        .map(|run| {
            [
                run.stage.label().to_owned(),
                format!("#{}", run.ticket.number),
                project_name(&snapshot, run),
                cut(&shown(&run.ticket.title), 48),
                format!("{}/{}", run.attempt, run.max_attempts),
                span(age(run, now)),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..6)
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for (run, row) in runs.iter().zip(&rows) {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            line.push_str(cell);
            let pad = widths[column] - cell.chars().count();
            line.push_str(&" ".repeat(pad + 2));
        }
        line.push_str(&cost(run.cost_micros).map_or(String::new(), |cost| format!("{cost}  ")));
        line.push_str(&run.id.to_string());
        println!("{line}");
        if let Some(reason) = &run.reason {
            println!("{}└ {}", " ".repeat(widths[0] + 2), shown(reason));
        }
    }
    Ok(())
}

/// One run: where it is, why, and how it got there.
pub(crate) async fn show(socket: &Path, wanted: &str) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let run = find_run(&snapshot, wanted)?.clone();
    let reply = connection
        .command(Command::RunHistory { run: run.id })
        .await?;
    let Reply::RunHistory { transitions } = reply else {
        return Err(unexpected(&reply));
    };
    print!("{}", describe(&snapshot, &run, &transitions, now()));
    Ok(())
}

pub(crate) async fn cancel(socket: &Path, wanted: &str) -> Outcome {
    let (run, mut connection) = command_target(socket, wanted).await?;
    connection
        .command(Command::CancelRun { run: run.id })
        .await?;
    println!(
        "Cancelled the run on #{}; its container goes, its record stays.",
        run.ticket.number
    );
    Ok(())
}

pub(crate) async fn retry(socket: &Path, wanted: &str, notes: Option<String>) -> Outcome {
    let notes = notes.map(read_text).transpose()?;
    let (run, mut connection) = command_target(socket, wanted).await?;
    connection
        .command(Command::RetryRun { run: run.id, notes })
        .await?;
    println!(
        "Sent the run on #{} back to work, with a fresh budget.",
        run.ticket.number
    );
    Ok(())
}

pub(crate) async fn request_changes(socket: &Path, wanted: &str, comments: String) -> Outcome {
    let comments = read_text(comments)?;
    let (run, mut connection) = command_target(socket, wanted).await?;
    connection
        .command(Command::RequestChanges {
            run: run.id,
            comments,
        })
        .await?;
    let draft = run
        .pr
        .as_ref()
        .map(|pr| format!("; pull request #{} is a draft again", pr.number))
        .unwrap_or_default();
    println!(
        "Sent the run on #{} back to its agent with your comments{draft}.",
        run.ticket.number
    );
    Ok(())
}

/// `-` reads the text from stdin, for long notes and comments.
fn read_text(text: String) -> Result<String, std::io::Error> {
    if text != "-" {
        return Ok(text);
    }
    let mut read = String::new();
    std::io::stdin().read_to_string(&mut read)?;
    Ok(read)
}

async fn command_target(
    socket: &Path,
    wanted: &str,
) -> Result<(Run, muster_client::Connection), Box<dyn std::error::Error>> {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let run = find_run(&snapshot, wanted)?.clone();
    Ok((run, connection))
}

/// A run by id, or by its issue's number (`42` or `#42`): the one going
/// on, else the latest.
pub(crate) fn find_run<'a>(snapshot: &'a Snapshot, wanted: &str) -> Result<&'a Run, String> {
    if let Ok(id) = wanted.parse::<RunId>() {
        return snapshot
            .runs
            .iter()
            .find(|run| run.id == id)
            .ok_or_else(|| format!("no run {id}"));
    }
    let Ok(number) = wanted.trim_start_matches('#').parse::<u64>() else {
        return Err(format!(
            "{wanted:?} is neither a run's id nor an issue's number"
        ));
    };
    let on_issue: Vec<&Run> = snapshot
        .runs
        .iter()
        .filter(|run| run.ticket.number == number)
        .collect();
    let going: Vec<&Run> = on_issue
        .iter()
        .copied()
        .filter(|run| !run.stage.is_over())
        .collect();
    let one_project = on_issue
        .iter()
        .all(|run| run.project == on_issue[0].project);
    let several = || format!("issue #{number} has runs in several projects; use the run's id");
    match going.as_slice() {
        [run] => Ok(run),
        [] if on_issue.is_empty() => Err(format!("no run on issue #{number}")),
        [] if !one_project => Err(several()),
        [] => Ok(on_issue
            .iter()
            .max_by_key(|run| run.created_at)
            .expect("not empty")),
        _ => Err(several()),
    }
}

/// A run as `muster run show` prints it.
fn describe(
    snapshot: &Snapshot,
    run: &Run,
    transitions: &[RunTransition],
    now: Timestamp,
) -> String {
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(format!(
        "Issue #{} in {}: {}",
        run.ticket.number,
        project_name(snapshot, run),
        shown(&run.ticket.title)
    ));
    line(shown(&run.ticket.url));
    line(String::new());
    let stage = match &run.reason {
        Some(reason) => format!("{}: {}", run.stage.label(), shown(reason)),
        None => run.stage.label().to_owned(),
    };
    line(format!("stage         {stage}"));
    line(format!("branch        {}", shown(&run.branch)));
    line(if run.attempt == 0 {
        format!("attempt       none yet, {} allowed", run.max_attempts)
    } else {
        format!("attempt       {} of {}", run.attempt, run.max_attempts)
    });
    if let Some(pr) = &run.pr {
        line(format!("pull request  #{} {}", pr.number, shown(&pr.url)));
    }
    if let Some(cost) = cost(run.cost_micros) {
        line(format!("cost          {cost}"));
    }
    let age = match run.ended_at {
        Some(ended) => format!(
            "ended {} ago, after {}",
            span(between(ended, now)),
            span(between(run.created_at, ended))
        ),
        None => format!("picked up {} ago", span(between(run.created_at, now))),
    };
    line(format!("time          {age}"));
    line(format!("run           {}", run.id));
    if !transitions.is_empty() {
        line(String::new());
        line("Timeline".to_owned());
        let offsets: Vec<String> = transitions
            .iter()
            .map(|step| format!("+{}", span(between(run.created_at, step.at))))
            .collect();
        let width = offsets.iter().map(String::len).max().unwrap_or(0);
        let label = transitions
            .iter()
            .map(|step| step.to.label().len())
            .max()
            .unwrap_or(0);
        for (step, offset) in transitions.iter().zip(&offsets) {
            line(format!(
                "  {offset:<width$}  {:<label$}  {}",
                step.to.label(),
                shown(&step.reason)
            ));
        }
    }
    if let Some(next) = next_steps(run) {
        line(String::new());
        line(next);
    }
    out
}

/// What the person can do about a run that waits on them.
fn next_steps(run: &Run) -> Option<String> {
    let number = run.ticket.number;
    match run.stage {
        RunStage::NeedsYou => Some(format!(
            "`muster run retry {number} [--notes …]` sends it back to work with a fresh budget;\n\
             `muster run cancel {number}` ends it."
        )),
        RunStage::ReadyForYou => Some(format!(
            "Review {} on GitHub. Merging it ends the run;\n\
             `muster run changes {number} \"…\"` sends it back to its agent with what to change.",
            run.pr
                .as_ref()
                .map_or("its pull request".to_owned(), |pr| format!(
                    "pull request #{}",
                    pr.number
                ))
        )),
        _ => None,
    }
}

/// Where a stage sorts in a list: what waits on the person first, then
/// the work in the order it happens, then what has ended.
fn order(stage: RunStage) -> u8 {
    match stage {
        RunStage::NeedsYou => 0,
        RunStage::ReadyForYou => 1,
        RunStage::TakenOver => 2,
        RunStage::AgentReview => 3,
        RunStage::OpeningPr => 4,
        RunStage::Checking => 5,
        RunStage::Building => 6,
        RunStage::Provisioning => 7,
        RunStage::Queued => 8,
        RunStage::Merged => 9,
        RunStage::Cancelled => 10,
    }
}

fn project_name(snapshot: &Snapshot, run: &Run) -> String {
    snapshot
        .projects
        .iter()
        .find(|project| project.id == run.project)
        .map_or_else(|| run.project.to_string(), |project| shown(&project.name))
}

/// How long the run has gone on, or went on.
fn age(run: &Run, now: Timestamp) -> u64 {
    between(run.created_at, run.ended_at.unwrap_or(now))
}

/// Dollars, where the harness reported a cost.
fn cost(micros: Option<u64>) -> Option<String> {
    micros.map(|micros| format!("${:.2}", micros as f64 / 1_000_000.0))
}

/// `text` cut to `max` characters, marked when cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(max.saturating_sub(1)).collect();
    kept.push('…');
    kept
}

fn now() -> Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    Timestamp::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests;
