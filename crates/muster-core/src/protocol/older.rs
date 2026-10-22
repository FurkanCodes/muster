//! What a client older than a change is sent instead (`ARC-09`): what its
//! protocol cannot read is left out, or told as it can read it. A client
//! older than plugins' panels ([`PANELS_PROTOCOL`]) is sent no panel's
//! session, which it would take for an agent's, and a layout and keys
//! without a panel's: its layout refuses a panel it does not know, and its
//! keys a name that is no action of theirs, each the whole setting.

use crate::keys::Action;
use crate::layout::Panel;
use crate::plugins::PANELS_PROTOCOL;
use crate::settings::Preferences;

use super::{Delta, Snapshot};

impl Snapshot {
    /// The state as a client that agreed to `version` can read it.
    #[must_use]
    pub fn for_version(mut self, version: u32) -> Self {
        if version < PANELS_PROTOCOL {
            self.sessions.retain(|session| !session.kind().is_panel());
            self.preferences = before_panels(self.preferences);
        }
        self
    }
}

impl Delta {
    /// The change as a client that agreed to `version` can read it, or
    /// nothing where it knows nothing of it: a delta newer than its
    /// protocol ([`Delta::since`]), or a panel's session to a client older
    /// than panels. What it is told of a session it was never sent, it
    /// passes over.
    #[must_use]
    pub fn for_version(self, version: u32) -> Option<Self> {
        if self.since() > version {
            return None;
        }
        if version >= PANELS_PROTOCOL {
            return Some(self);
        }
        match self {
            Self::SessionUpserted(session) if session.kind().is_panel() => None,
            Self::Preferences(preferences) => Some(Self::Preferences(before_panels(preferences))),
            delta => Some(delta),
        }
    }
}

/// `preferences` without a plugin's panel in the layout, and with only the
/// keys of Muster's own actions.
fn before_panels(mut preferences: Preferences) -> Preferences {
    preferences.panels = preferences.panels.without(Panel::Plugin);
    preferences
        .keys
        .retain(|name, _| Action::named(name).is_some());
    preferences
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Session;
    use crate::ids::{SessionId, WorkspaceId};
    use crate::layout::Layout;
    use crate::session::SessionStatus;
    use crate::time::Timestamp;

    fn session(harness: &str) -> Session {
        let at = Timestamp::from_millis(1);
        Session {
            id: SessionId::new(),
            workspace: WorkspaceId::new(),
            harness: harness.to_owned(),
            title: harness.to_owned(),
            status: SessionStatus::Running,
            unseen: false,
            resume_id: None,
            created_at: at,
            ended_at: None,
            status_since: at,
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
            started_by: None,
        }
    }

    fn preferences() -> Preferences {
        let mut preferences = Preferences {
            panels: Layout::default(),
            ..Preferences::default()
        };
        for (name, key) in [("rename", "F2"), ("plugin.zoetrope.graph", "Z")] {
            preferences
                .keys
                .insert(name.to_owned(), vec![key.to_owned()]);
        }
        preferences
    }

    #[test]
    fn a_client_older_than_panels_is_sent_no_panel_and_no_word_of_one_arc_09() {
        let chat = session("claude");
        let panel = Session {
            beside: Some(chat.id),
            ..session("plugin:zoetrope/graph")
        };
        let snapshot = Snapshot {
            sessions: vec![chat.clone(), panel.clone()],
            preferences: preferences(),
            ..Snapshot::default()
        };
        assert!(snapshot.preferences.panels.to_string().contains("plugin"));
        let older = snapshot.clone().for_version(PANELS_PROTOCOL - 1);
        assert_eq!(older.sessions, std::slice::from_ref(&chat));
        assert!(!older.preferences.panels.to_string().contains("plugin"));
        assert_eq!(
            older.preferences.keys.keys().collect::<Vec<_>>(),
            ["rename"]
        );
        // Encoded as it goes, the older client's frame names no panel's
        // harness or key.
        let bytes = rmp_serde::to_vec_named(&older).unwrap();
        for word in [&b"plugin:"[..], b"plugin."] {
            assert!(!bytes.windows(word.len()).any(|window| window == word));
        }
        // A client of panels is sent all of it.
        assert_eq!(snapshot.clone().for_version(PANELS_PROTOCOL), snapshot);

        let upserted = |session: &Session| Delta::SessionUpserted(session.clone());
        let older = PANELS_PROTOCOL - 1;
        assert_eq!(upserted(&panel).for_version(older), None);
        assert_eq!(upserted(&chat).for_version(older), Some(upserted(&chat)));
        assert_eq!(
            upserted(&panel).for_version(PANELS_PROTOCOL),
            Some(upserted(&panel))
        );
        let Some(Delta::Preferences(told)) = Delta::Preferences(preferences()).for_version(older)
        else {
            panic!("the preferences are told");
        };
        assert!(!told.panels.to_string().contains("plugin"));
        assert!(!told.keys.contains_key("plugin.zoetrope.graph"));
        // What it is told of a session it was never sent, it passes over.
        let status = Delta::SessionStatus {
            session: panel.id,
            status: SessionStatus::Finished,
            unseen: false,
            since: Timestamp::from_millis(2),
        };
        assert_eq!(status.clone().for_version(older), Some(status));
    }

    #[test]
    fn a_delta_newer_than_the_client_is_never_sent() {
        let plugins = Delta::PluginsChanged(Vec::new());
        assert_eq!(
            plugins
                .clone()
                .for_version(crate::plugins::PLUGINS_PROTOCOL - 1),
            None
        );
        assert_eq!(
            plugins
                .clone()
                .for_version(crate::plugins::PLUGINS_PROTOCOL),
            Some(plugins)
        );
    }
}
