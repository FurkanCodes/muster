//! Which notifications, and how each sounds, on the Settings screen: a
//! row for each kind under *Notifications*, and what the selected one is.

use muster_core::telling::{Kind, Sound};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};

use crate::app::App;

/// What `kind`'s row says of it: on, with its sound, or off.
pub(super) fn value(app: &App, kind: Kind) -> Span<'static> {
    let tell = app.model.preferences().telling.of(kind);
    if app.telling_changing {
        return Span::from("…").dim();
    }
    if !tell.on {
        return Span::from("off").dim();
    }
    let value = Span::from(format!("on · {}", sound(&tell.sound)));
    if app.model.preferences().notifications {
        value.green().bold()
    } else {
        value.dim()
    }
}

/// What `kind`'s row is called: under *Notifications*, so set in.
pub(super) fn label(kind: Kind) -> String {
    format!("  {}", kind.label())
}

fn sound(sound: &Sound) -> String {
    match sound {
        Sound::Default => "Mac's sound".to_owned(),
        Sound::Silent => "silent".to_owned(),
        Sound::Named(name) => name.clone(),
    }
}

/// What the selected kind's row does, and where it is kept.
pub(super) fn text(app: &App, kind: Kind) -> Text<'static> {
    let what = match kind {
        Kind::Question => {
            "A session asks something of you: a question, a permission, an MCP server's form. \
             Told at once."
        }
        Kind::Finish => {
            "An agent's turn finished, or its program ended well. Told once it has stayed \
             finished two seconds, as an agent often starts another turn at once."
        }
        Kind::Error => "An agent's program ended with an error.",
    };
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{}  ", kind.label())).bold(),
            value(app, kind),
        ]),
        Line::from(what),
        Line::from(""),
        Line::from(
            "⏎ tells it or not. ← → choose its sound: the Mac's own, none, or one of the Mac's \
             sounds, heard as you choose. A sound of your own goes in ~/Library/Sounds, its \
             file's name written in the file below.",
        ),
        Line::from(""),
        Line::from(
            "A notification says where as its title (the project and the branch), the chat \
             under it, then what happened, kept with the others of its project. Where your \
             terminal tells you instead, its bell rings, whatever the sound.",
        )
        .dim(),
    ];
    if !app.model.preferences().notifications {
        lines.push(Line::from(""));
        lines.push(Line::from("Notifications are off: Notifications, above, turns them on.").dim());
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::from("Saved in ").dim(),
        Span::raw(format!(
            "{}  [notifications] {}, {}_sound",
            app.model.preferences().file.display(),
            kind.key(),
            kind.key()
        )),
    ]));
    Text::from(lines)
}
