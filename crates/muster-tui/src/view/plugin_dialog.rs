//! The dialog adding or updating a plugin (`app::plugins`): where from,
//! then a wait while it is fetched, then everything it would run, for the
//! person to say yes or no; or why it could not be fetched.

use muster_core::plugins::PluginSource;
use muster_core::protocol::plugins::{PluginPreview, PreviewKind};
use muster_core::text::one_line;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::dialogs::input_window;
use super::plugins::{runs_lines, source_text};
use super::settings::detail;
use super::theme::Theme;
use crate::app::plugins::{Typed, is_folder};
use crate::app::{App, PluginDialog};
use crate::cells;
use crate::input::LineInput;

/// One part of the dialog, top to bottom.
enum Part<'a> {
    Text(Vec<Line<'static>>),
    /// A field, and whether it has the cursor.
    Input(&'a LineInput, bool),
}

pub(super) fn draw_plugin_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    dialog: &PluginDialog,
    area: Rect,
) {
    let theme = super::theme::of(app);
    let width = area.width.saturating_sub(4).min(84);
    let inner = width.saturating_sub(2).max(1);
    let (title, parts) = match dialog {
        PluginDialog::Add { typed, refused } => (
            " Add a plugin ".to_owned(),
            add(typed, refused.as_deref(), theme),
        ),
        PluginDialog::Fetching { what, typed, .. } => {
            let title = if typed.is_some() {
                " Add a plugin "
            } else {
                " Update a plugin "
            };
            (title.to_owned(), fetching(app, what, usize::from(inner)))
        }
        PluginDialog::Failed { title, message } => (
            format!(" {title} "),
            vec![Part::Text(vec![
                Line::from("It could not be fetched:"),
                Line::from(one_line(message)).fg(theme.waiting),
                Line::from(""),
                Line::from("Nothing was changed.   esc close").dim(),
            ])],
        ),
        PluginDialog::Preview { preview, offset } => {
            let available = area.height.saturating_sub(2 + 2 + 2);
            (
                title(preview),
                shown_preview(preview, *offset, inner, available, theme),
            )
        }
    };
    let height = 2 + parts.iter().map(|part| part_rows(part, inner)).sum::<u16>();
    let rect = super::floating::centred(area, width, height);
    let mut at = super::floating::draw(frame, rect, Some(title), theme);
    // The keys, last, keep to the bottom, whatever is cut above them.
    let mut parts = parts;
    let keys = parts.pop();
    let keys_rows = keys
        .as_ref()
        .map_or(0, |part| part_rows(part, inner))
        .min(at.height);
    let mut bottom = Rect {
        y: at.y + at.height - keys_rows,
        height: keys_rows,
        ..at
    };
    at.height -= keys_rows;
    for part in parts {
        draw_part(frame, part, &mut at, inner);
    }
    if let Some(keys) = keys {
        draw_part(frame, keys, &mut bottom, inner);
    }
}

/// Draws `part` at the top of `at`, which it takes its rows of.
fn draw_part(frame: &mut Frame<'_>, part: Part<'_>, at: &mut Rect, inner: u16) {
    let height = part_rows(&part, inner).min(at.height);
    let here = Rect { height, ..*at };
    match part {
        Part::Text(lines) => {
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), here);
        }
        Part::Input(input, cursor) => {
            let (shown, column) = input_window(input.text(), input.before_cursor(), inner);
            frame.render_widget(Paragraph::new(Line::from(shown).bold()), here);
            if cursor && height > 0 {
                frame.set_cursor_position(Position {
                    x: here.x + column,
                    y: here.y,
                });
            }
        }
    }
    at.y += height;
    at.height -= height;
}

fn part_rows(part: &Part<'_>, inner: u16) -> u16 {
    match part {
        Part::Text(lines) => lines.iter().map(|line| rows(line, inner)).sum(),
        Part::Input(..) => 1,
    }
}

/// The rows `line` takes `width` columns wide, wrapped as a paragraph
/// wraps it: at spaces, a word longer than a row broken across rows. A
/// long address goes to a row of its own, which counting its characters
/// would miss.
fn rows(line: &Line<'_>, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    let (mut rows, mut at) = (1_usize, 0_usize);
    for word in text.split(' ') {
        let len = cells::width(word);
        let gap = usize::from(at > 0);
        if at + gap + len <= width {
            at += gap + len;
            continue;
        }
        if at > 0 {
            rows += 1;
        }
        let more = len.saturating_sub(1) / width;
        rows += more;
        at = len - more * width;
    }
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// Where to add a plugin from: an address or a folder, and a branch or
/// tag for git.
fn add<'a>(typed: &'a Typed, refused: Option<&str>, theme: Theme) -> Vec<Part<'a>> {
    let folder = is_folder(typed.address.text().trim());
    let mut parts = vec![
        Part::Text(vec![Line::from(
            "A git repository's address, to install from (https://, ssh://, user@host:path or \
             file://), or a folder's path (/…, ~/…), to link where it is, for a plugin you write:",
        )]),
        Part::Input(&typed.address, !typed.on_ref),
        Part::Text(vec![
            Line::from(""),
            if folder {
                Line::from("Branch or tag: none, a folder is read as it is.").dim()
            } else {
                Line::from("Branch or tag (empty for its default branch):")
            },
        ]),
        Part::Input(&typed.git_ref, typed.on_ref),
    ];
    let mut below = vec![Line::from("")];
    if let Some(refused) = refused {
        below.extend([
            Line::from(one_line(refused)).fg(theme.waiting),
            Line::from(""),
        ]);
    }
    below.push(
        Line::from("⏎ fetch it and show what it runs   ⇥ the other field   esc cancel").dim(),
    );
    parts.push(Part::Text(below));
    parts
}

fn fetching(app: &App, what: &str, width: usize) -> Vec<Part<'static>> {
    vec![Part::Text(vec![
        super::loading::rule(app, "Fetching the plugin", width),
        Line::from(one_line(what)).dim(),
        Line::from(""),
        Line::from(
            "Nothing of it runs: Muster reads what it would run, and shows you before anything \
             is added. git may take up to three minutes.",
        )
        .dim(),
        Line::from(""),
        Line::from("esc give up").dim(),
    ])]
}

/// The preview's title: what saying yes does.
fn title(preview: &PluginPreview) -> String {
    let id = one_line(&preview.id);
    match preview.kind {
        PreviewKind::Install => format!(" Install {id}? "),
        PreviewKind::Link if preview.current.is_some() => format!(" Approve {id} again? "),
        PreviewKind::Link => format!(" Link {id}? "),
        PreviewKind::Update => format!(" Update {id}? "),
    }
}

/// The preview from its line `offset`, in what fits of `available` rows
/// `inner` columns wide, and the keys under it.
fn shown_preview(
    preview: &PluginPreview,
    offset: usize,
    inner: u16,
    available: u16,
    theme: Theme,
) -> Vec<Part<'static>> {
    let lines = preview_lines(preview, theme);
    let total = lines.len();
    let mut shown = Vec::new();
    let mut used = 0;
    for line in lines.into_iter().skip(offset) {
        let height = rows(&line, inner);
        if used + height > available {
            break;
        }
        used += height;
        shown.push(line);
    }
    let more = offset > 0 || offset + shown.len() < total;
    let yes = match preview.kind {
        PreviewKind::Install => "y install",
        PreviewKind::Link if preview.current.is_some() => "y approve",
        PreviewKind::Link => "y link",
        PreviewKind::Update => "y update",
    };
    let scroll = if more { "   ↑↓ PgUp PgDn more" } else { "" };
    vec![
        Part::Text(shown),
        Part::Text(vec![
            Line::from(""),
            Line::from(format!("{yes}   n no{scroll}")).dim(),
        ]),
    ]
}

/// Everything a preview shows, a line each before wrapping: what the
/// plugin is, where it is from and would go, everything it would run,
/// what it needs and what it would be given.
pub(crate) fn preview_lines(preview: &PluginPreview, theme: Theme) -> Vec<Line<'static>> {
    let details = &preview.details;
    let mut lines = vec![
        Line::from(format!(
            "{} {} ({})",
            one_line(&details.name),
            one_line(&details.version),
            one_line(&preview.id)
        ))
        .bold(),
    ];
    if let Some(description) = &details.description {
        lines.push(Line::from(one_line(description)));
    }
    lines.push(Line::from(""));
    lines.push(detail("From", source_text(&preview.source)));
    let commit = match &preview.source {
        PluginSource::Git { commit, .. } => commit.get(..7).unwrap_or(commit),
        _ => "",
    };
    lines.push(match preview.kind {
        PreviewKind::Link => detail(
            "Stays",
            "where it is: Muster reads it there, never writes to it".to_owned(),
        ),
        PreviewKind::Install | PreviewKind::Update => detail(
            "Goes into",
            format!(
                "Muster's data folder, plugins/{}/{}/, yours alone",
                one_line(&preview.id),
                one_line(commit)
            ),
        ),
    });
    if let Some(current) = &preview.current {
        let was = current
            .details
            .as_ref()
            .map_or_else(String::new, |details| {
                format!("{} ", one_line(&details.version))
            });
        let manifest = match &current.details {
            Some(now) if now.sha256 == details.sha256 => "the same manifest",
            _ => "a manifest that is not this one",
        };
        // From the same address, its commit says enough.
        let from = match (&current.source, &preview.source) {
            (
                PluginSource::Git {
                    url: was, commit, ..
                },
                PluginSource::Git { url, .. },
            ) if was == url => {
                format!("at {}", one_line(commit.get(..7).unwrap_or(commit)))
            }
            (source, _) => format!("from {}", source_text(source)),
        };
        lines.push(detail("Now", format!("{was}{from}, with {manifest}")));
    }
    lines.push(detail(
        "Works with",
        format!("Muster {} or later", one_line(&details.muster)),
    ));
    lines.push(detail("SHA-256", details.sha256.clone()));
    lines.push(Line::from(""));
    lines.extend(runs_lines(details, theme));
    lines.push(Line::from(""));
    let (done, after) = match preview.kind {
        PreviewKind::Update => ("updated", "Updating keeps it on or off, as it is."),
        PreviewKind::Link if preview.current.is_some() => {
            ("approved", "Approving keeps it on or off, as it is.")
        }
        PreviewKind::Install | PreviewKind::Link => ("added", "Adding it turns it on."),
    };
    if details.requires.iter().any(|required| !required.found) {
        lines.push(
            Line::from(format!(
                "It needs programs that are not on PATH: it can be {done}, but they are yours to \
                 get."
            ))
            .fg(theme.working),
        );
    }
    lines.push(Line::from(after));
    lines
}
