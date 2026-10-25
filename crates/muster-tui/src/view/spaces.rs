//! What the sessions list says of spaces (`PRODUCT.md` §6, `ARC-33`): the
//! heading over the terminals in no project, the spaces a project is in
//! where everything is shown, and the space a session in *Needs you* shows
//! in, as alerts reach the person whichever space is shown.

use muster_core::spaces::Member;
use muster_core::{ProjectId, Session};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::model::Model;

/// The heading over the terminals in no project.
pub(super) const TERMINALS: &str = "Terminals";

/// The Terminals heading's line, after its edge and fold mark.
pub(super) fn terminals_line(app: &App, edge: Span<'static>, fold: Span<'static>) -> Line<'static> {
    Line::from(vec![
        edge,
        fold,
        Span::raw(" "),
        Span::styled(
            TERMINALS,
            super::theme::of(app).quiet.add_modifier(Modifier::BOLD),
        ),
    ])
}

/// `  in Work, Home` after a project's name, while everything is shown.
pub(super) fn project_note(app: &App, project: ProjectId) -> Option<Span<'static>> {
    if app.space_shown().is_some() {
        return None;
    }
    let spaces: Vec<_> = app.model.spaces_of(Member::Project(project)).collect();
    (!spaces.is_empty()).then(|| {
        Span::styled(
            format!("  {}", Model::in_spaces(&spaces)),
            super::theme::of(app).quiet,
        )
    })
}

/// `in Work` beside a session's project in *Needs you*, kept whole as its
/// project's name is; nothing where it shows in no space.
pub(super) fn queue_note(app: &App, session: &Session) -> Option<Span<'static>> {
    let spaces = Model::in_spaces(&app.model.spaces_of_session(session));
    (!spaces.is_empty()).then(|| Span::styled(format!("{spaces}  "), super::theme::of(app).quiet))
}
