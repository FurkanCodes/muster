//! What the sessions list says of spaces (`PRODUCT.md` §6, `ARC-33`): each
//! space's heading, under which its projects and free terminals are listed
//! while everything is shown, and the space a session in *Needs you* is in,
//! as alerts reach the person whichever space is shown.

use muster_core::{Session, SpaceId};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::App;

/// A space's name on one line.
pub(super) fn space_name(app: &App, space: SpaceId) -> String {
    app.model.space(space).map_or_else(String::new, |found| {
        muster_core::text::one_line(&found.name).trim().to_owned()
    })
}

/// A space's heading line, after its edge and fold mark.
pub(super) fn space_line(
    app: &App,
    space: SpaceId,
    edge: Span<'static>,
    fold: Span<'static>,
) -> Line<'static> {
    Line::from(vec![
        edge,
        fold,
        Span::raw(" "),
        Span::styled(
            space_name(app, space),
            super::theme::of(app).quiet.add_modifier(Modifier::BOLD),
        ),
    ])
}

/// `in Work` beside a session's project in *Needs you*, kept whole as its
/// project's name is; nothing where it is in no space.
pub(super) fn queue_note(app: &App, session: &Session) -> Option<Span<'static>> {
    let space = app.model.spaces_of_session(session).first()?.id;
    Some(Span::styled(
        format!("in {}  ", space_name(app, space)),
        super::theme::of(app).quiet,
    ))
}
