//! The pane beside the list when the list has the keys: the selected
//! session's terminal, or a worktree's or a project's details.

use muster_core::keys::Action as Does;
use muster_core::{Project, Workspace, WorkspaceKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};

use super::terminal::draw_session;
use super::{kep, panel};
use crate::app::{App, View};
use crate::model::Row;

pub(super) fn draw_preview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some(session) = app.focused_session() {
        draw_session(frame, app, session, area, false);
        return;
    }
    let text = match app
        .view
        .eq(&View::Sessions)
        .then_some(app.selected)
        .flatten()
    {
        Some(Row::Worktree(id)) => app
            .model
            .workspace(id)
            .map_or_else(Text::default, |workspace| workspace_details(app, workspace)),
        Some(Row::Project(id)) => app
            .model
            .project(id)
            .map_or_else(Text::default, |project| project_details(app, project)),
        _ if !app.model.loaded() => {
            return kep::draw_empty_pane(frame, app, area, "Connecting to the daemon…", &[]);
        }
        _ if app.model.projects().is_empty() => {
            let add = format!(
                "Press {} to add a project: a git repository, or any folder.",
                app.key(Does::AddProject)
            );
            return kep::draw_empty_pane(
                frame,
                app,
                area,
                "Agents on your tickets, side by side.",
                &[&add],
            );
        }
        _ => Text::from("Select a session to watch it here.").dim(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(app, " Details ", false)),
        area,
    );
}

/// A worktree's or a checkout's details, and what can be done with it.
fn workspace_details(app: &App, workspace: &Workspace) -> Text<'static> {
    let theme = super::theme::of(app);
    let id = workspace.id;
    let sessions = app.model.sessions_of(id).count();
    let mut lines = Vec::new();
    if !workspace.kind.in_git() {
        lines.push(
            Line::from("The project's own folder, where you work too. It is not a git repository.")
                .dim(),
        );
    } else if workspace.kind.is_checkout() {
        lines.push(Line::from("The project's own checkout, where you work too.").dim());
    }
    if let Some(run) = app.model.run_of(id) {
        lines.push(
            Line::from(format!(
                "The container of the run on #{} ({}), with a clone of its own.",
                run.ticket.number,
                run.stage.label()
            ))
            .dim(),
        );
    }
    if workspace.kind.in_git() {
        lines.push(Line::from(vec![
            Span::from("Branch  ").dim(),
            Span::raw(workspace.kind.branch().to_owned()),
        ]));
    }
    if let Some(summary) = &workspace.changes {
        let files = if summary.files == 1 {
            "1 file".to_owned()
        } else {
            format!("{} files", summary.files)
        };
        let mut spans = vec![Span::from("Changed ").dim()];
        if summary.added + summary.removed > 0 {
            spans.extend(super::changes::summary(summary, theme));
            spans.push(Span::raw(format!(" in {files}")));
        } else {
            spans.push(Span::raw(files));
        }
        lines.push(Line::from(spans));
    }
    if let Some(stacked) = &workspace.stacked_on {
        let merged = if app.stack_merged(workspace.id) {
            format!(
                " · it has merged: {} moves this onto its base",
                app.key(Does::MoveBranch)
            )
        } else {
            String::new()
        };
        lines.push(Line::from(vec![
            Span::from("Stacked ").dim(),
            Span::raw(format!(
                "on ⎇ {}{merged}",
                muster_core::text::one_line(&stacked.branch)
            )),
        ]));
    }
    lines.extend(super::clashes::detail_lines(app, &workspace.clashes));
    if let Some(found) = &workspace.pull_request {
        lines.push(Line::from(vec![
            Span::from("PR      ").dim(),
            Span::raw(found.summary()),
        ]));
    }
    lines.extend([
        Line::from(vec![
            Span::from("Path    ").dim(),
            Span::raw(workspace.kind.host_path().map_or_else(
                || "in the run's container".to_owned(),
                |path| path.display().to_string(),
            )),
        ]),
        Line::from(vec![
            Span::from("Sessions ").dim(),
            Span::raw(sessions.to_string()),
        ]),
        Line::from(""),
    ]);
    lines.extend(workspace_keys(app, workspace));
    Text::from(lines)
}

/// What the keys do on a worktree's or a checkout's line, said under its
/// details.
fn workspace_keys(app: &App, workspace: &Workspace) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let new = app.key(Does::NewSession);
    lines.push(
        Line::from(match &workspace.kind {
            WorkspaceKind::Checkout { .. } => format!("Press {new} or ⏎ to start a session here."),
            WorkspaceKind::Worktree { .. } => format!(
                "Press {new} or ⏎ to start a session here, {} to remove the worktree.",
                app.key(Does::Close)
            ),
            WorkspaceKind::Container { .. } => {
                format!("Press {new} or ⏎ for a shell in it; it goes when the run ends.")
            }
        })
        .dim(),
    );
    if workspace.kind.in_git() && workspace.kind.run().is_none() {
        lines.push(
            Line::from(format!(
                "{} shows what changed: each file, and its diff.",
                app.key(Does::ShowChanges)
            ))
            .dim(),
        );
    }
    if workspace.pull_request.is_some() {
        lines.push(
            Line::from(format!(
                "{} opens its pull request in the browser.",
                app.key(Does::PullRequestInBrowser)
            ))
            .dim(),
        );
    }
    lines
}

/// A project's details, and what can be done with it.
fn project_details(app: &App, project: &Project) -> Text<'static> {
    if app.model.is_plain_folder(project.id) {
        return Text::from(vec![
            Line::from(vec![
                Span::from("Folder  ").dim(),
                Span::raw(project.root.display().to_string()),
            ]),
            Line::from(""),
            Line::from(format!(
                "Press {} or ⏎ to start a session in the folder.",
                app.key(Does::NewSession)
            ))
            .dim(),
            Line::from(
                "It is not a git repository, so it has no worktrees or tickets; git init there makes it one.",
            )
            .dim(),
        ]);
    }
    let worktrees = app
        .model
        .workspaces_of(project.id)
        .filter(|workspace| !workspace.kind.is_checkout())
        .count();
    Text::from(vec![
        Line::from(vec![
            Span::from("Repository  ").dim(),
            Span::raw(project.root.display().to_string()),
        ]),
        Line::from(vec![
            Span::from("Worktrees   ").dim(),
            Span::raw(worktrees.to_string()),
        ]),
        Line::from(""),
        Line::from(format!(
            "Press {} or ⏎ to create a worktree for a new piece of work,",
            app.key(Does::NewWorktree)
        ))
        .dim(),
        Line::from(format!(
            "{} to start a session in the project's own checkout.",
            app.key(Does::NewSession)
        ))
        .dim(),
    ])
}
