//! Chats named by an agent, on the Settings screen (`PRODUCT.md` §6):
//! whether an agent names them, which (each session's own, or one the
//! person picks among those that can be asked one question), and with
//! which model, chosen among those the agent lists or typed.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;
use muster_core::settings::PreferenceChange;

use super::harness_choice::HarnessChoice;
use super::{Action, App, Dialog, Mode, Then};
use crate::input::{Edit, LineInput};

impl App {
    /// The agents that can name chats, by id and label: each session's own
    /// first (`None`), then Muster's own that can be asked one question,
    /// then the person's that say how.
    pub(crate) fn namers(&self) -> Vec<(Option<String>, String)> {
        let mut namers = vec![(None, "each session's own agent".to_owned())];
        namers.extend(
            HarnessChoice::all(self.added_agents())
                .into_iter()
                .filter(|choice| match choice {
                    HarnessChoice::Added(agent) => agent.asks,
                    own => muster_core::ask::built_in(own.id()).is_some(),
                })
                .map(|choice| (Some(choice.id().to_owned()), choice.label().to_owned())),
        );
        namers
    }

    /// What the agent naming chats is called on screen.
    pub(crate) fn namer_label(&self) -> String {
        let naming = &self.model.preferences().naming;
        self.namers()
            .into_iter()
            .find(|(id, _)| *id == naming.agent)
            .map_or_else(
                || naming.agent.clone().unwrap_or_default(),
                |(_, label)| label,
            )
    }

    /// ⏎ on *Chats named by an agent*: on, or off.
    pub(super) fn toggle_names_by_agent(&mut self) -> Vec<Action> {
        let on = !self.model.preferences().naming.by_agent;
        let said = if on {
            "An agent names new chats."
        } else {
            "Chats keep the first line of their first prompt."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::NamesByAgent(on)),
            Then::Say(said),
        )]
    }

    /// ⏎ on *Named by*: the next agent that can name chats, with its own
    /// choice of model.
    pub(super) fn next_namer(&mut self) -> Vec<Action> {
        let namers = self.namers();
        let now = &self.model.preferences().naming.agent;
        let at = namers.iter().position(|(id, _)| id == now).unwrap_or(0);
        let (agent, _) = namers[(at + 1) % namers.len()].clone();
        vec![self.command(
            Command::ChangePreference(PreferenceChange::NamedBy { agent, model: None }),
            Then::Say("Chats are named by the agent shown."),
        )]
    }

    /// ⏎ on *Its model*: a field for the model, with those the agent lists
    /// to choose from, asked of the daemon.
    pub(super) fn choose_naming_model(&mut self) -> Vec<Action> {
        let naming = self.model.preferences().naming.clone();
        let Some(agent) = naming.agent else {
            self.complain("Pick the agent that names chats first: each uses its own model.");
            return Vec::new();
        };
        self.mode = Mode::Dialog(Dialog::NamingModel {
            agent: agent.clone(),
            model: LineInput::with(naming.model.as_deref().unwrap_or_default()),
            listed: None,
        });
        vec![self.command(
            Command::ListModels {
                agent: agent.clone(),
            },
            Then::Models(agent),
        )]
    }

    /// The models `agent` offers, for the dialog still open for it.
    pub(super) fn on_models(&mut self, agent: &str, outcome: Result<Vec<String>, String>) {
        if let Mode::Dialog(Dialog::NamingModel {
            agent: open,
            listed,
            ..
        }) = &mut self.mode
            && open == agent
        {
            *listed = Some(outcome);
        }
    }

    /// A key in the model's dialog: ⇥ and ⇧⇥ go through the models listed,
    /// ⏎ keeps the one in the field (none for the agent's own choice).
    pub(super) fn naming_model_key(
        &mut self,
        agent: String,
        mut model: LineInput,
        listed: Option<Result<Vec<String>, String>>,
        key: &KeyEvent,
    ) -> Vec<Action> {
        if let (KeyCode::Tab | KeyCode::BackTab, Some(Ok(models))) = (key.code, &listed)
            && !models.is_empty()
        {
            let at = models.iter().position(|listed| listed == model.text());
            let next = match (at, key.code == KeyCode::Tab) {
                (None, true) => 0,
                (None, false) => models.len() - 1,
                (Some(at), true) => (at + 1) % models.len(),
                (Some(at), false) => (at + models.len() - 1) % models.len(),
            };
            model = LineInput::with(&models[next]);
            self.mode = Mode::Dialog(Dialog::NamingModel {
                agent,
                model,
                listed,
            });
            return Vec::new();
        }
        match model.handle(key) {
            Edit::Submit => {
                self.mode = Mode::Browse;
                let model = Some(model.text().trim().to_owned()).filter(|text| !text.is_empty());
                vec![self.command(
                    Command::ChangePreference(PreferenceChange::NamedBy {
                        agent: Some(agent),
                        model,
                    }),
                    Then::Say("Chats are named with the model shown."),
                )]
            }
            edit => self.keep_editing(
                edit,
                Dialog::NamingModel {
                    agent,
                    model,
                    listed,
                },
            ),
        }
    }
}
