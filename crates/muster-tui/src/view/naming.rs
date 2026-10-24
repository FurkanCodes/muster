//! How chats are named by an agent, on the Settings screen: its three rows
//! under *Muster*, and what they do.

use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};

use super::settings::detail;
use crate::app::App;

/// What the setting of naming chats by an agent is called.
pub(super) const NAMES_BY_AGENT: &str = "Chats named by an agent";
/// What the setting of the agent that names chats is called.
pub(super) const NAMED_BY: &str = "Named by";
/// What the setting of the model it names them with is called.
pub(super) const NAMING_MODEL: &str = "Its model";

/// Whether an agent names chats.
pub(super) fn by_agent_value(app: &App) -> Span<'static> {
    let theme = super::theme::of(app);
    if app.model.preferences().naming.by_agent {
        Span::from("on").fg(theme.done).bold()
    } else {
        Span::from("off").dim()
    }
}

/// The model chats are named with: the one picked, or the agent's own.
pub(super) fn model_value(app: &App) -> Span<'static> {
    let naming = &app.model.preferences().naming;
    match (&naming.agent, &naming.model) {
        (_, Some(model)) => Span::from(model.clone()),
        (Some(_), None) => Span::from("its own choice").dim(),
        (None, None) => Span::from("each agent's own").dim(),
    }
}

/// What naming chats by an agent does and costs, said beside it.
pub(super) fn text(app: &App) -> Text<'static> {
    let naming = &app.model.preferences().naming;
    let asked = match &naming.agent {
        None => "each chat's own agent".to_owned(),
        Some(_) => app.namer_label(),
    };
    let can: Vec<String> = app
        .namers()
        .into_iter()
        .filter_map(|(id, label)| id.map(|_| label))
        .collect();
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{NAMES_BY_AGENT}  ")).bold(),
            by_agent_value(app),
        ]),
        Line::from(
            "A new chat is named at once by the first line of its first prompt. A few seconds \
             later Muster asks an agent for a name of two to four words.",
        ),
        Line::from(""),
        Line::from(format!(
            "It asks {asked} with your own sign-in, so it uses a little of your plan. It sends \
             only the chat's first prompt (and, where that is a few words, the start of its \
             first answer), credentials hidden, with no tools, none of your project's files or \
             instructions, and nothing kept in the agent's history."
        )),
        Line::from("A name you give a chat (R) is never replaced.").dim(),
        Line::from(""),
        detail("Named by", app.namer_label()),
        detail("Its model", model_value(app).content.into_owned()),
        detail("Can name", can.join(", ")),
        detail(
            "Saved in",
            app.model.preferences().file.display().to_string(),
        ),
        Line::from(""),
        Line::from(
            "An agent you added names chats once its settings say how to ask it one question \
             (ask = [\"-p\", \"{prompt}\"]).",
        )
        .dim(),
    ])
}
