//! The dialogs drawn over the list: questions and inputs. Where each is
//! laid out is one function (`lay_out`), which the mouse reads too
//! (`dialog_presses`, `ARC-32`): a click on a key its hint names presses
//! it, on one of its choices (an agent, a tracker, a project) chooses it,
//! on a field types there, on a folder or a repository offered selects
//! it, and outside the dialog cancels it.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use muster_core::text::one_line;
use muster_core::{ProjectId, TicketKey, WorkspaceId};

use crate::app::cloning::{CLONE_ROWS, CloneChoice};
use crate::app::tickets_from::Field;
use crate::app::{App, Dialog, FolderPicker, HarnessChoice, PICKER_ROWS, Shown};
use crate::cells;
use crate::input::LineInput;

/// What a click on part of a dialog does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogPress {
    /// Presses the key a hint names.
    Key(KeyEvent),
    /// Chooses this agent.
    Harness(HarnessChoice),
    /// Chooses where tickets come from: a ticket plugin by its id, or the
    /// git remote's.
    Tracker(Option<String>),
    /// Chooses the project an agent on a shared ticket starts in.
    Project(ProjectId),
    /// The cursor to this field of the dialog's.
    Field(usize),
    /// Selects the folder, or the repository, shown at this place among
    /// all.
    Offered(usize),
    /// Selects the action on this line of the keys' dialog.
    Row(usize),
}

/// Where in a dialog's lines a click does something: its line, the cells
/// it takes from where along it (none for the whole line), and what.
struct Mark {
    line: Where,
    from: u16,
    width: Option<u16>,
    press: DialogPress,
}

/// Which line of a dialog a mark is on.
#[derive(Clone, Copy)]
enum Where {
    /// Of the text above the input.
    Text(usize),
    /// Of what is under the input.
    Below(usize),
}

/// Where on a line a click does something: where from, how wide, what.
type Marked = Vec<(u16, u16, DialogPress)>;

/// The hints in `text` that name a key (`⏎ start`, `esc cancel`, `y yes`),
/// three spaces apart: where each starts, its width and its key.
pub(super) fn hint_marks(text: &str) -> Vec<(u16, u16, KeyEvent)> {
    let mut marks = Vec::new();
    let mut at = 0;
    for (index, part) in text.split("   ").enumerate() {
        if index > 0 {
            at += 3;
        }
        let width = cells::width(part);
        if let Some(key) = part.split(' ').next().and_then(crate::keys::labelled) {
            marks.push((
                u16::try_from(at).unwrap_or(u16::MAX),
                u16::try_from(width).unwrap_or(u16::MAX),
                key,
            ));
        }
        at += width;
    }
    marks
}

/// A line of choices after `lead` (`Run `), each a chip, the chosen one
/// stood out, and where a click chooses each.
fn chips(
    lead: &'static str,
    options: impl IntoIterator<Item = (String, bool, DialogPress)>,
) -> (Line<'static>, Vec<(u16, u16, DialogPress)>) {
    let mut spans = vec![Span::raw(lead)];
    let mut marks = Vec::new();
    let mut at = cells::width(lead);
    for (label, chosen, press) in options {
        let span = choice(&label, chosen);
        let width = span.width();
        marks.push((
            u16::try_from(at).unwrap_or(u16::MAX),
            u16::try_from(width).unwrap_or(u16::MAX),
            press,
        ));
        at += width;
        spans.push(span);
    }
    (Line::from(spans), marks)
}

/// The agents offered in a dialog as chips after `lead`.
fn harness_chips(
    lead: &'static str,
    options: Vec<HarnessChoice>,
    chosen: &HarnessChoice,
) -> (Line<'static>, Vec<(u16, u16, DialogPress)>) {
    chips(
        lead,
        options.into_iter().map(|option| {
            let label = option.label().to_owned();
            let is = option == *chosen;
            (label, is, DialogPress::Harness(option))
        }),
    )
}

/// One of a dialog's choices, the chosen one stood out.
fn choice(label: &str, chosen: bool) -> Span<'static> {
    let label = format!(" {label} ");
    if chosen {
        Span::from(label).reversed()
    } else {
        Span::from(label).dim()
    }
}

/// A field of a dialog that does not have the cursor, as typed.
fn typed_line(input: &LineInput) -> Line<'static> {
    match input.text() {
        "" => Line::from("(nothing yet)").dim(),
        text => Line::from(text.to_owned()),
    }
}

/// What a dialog shows: its title and text, the input with the cursor if
/// it has one, what goes between that input and the hint, and the hint.
struct Shape<'a> {
    title: &'static str,
    lines: Vec<Line<'static>>,
    input: Option<&'a LineInput>,
    below: Vec<Line<'static>>,
    hint: &'static str,
    /// Where a click does something, beside the hints, which are found in
    /// the hint (or, with no input, the last line).
    marks: Vec<Mark>,
}

impl<'a> Shape<'a> {
    fn new(title: &'static str, lines: Vec<Line<'static>>, input: Option<&'a LineInput>) -> Self {
        Self {
            title,
            lines,
            input,
            below: Vec::new(),
            hint: "⏎ confirm   esc cancel",
            marks: Vec::new(),
        }
    }

    /// Marks `marks` on line `line` of the text.
    fn mark(&mut self, line: Where, marks: Vec<(u16, u16, DialogPress)>) {
        self.marks
            .extend(marks.into_iter().map(|(from, width, press)| Mark {
                line,
                from,
                width: Some(width),
                press,
            }));
    }
}

/// A dialog laid out: what it shows and where.
struct Laid<'a> {
    shape: Shape<'a>,
    rect: Rect,
    inner_width: u16,
    text_area: Rect,
    input_area: Rect,
}

/// Where `dialog` is drawn in `area`, as the drawing and the mouse share
/// it.
fn lay_out<'a>(app: &App, dialog: &'a Dialog, area: Rect) -> Laid<'a> {
    let width = area.width.saturating_sub(4).min(72);
    let inner_width = width.saturating_sub(2).max(1);
    let shape = shape(app, dialog, inner_width);
    // Text wraps rather than being cut off; an input takes its line, a
    // blank one and its hint, with what goes below it and a blank between.
    let text_height = text_height(&shape.lines, inner_width);
    let below_height = match shape.below.len() {
        0 => 0,
        rows => u16::try_from(rows + 1).unwrap_or(u16::MAX),
    };
    let height = 2
        + text_height
        + if shape.input.is_some() {
            3 + below_height
        } else {
            0
        };
    let rect = super::floating::centred(area, width, height);
    let inner = Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    };
    let [text_area, input_area] =
        Layout::vertical([Constraint::Length(text_height), Constraint::Min(0)]).areas(inner);
    Laid {
        shape,
        rect,
        inner_width,
        text_area,
        input_area,
    }
}

/// How many rows `lines` take wrapped `width` columns wide.
fn text_height(lines: &[Line<'_>], width: u16) -> u16 {
    lines
        .iter()
        .map(|line| wrapped_height(line.width(), width))
        .sum()
}

/// Where `dialog` is in `area`, and where on it a click does something,
/// each with what it does (`app::dialog_mouse`).
pub(crate) fn dialog_presses(
    app: &App,
    dialog: &Dialog,
    area: Rect,
) -> (Rect, Vec<(Rect, DialogPress)>) {
    if let Dialog::Keys(rebinding) = dialog {
        return super::rebind::rebinding_presses(app, rebinding, area);
    }
    if let Dialog::Plugin(plugin) = dialog {
        return super::plugin_dialog::plugin_presses(app, plugin, area);
    }
    let laid = lay_out(app, dialog, area);
    let shape = &laid.shape;
    let text_row = |line: usize| {
        laid.text_area.y
            + text_height(
                &shape.lines[..line.min(shape.lines.len())],
                laid.inner_width,
            )
    };
    let below_row = |line: usize| laid.input_area.y + 2 + u16::try_from(line).unwrap_or(0);
    let mut presses: Vec<(Rect, DialogPress)> = shape
        .marks
        .iter()
        .map(|mark| {
            let (x, y) = match mark.line {
                Where::Text(line) => (laid.text_area.x, text_row(line)),
                Where::Below(line) => (laid.input_area.x, below_row(line)),
            };
            let width = mark.width.unwrap_or(laid.inner_width);
            (Rect::new(x + mark.from, y, width, 1), mark.press.clone())
        })
        .collect();
    // The keys its hint names: under the input, or its text's last line.
    let hint = match shape.input {
        Some(_) => {
            let below = u16::try_from(shape.below.len()).unwrap_or(0);
            let row = laid.input_area.y + 2 + if below > 0 { below + 1 } else { 0 };
            Some((shape.hint.to_owned(), row))
        }
        None => shape.lines.last().map(|line| {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            (text, text_row(shape.lines.len() - 1))
        }),
    };
    if let Some((text, row)) = hint {
        presses.extend(hint_marks(&text).into_iter().map(|(from, width, key)| {
            (
                Rect::new(laid.text_area.x + from, row, width, 1),
                DialogPress::Key(key),
            )
        }));
    }
    (laid.rect, presses)
}

pub(super) fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &Dialog, area: Rect) {
    if let Dialog::Keys(rebinding) = dialog {
        super::rebind::draw_rebinding(frame, app, rebinding, area);
        return;
    }
    if let Dialog::Plugin(plugin) = dialog {
        super::plugin_dialog::draw_plugin_dialog(frame, app, plugin, area);
        return;
    }
    let Laid {
        shape:
            Shape {
                title,
                lines,
                input,
                mut below,
                hint,
                ..
            },
        rect,
        inner_width,
        text_area,
        input_area,
    } = lay_out(app, dialog, area);
    super::floating::draw(frame, rect, Some(title), super::theme::of(app));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
    if let Some(input) = input {
        let (shown, column) = input_window(input.text(), input.before_cursor(), inner_width);
        let mut input_lines = vec![Line::from(shown).bold(), Line::from("")];
        if !below.is_empty() {
            input_lines.append(&mut below);
            input_lines.push(Line::from(""));
        }
        input_lines.push(Line::from(hint).dim());
        frame.render_widget(Paragraph::new(input_lines), input_area);
        if input_area.height > 0 {
            frame.set_cursor_position(Position {
                x: input_area.x + column,
                y: input_area.y,
            });
        }
    }
}

/// What `dialog` shows, `inner_width` columns wide.
fn shape<'a>(app: &App, dialog: &'a Dialog, inner_width: u16) -> Shape<'a> {
    match dialog {
        Dialog::AddProject {
            path,
            picker,
            clone,
        } => add_project(app, path, picker, clone.as_deref(), inner_width),
        Dialog::CloneProject(choice) => {
            let mut shape = Shape {
                below: repository_lines(app, choice, usize::from(inner_width)),
                hint: "↑↓ choose   ⏎ choose where   esc cancel",
                ..Shape::new(
                    " Clone from GitHub ",
                    vec![Line::from(
                        "Your repositories on GitHub, or one by its name (owner/name):",
                    )],
                    Some(&choice.typed),
                )
            };
            if matches!(choice.listed, Some(Ok(_))) {
                let rows = choice
                    .shown()
                    .len()
                    .saturating_sub(choice.offset)
                    .min(CLONE_ROWS);
                shape.marks.extend((0..rows).map(|row| Mark {
                    line: Where::Below(row),
                    from: 0,
                    width: None,
                    press: DialogPress::Offered(choice.offset + row),
                }));
            }
            shape
        }
        Dialog::NewWorktree { branch, .. } => Shape::new(
            " New worktree ",
            vec![Line::from(
                "Branch (checked out if it exists; empty for muster/<random>):",
            )],
            Some(branch),
        ),
        Dialog::NewSession {
            workspace,
            harness,
            prompt,
        } => new_session(app, *workspace, harness, prompt),
        Dialog::AssignAgent {
            project,
            key,
            harness,
            another,
            into,
            on,
        } => assign_agent(app, *project, key, harness, (*another, *into, *on)),
        Dialog::Confirm { question, .. } => Shape::new(
            " Confirm ",
            vec![
                Line::from(question.clone()),
                Line::from(""),
                Line::from("y yes   n no").dim(),
            ],
            None,
        ),
        Dialog::TicketsFrom {
            project,
            tracker,
            fields,
            at,
            ..
        } => tickets_from(app, *project, tracker.as_deref(), fields, *at),
        Dialog::Undo {
            plan,
            what,
            agent_remembers,
            ..
        } => Shape::new(
            " Put the files back? ",
            undo_lines(plan, what, agent_remembers.as_deref()),
            None,
        ),
        Dialog::Tidy { workspace, harness } => tidy(app, *workspace, harness),
        Dialog::NamingModel {
            agent,
            model,
            listed,
        } => naming_model(app, agent, model, listed.as_ref()),
        // Drawn by themselves instead (`draw_dialog`).
        Dialog::Keys(_) => Shape::new(" Keys ", Vec::new(), None),
        Dialog::Plugin(_) => Shape::new(" Plugin ", Vec::new(), None),
        Dialog::Rename { name, .. } => Shape {
            hint: "⏎ rename   esc cancel",
            ..Shape::new(
                " Rename ",
                vec![Line::from("What to call this chat:")],
                Some(name),
            )
        },
    }
}

/// The model an agent names chats with: typed, or one it lists.
fn naming_model<'a>(
    app: &App,
    agent: &str,
    model: &'a LineInput,
    listed: Option<&Result<Vec<String>, String>>,
) -> Shape<'a> {
    let theme = super::theme::of(app);
    let label = app
        .namers()
        .into_iter()
        .find(|(id, _)| id.as_deref() == Some(agent))
        .map_or_else(|| agent.to_owned(), |(_, label)| label);
    let offered = match listed {
        None => super::loading::rule(app, "Loading models", 40),
        Some(Ok(models)) if models.is_empty() => {
            Line::from(format!("{label} lists none: type one it knows.")).dim()
        }
        Some(Ok(models)) => {
            let (line, offered) = chips(
                "It offers:",
                models.iter().enumerate().map(|(at, name)| {
                    (name.clone(), model.text() == name, DialogPress::Offered(at))
                }),
            );
            let mut shape = naming_shape(&label, model, line);
            shape.mark(Where::Below(0), offered);
            return shape;
        }
        Some(Err(error)) => Line::from(one_line(error)).fg(theme.waiting),
    };
    naming_shape(&label, model, offered)
}

/// The dialog for the model chats are named with, `offered` under it.
fn naming_shape<'a>(label: &str, model: &'a LineInput, offered: Line<'static>) -> Shape<'a> {
    Shape {
        below: vec![offered],
        hint: "⇥ ⇧⇥ the models offered   ⏎ keep   esc cancel",
        ..Shape::new(
            " Model for naming chats ",
            vec![Line::from(format!(
                "The model {label} names chats with (empty for its own choice):"
            ))],
            Some(model),
        )
    }
}

/// Which agent tidies a workspace's commits, and what it is asked to do.
fn tidy(app: &App, workspace: WorkspaceId, harness: &HarnessChoice) -> Shape<'static> {
    let branch = one_line(app.model.branch(workspace).unwrap_or("?"));
    let (run, agents) = harness_chips(
        "Tidy with ",
        HarnessChoice::agents(app.added_agents()),
        harness,
    );
    let mut shape = Shape::new(
        " Tidy the commits ",
        vec![
            Line::from(format!(
                "An agent rewrites the commits on {branch} not pushed yet into clear ones, the \
                 files left as they are."
            )),
            Line::from(format!(
                "Muster checks the files once it stops; {} lists its turn, and {} puts it back.",
                app.key(muster_core::keys::Action::ShowChanges),
                app.key(muster_core::keys::Action::PutBack)
            ))
            .dim(),
            Line::from(""),
            run,
            Line::from(""),
            Line::from("⇥ ⇧⇥ choose   ⏎ start   esc cancel").dim(),
        ],
        None,
    );
    shape.mark(Where::Text(3), agents);
    shape
}

/// What putting the files back from an entry does, for the person to say
/// yes to: what else goes with it, the branch, and what the agent keeps.
fn undo_lines(
    plan: &muster_core::timeline::UndoPlan,
    what: &str,
    agent: Option<&str>,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "The files go back as they were before #{} ({}).",
        plan.number,
        one_line(what)
    ))];
    match plan.later {
        0 => {}
        1 => lines.push(Line::from("What 1 later entry changed goes too.")),
        later => lines.push(Line::from(format!(
            "What {later} later entries changed goes too."
        ))),
    }
    lines.push(
        Line::from("What is there now is kept as an entry of its own, so this can be undone too.")
            .dim(),
    );
    if let Some(agent) = agent {
        lines.push(
            Line::from(format!(
                "{} still remembers that turn: tell it what you put back.",
                one_line(agent)
            ))
            .dim(),
        );
    }
    let mut hint = "y put the files back   n no";
    if let Some(branch) = &plan.branch {
        let name = one_line(&branch.branch);
        let commits = match branch.commits {
            1 => "1 commit".to_owned(),
            count => format!("{count} commits"),
        };
        lines.push(Line::from(""));
        lines.push(Line::from(match (&branch.refused, branch.forward) {
            (Some(why), _) => format!("{name} stays where it is: {}.", one_line(why)),
            (None, false) => format!(
                "b also moves {name} back to {}, before its {commits} since.",
                one_line(&branch.to)
            ),
            (None, true) => format!(
                "b also moves {name} forward to {}, over {commits}.",
                one_line(&branch.to)
            ),
        }));
        if branch.refused.is_none() {
            hint = "y the files only   b and the branch   n no";
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(hint).dim());
    lines
}

fn new_session<'a>(
    app: &App,
    workspace: WorkspaceId,
    harness: &HarnessChoice,
    prompt: &'a LineInput,
) -> Shape<'a> {
    let theme = super::theme::of(app);
    // A branch, or, in a plain folder, which has none, the project.
    let place = match app.model.workspace(workspace) {
        Some(found) if !found.kind.in_git() => app
            .model
            .project(found.project)
            .map_or("?", |project| project.name.as_str()),
        _ => app.model.branch(workspace).unwrap_or("?"),
    };
    let (run, agents) = harness_chips("Run ", HarnessChoice::all(app.added_agents()), harness);
    let mut lines = vec![Line::from(format!("In {place}")).dim()];
    // Another agent where one already works changes the same files.
    let warning = (*harness != HarnessChoice::Shell)
        .then(|| app.model.second_agent_warning(workspace))
        .flatten();
    if let Some(warning) = warning {
        lines.push(Line::from(warning).fg(theme.working));
    }
    let run_line = lines.len();
    lines.push(run);
    let input = harness.takes_prompt().then(|| {
        lines.push(Line::from("First prompt (optional):"));
        prompt
    });
    let mut shape = Shape {
        hint: "⇥ ⇧⇥ choose   ⏎ start   esc cancel",
        ..Shape::new(" New session ", lines, input)
    };
    shape.mark(Where::Text(run_line), agents);
    shape
}

fn assign_agent(
    app: &App,
    project: ProjectId,
    key: &TicketKey,
    harness: &HarnessChoice,
    (another, into, on): (bool, ProjectId, Option<WorkspaceId>),
) -> Shape<'static> {
    let stacking = app.offers_stack(into, key, another);
    let title = app
        .ticket(project, key)
        .map_or(String::new(), |ticket| format!(" {}", ticket.title));
    let mut lines = vec![Line::from(format!("{}{title}", key.shown()))];
    let mut marks: Vec<(usize, Marked)> = Vec::new();
    let sharing = app.sharing(project);
    let keys = if sharing.len() > 1 {
        // Nothing in the ticket says which repository it is for.
        lines.push(Line::from("Which project is it for?").dim());
        for choice_of in sharing {
            let name = app
                .model
                .project(choice_of)
                .map_or("?", |found| found.name.as_str());
            let mut line = vec![Span::raw(" "), choice(name, choice_of == into)];
            marks.push((
                lines.len(),
                vec![(0, u16::MAX, DialogPress::Project(choice_of))],
            ));
            let agents = app.model.agents_on(&[choice_of], key).len();
            if agents > 0 {
                let noun = if agents == 1 { "agent" } else { "agents" };
                line.push(Span::from(format!("  {agents} {noun} on it")).dim());
            }
            lines.push(Line::from(line));
        }
        if stacking {
            "↑↓ project   ⇥ ⇧⇥ agent   ←→ on top of   ⏎ start   esc cancel"
        } else {
            "↑↓ project   ⇥ ⇧⇥ agent   ⏎ start   esc cancel"
        }
    } else if stacking {
        "⇥ ⇧⇥ agent   ←→ on top of   ⏎ start   esc cancel"
    } else {
        "⇥ ⇧⇥ choose   ⏎ start   esc cancel"
    };
    let (run, agents) = harness_chips("Start ", HarnessChoice::agents(app.added_agents()), harness);
    let given = app.given_words().unwrap_or_else(|| "the ticket".to_owned());
    let place = if app.starts_another(into, key, another) {
        format!("In a worktree of its own, with {given} as its first prompt.")
    } else {
        format!("In the ticket's worktree, with {given} as its first prompt.")
    };
    lines.push(Line::from(""));
    marks.push((lines.len(), agents));
    lines.push(run);
    if stacking {
        let stacked = match on {
            Some(on) => app.worktree_label(on),
            None => "its base".to_owned(),
        };
        // ◂ and ▸ go round what it is made on top of, as ← and → do.
        let lead = cells::width("On top of ");
        let after = lead + 2 + cells::width(&stacked) + 1;
        let key = |code| DialogPress::Key(KeyEvent::from(code));
        marks.push((
            lines.len(),
            vec![
                (
                    u16::try_from(lead).unwrap_or(0),
                    1,
                    key(crossterm::event::KeyCode::Left),
                ),
                (
                    u16::try_from(after).unwrap_or(0),
                    1,
                    key(crossterm::event::KeyCode::Right),
                ),
            ],
        ));
        lines.push(Line::from(vec![
            Span::raw("On top of ◂ "),
            Span::from(stacked).reversed(),
            Span::raw(" ▸"),
        ]));
    }
    lines.extend([
        Line::from(place).dim(),
        Line::from(""),
        Line::from(keys).dim(),
    ]);
    let mut shape = Shape::new(" Assign an agent ", lines, None);
    for (line, marked) in marks {
        shape.mark(Where::Text(line), marked);
    }
    shape
}

fn tickets_from<'a>(
    app: &App,
    project: ProjectId,
    tracker: Option<&str>,
    fields: &'a [Field],
    at: usize,
) -> Shape<'a> {
    let name = app
        .model
        .project(project)
        .map_or("?", |found| found.name.as_str());
    let offered = app.offered_trackers();
    let (from, trackers) = chips(
        "From ",
        std::iter::once((
            "its git remote".to_owned(),
            tracker.is_none(),
            DialogPress::Tracker(None),
        ))
        .chain(offered.iter().map(|option| {
            (
                option.name.clone(),
                tracker == Some(option.id.as_str()),
                DialogPress::Tracker(Some(option.id.clone())),
            )
        })),
    );
    let mut lines = vec![
        Line::from(format!("Where {name}'s tickets come from")).bold(),
        from,
        Line::from(""),
    ];
    let mut field_marks: Vec<(Where, usize)> = Vec::new();
    let chosen = offered
        .iter()
        .find(|option| Some(option.id.as_str()) == tracker);
    let Some(input) = fields.get(at) else {
        lines.push(
            Line::from(match chosen {
                None => "Where its git remote's host keeps them, as Muster can tell.".to_owned(),
                Some(chosen) => chosen
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Its tickets in {}.", chosen.name)),
            })
            .dim(),
        );
        // With nothing to type, its hint is a line of its own.
        lines.extend([
            Line::from(""),
            Line::from("⇥ ⇧⇥ choose   ⏎ save   esc cancel").dim(),
        ]);
        let mut shape = Shape::new(" Tickets from ", lines, None);
        shape.mark(Where::Text(1), trackers);
        return shape;
    };
    let label = |field: &Field| {
        let needed = if field.setting.required {
            ""
        } else {
            " (if you like)"
        };
        Line::from(format!("{}{needed}:", field.setting.label))
    };
    for (index, field) in fields[..at].iter().enumerate() {
        field_marks.push((Where::Text(lines.len()), index));
        lines.push(label(field));
        field_marks.push((Where::Text(lines.len()), index));
        lines.push(typed_line(&field.input));
    }
    lines.push(label(input));
    let below = fields[at + 1..]
        .iter()
        .flat_map(|field| [label(field), typed_line(&field.input)])
        .collect();
    for line in 0..2 * (fields.len() - at - 1) {
        field_marks.push((Where::Below(line), at + 1 + line / 2));
    }
    let mut shape = Shape {
        below,
        hint: if fields.len() > 1 {
            "⇥ ⇧⇥ choose   ↑↓ field   ⏎ save   esc cancel"
        } else {
            "⇥ ⇧⇥ choose   ⏎ save   esc cancel"
        },
        ..Shape::new(" Tickets from ", lines, Some(&input.input))
    };
    shape.mark(Where::Text(1), trackers);
    shape
        .marks
        .extend(field_marks.into_iter().map(|(line, field)| Mark {
            line,
            from: 0,
            width: None,
            press: DialogPress::Field(field),
        }));
    shape
}

/// The folder picker's lines: the folders shown for `typed`, the chosen one
/// marked, at most [`PICKER_ROWS`] of them, and where they are among all;
/// always as many lines, so the dialog keeps its place as the person types.
/// The folder to add as a project, or to clone `clone` into.
fn add_project<'a>(
    app: &App,
    path: &'a LineInput,
    picker: &FolderPicker,
    clone: Option<&str>,
    inner_width: u16,
) -> Shape<'a> {
    let (title, said, hint) = match clone {
        None => (
            " Add a project ",
            "Folder to add (a git repository, or any folder):".to_owned(),
            "↑↓ choose   ⇥ open   ⇧⇥ up   ⏎ add   esc cancel",
        ),
        Some(name) => (
            " Clone from GitHub ",
            format!(
                "Folder to clone {name} into (as {}/ there):",
                muster_core::protocol::repositories::folder_of(name)
            ),
            "↑↓ choose   ⇥ open   ⇧⇥ up   ⏎ clone here   esc cancel",
        ),
    };
    let mut shape = Shape {
        below: picker_lines(app, picker, path.text(), usize::from(inner_width)),
        hint,
        ..Shape::new(title, vec![Line::from(said)], Some(path))
    };
    if let Shown::Folders(folders) = picker.shown(path.text()) {
        let rows = folders.len().saturating_sub(picker.offset).min(PICKER_ROWS);
        shape.marks.extend((0..rows).map(|row| Mark {
            line: Where::Below(row),
            from: 0,
            width: None,
            press: DialogPress::Offered(picker.offset + row),
        }));
    }
    shape
}

/// The repositories offered, the chosen one lit, and why ⏎ did nothing.
fn repository_lines(app: &App, choice: &CloneChoice, columns: usize) -> Vec<Line<'static>> {
    let theme = super::theme::of(app);
    let mut lines = match &choice.listed {
        None => vec![super::loading::rule(app, "Loading repositories", columns)],
        Some(Err(reason)) => vec![
            Line::from(super::shorten(&format!("GitHub: {reason}"), columns)).fg(theme.waiting),
            Line::from("⏎ still clones one typed as owner/name.").dim(),
        ],
        Some(Ok(_)) => {
            let shown = choice.shown();
            if shown.is_empty() {
                vec![Line::from("None of yours matches; ⏎ clones one typed as owner/name.").dim()]
            } else {
                shown
                    .iter()
                    .enumerate()
                    .skip(choice.offset)
                    .take(CLONE_ROWS)
                    .map(|(at, repository)| {
                        let chosen = choice.selected == Some(at);
                        let mark = if repository.private { "  private" } else { "" };
                        let name = super::shorten(&repository.name, columns.saturating_sub(2));
                        let room = columns.saturating_sub(4 + cells::width(&name) + mark.len());
                        let about = repository
                            .description
                            .as_deref()
                            .map(|text| super::shorten(&format!("  {text}"), room))
                            .unwrap_or_default();
                        Line::from(vec![
                            Span::raw(if chosen { "› " } else { "  " }),
                            if chosen {
                                Span::from(name).reversed()
                            } else {
                                Span::raw(name)
                            },
                            Span::from(mark).dim(),
                            Span::from(about).dim(),
                        ])
                    })
                    .collect()
            }
        }
    };
    lines.resize(CLONE_ROWS + 1, Line::from(""));
    if let Some(refused) = &choice.refused {
        lines.push(Line::from(refused.clone()).fg(theme.waiting));
    }
    lines
}

fn picker_lines(
    app: &App,
    picker: &FolderPicker,
    typed: &str,
    columns: usize,
) -> Vec<Line<'static>> {
    let mut lines = folder_lines(app, picker, typed, columns);
    lines.resize(PICKER_ROWS + 1, Line::from(""));
    lines
}

fn folder_lines(
    app: &App,
    picker: &FolderPicker,
    typed: &str,
    columns: usize,
) -> Vec<Line<'static>> {
    let folders = match picker.shown(typed) {
        Shown::Loading => return vec![super::loading::rule(app, "Loading folder", columns)],
        Shown::Failed(reason) => {
            return vec![
                Line::from(super::shorten(&format!("This folder: {reason}"), columns)).dim(),
            ];
        }
        Shown::Folders(folders) => folders,
    };
    if folders.is_empty() {
        let filter = crate::app::picker_filter(typed);
        let said = if filter.is_empty() {
            "No folders in it.".to_owned()
        } else {
            format!("No folder here matches {filter:?}.")
        };
        return vec![Line::from(super::shorten(&said, columns)).dim()];
    }
    let mut lines: Vec<Line<'static>> = folders
        .iter()
        .enumerate()
        .skip(picker.offset)
        .take(PICKER_ROWS)
        .map(|(at, folder)| {
            let mark = if folder.git { "  git" } else { "" };
            let name = super::shorten(
                &format!("{}/", folder.name),
                columns.saturating_sub(4 + mark.len()),
            );
            let chosen = picker.selected == Some(at);
            let name = if chosen {
                Span::from(name).reversed()
            } else {
                Span::raw(name)
            };
            Line::from(vec![
                Span::raw(if chosen { "› " } else { "  " }),
                name,
                Span::from(mark).dim(),
            ])
        })
        .collect();
    if folders.len() > PICKER_ROWS {
        let first = picker.offset + 1;
        let last = (picker.offset + PICKER_ROWS).min(folders.len());
        lines.push(Line::from(format!("  {first}–{last} of {}", folders.len())).dim());
    }
    lines
}

/// How many lines text `width` columns wide takes when wrapped at `columns`.
fn wrapped_height(width: usize, columns: u16) -> u16 {
    let lines = width.div_ceil(usize::from(columns.max(1))).max(1);
    u16::try_from(lines).unwrap_or(u16::MAX)
}

/// The part of an input's `text` that fits in `columns`, scrolled so the
/// cursor (after `before_cursor`) stays in view, and the cursor's column
/// within it.
pub(super) fn input_window(text: &str, before_cursor: &str, columns: u16) -> (String, u16) {
    let columns = usize::from(columns.max(1));
    let cursor = cells::width(before_cursor);
    // The cursor sits after the last character shown, so keep a column for it.
    let start = (cursor + 1).saturating_sub(columns);
    let rest = cells::after(text, start);
    let skipped = cells::width(text) - cells::width(rest);
    let shown = cells::start_within(rest, columns).to_owned();
    (
        shown,
        u16::try_from(cursor.saturating_sub(skipped)).unwrap_or(0),
    )
}
