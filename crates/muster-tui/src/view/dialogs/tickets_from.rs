//! The dialog choosing where a project's tickets come from, as drawn.

use ratatui::style::Stylize;
use ratatui::text::Line;

use super::{App, DialogPress, Field, Mark, ProjectId, Shape, Where, chips, typed_line};

pub(super) fn tickets_from<'a>(
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
