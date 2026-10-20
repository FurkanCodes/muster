//! Plugins' panels' keys (`PRD-15`): each panel of a plugin that runs is
//! an action named for it, `plugin.<plugin>.<panel>`, on the lists of
//! sessions, where its key opens the panel beside the session selected
//! and puts it away again. It has the key its manifest suggests unless the
//! person sets another (`[keys]`, kept aside until its plugin is known,
//! [`Keymap::aside`]). A key Muster has a use for there already, one that
//! is the same on every screen, or one another panel has, is dropped and
//! warned of, never refused: the person can set another. The keys the
//! person set are bound first, then the suggestions in the plugins'
//! order, by id, so a plugin added later never takes a key the person
//! gave another's panel.

use super::{Action, Chord, Keymap, KeysError, KeysWarning, SESSION_LISTS, Screen, kept};

/// What a panel's name in `[keys]` starts with.
const PREFIX: &str = "plugin.";

/// A plugin's panel, as its key opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelAction {
    /// Its plugin's id.
    pub plugin: String,
    /// Its id in the plugin.
    pub panel: String,
    /// Its plugin's name, under which Settings › Keys lists it.
    pub plugin_name: String,
    /// Its title, which says what its key opens.
    pub title: String,
    /// The key its manifest suggests, as written.
    pub suggested: Option<String>,
}

impl PanelAction {
    /// Its name in `[keys]`: `plugin.zoetrope.graph`.
    pub fn name(&self) -> String {
        panel_action(&self.plugin, &self.panel)
    }

    /// What its key does, for the help and the Settings screen.
    pub fn what(&self) -> String {
        format!("{} beside the session (again: put it away)", self.title)
    }
}

/// The name in `[keys]` of `plugin`'s panel `panel`.
pub fn panel_action(plugin: &str, panel: &str) -> String {
    format!("{PREFIX}{plugin}.{panel}")
}

/// Whether `name` names a plugin's panel's action, known or not: what a
/// client may set a key for besides Muster's own actions.
pub fn is_panel_action(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|rest| rest.split_once('.'))
        .is_some_and(|(plugin, panel)| {
            crate::plugins::is_id(plugin) && crate::plugins::is_id(panel)
        })
}

/// A panel's action with its keys as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Bound {
    pub(super) action: PanelAction,
    pub(super) chords: Vec<Chord>,
    /// Whether the person set them.
    pub(super) set: bool,
}

impl Keymap {
    /// The keys with `panels`' over them, listed in the order given: first
    /// the keys the person set for any of them, then the ones the others
    /// suggest, in that order, so a suggestion never takes a key the
    /// person gave a panel; each kept where it is free on the lists of
    /// sessions and dropped with a warning where it is not.
    #[must_use]
    pub fn with_panels(mut self, panels: impl IntoIterator<Item = PanelAction>) -> Self {
        self.panels = panels
            .into_iter()
            .map(|action| {
                let set = self.aside.contains_key(&action.name());
                Bound {
                    action,
                    chords: Vec::new(),
                    set,
                }
            })
            .collect();
        self.dropped.clear();
        for set in [true, false] {
            for at in 0..self.panels.len() {
                if self.panels[at].set == set {
                    self.bind_panel(at);
                }
            }
        }
        self
    }

    /// Binds the keys of the panel at `at` in the list: the person's, or
    /// the one it suggests.
    fn bind_panel(&mut self, at: usize) {
        let Bound { action, set, .. } = &self.panels[at];
        let (action, set) = (action.clone(), *set);
        let name = action.name();
        let written = match self.aside.get(&name) {
            Some(keys) if set => keys.clone(),
            _ => action.suggested.iter().cloned().collect(),
        };
        for key in written {
            let taken = Chord::parse(&key)
                .map_err(|error| error.to_string())
                .and_then(|chord| self.free_for_panel(&name, chord).map(|()| chord));
            match taken {
                Ok(chord) => self.panels[at].chords.push(chord),
                Err(why) => self.dropped.push(if set {
                    KeysWarning::PanelKey {
                        name: name.clone(),
                        key,
                        why,
                    }
                } else {
                    KeysWarning::Suggested {
                        plugin: action.plugin_name.clone(),
                        panel: action.title.clone(),
                        key,
                        why,
                    }
                }),
            }
        }
    }

    /// Why `chord` cannot be the panel `name`'s, if it cannot: it is the
    /// same everywhere, or another action's or panel's on the lists of
    /// sessions.
    fn free_for_panel(&self, name: &str, chord: Chord) -> Result<(), String> {
        if let Some(used) = kept(chord) {
            return Err(format!("is {used}"));
        }
        if let Some(action) = SESSION_LISTS
            .iter()
            .find_map(|screen| self.action(*screen, chord))
        {
            return Err(format!("is Muster's for {}", action.name()));
        }
        match self.panel_with(chord) {
            Some(other) if other.action.name() != name => {
                Err(format!("is {}'s", other.action.name()))
            }
            _ => Ok(()),
        }
    }

    /// Whether `chord` may be the panel `name`'s key, as the person sets it
    /// on the Settings screen.
    pub fn check_panel(&self, name: &str, chord: Chord) -> Result<(), KeysError> {
        let key = chord.label();
        let action = name.to_owned();
        if let Some(used) = kept(chord) {
            return Err(KeysError::Kept { action, key, used });
        }
        let other = SESSION_LISTS
            .iter()
            .find_map(|screen| self.action(*screen, chord))
            .map(|action| action.name().to_owned())
            .or_else(|| {
                self.panel_with(chord)
                    .map(|other| other.action.name())
                    .filter(|other| other != name)
            });
        match other {
            Some(other) => Err(KeysError::Taken { action, key, other }),
            None => Ok(()),
        }
    }

    /// The panel whose key `chord` is.
    fn panel_with(&self, chord: Chord) -> Option<&Bound> {
        self.panels
            .iter()
            .find(|bound| bound.chords.contains(&chord))
    }

    /// The panel `chord` opens on `screen`, if any.
    pub fn panel(&self, screen: Screen, chord: Chord) -> Option<&PanelAction> {
        SESSION_LISTS
            .contains(&screen)
            .then(|| self.panel_with(chord))
            .flatten()
            .map(|bound| &bound.action)
    }

    /// The panels whose keys are known, in order, each with its keys.
    pub fn panels(&self) -> impl Iterator<Item = (&PanelAction, &[Chord])> {
        self.panels
            .iter()
            .map(|bound| (&bound.action, bound.chords.as_slice()))
    }

    /// The panel named `name`, if its plugin runs.
    pub fn panel_named(&self, name: &str) -> Option<&PanelAction> {
        self.bound(name).map(|bound| &bound.action)
    }

    fn bound(&self, name: &str) -> Option<&Bound> {
        self.panels.iter().find(|bound| bound.action.name() == name)
    }

    /// The panel `name`'s first key as people read it; `none` where it has
    /// none.
    pub fn panel_label(&self, name: &str) -> String {
        self.bound(name)
            .and_then(|bound| bound.chords.first())
            .map_or_else(|| "none".to_owned(), |chord| chord.label())
    }

    /// Each of the panel `name`'s keys as people read them, spaced.
    pub fn panel_labels(&self, name: &str) -> String {
        let labels: Vec<String> = self
            .bound(name)
            .map(|bound| bound.chords.iter().map(|chord| chord.label()).collect())
            .unwrap_or_default();
        if labels.is_empty() {
            "none".to_owned()
        } else {
            labels.join(" ")
        }
    }

    /// Whether the person set the panel `name`'s keys.
    pub fn panel_is_set(&self, name: &str) -> bool {
        self.bound(name).is_some_and(|bound| bound.set)
    }

    /// Whether `action`, one of Muster's, has `chord` taken by a panel on
    /// a screen they share.
    pub(super) fn taken_by_panel(&self, action: Action, chord: Chord) -> Option<String> {
        let shared = action
            .screens()
            .iter()
            .any(|screen| SESSION_LISTS.contains(screen));
        shared
            .then(|| self.panel_with(chord))
            .flatten()
            .map(|bound| bound.action.name())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn zoetrope(panel: &str, suggested: Option<&str>) -> PanelAction {
        PanelAction {
            plugin: "zoetrope".to_owned(),
            panel: panel.to_owned(),
            plugin_name: "Zoetrope".to_owned(),
            title: panel.to_owned(),
            suggested: suggested.map(str::to_owned),
        }
    }

    fn set(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(name, key)| ((*name).to_owned(), vec![(*key).to_owned()]))
            .collect()
    }

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap()
    }

    #[test]
    fn a_panel_s_name_says_its_plugin_and_panel() {
        assert_eq!(panel_action("zoetrope", "graph"), "plugin.zoetrope.graph");
        assert!(is_panel_action("plugin.zoetrope.graph"));
        assert!(is_panel_action("plugin.my-panel.files-2"));
        for not in [
            "rename",
            "plugin.zoetrope",
            "plugin..graph",
            "plugin.Zoe.graph",
            "plugin.a.b.c",
            "zoetrope_graph",
        ] {
            assert!(!is_panel_action(not), "{not}");
        }
    }

    #[test]
    fn a_panel_has_the_key_it_suggests_on_the_lists_of_sessions() {
        let keymap = Keymap::default().with_panels([zoetrope("graph", Some("Z"))]);
        let opened = keymap.panel(Screen::Sessions, chord("Z")).unwrap();
        assert_eq!(opened.name(), "plugin.zoetrope.graph");
        assert_eq!(
            keymap
                .panel(Screen::NeedsYou, chord("Z"))
                .map(PanelAction::name),
            Some(opened.name())
        );
        // Nowhere else: in a session it goes to its program.
        assert_eq!(keymap.panel(Screen::InSession, chord("Z")), None);
        assert_eq!(keymap.panel(Screen::Settings, chord("Z")), None);
        assert_eq!(keymap.panel_label("plugin.zoetrope.graph"), "Shift-Z");
        assert!(!keymap.panel_is_set("plugin.zoetrope.graph"));
        assert!(keymap.warnings().is_empty());
    }

    #[test]
    fn a_suggestion_muster_has_a_use_for_is_dropped_and_warned_of() {
        // `T` opens a shell; `j` moves down; ⏎ is everywhere's.
        let keymap = Keymap::default().with_panels([
            zoetrope("graph", Some("T")),
            zoetrope("files", Some("enter")),
            zoetrope("log", Some("G")),
            zoetrope("again", Some("G")),
        ]);
        assert_eq!(keymap.panel_label("plugin.zoetrope.graph"), "none");
        assert_eq!(keymap.panel_label("plugin.zoetrope.files"), "none");
        assert_eq!(keymap.panel_label("plugin.zoetrope.log"), "Shift-G");
        assert_eq!(keymap.panel_label("plugin.zoetrope.again"), "none");
        let warnings: Vec<String> = keymap.warnings().iter().map(ToString::to_string).collect();
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(
            warnings[0].starts_with(
                "the Zoetrope plugin's graph panel suggests T, which is Muster's for shell"
            ),
            "{warnings:?}"
        );
        assert!(
            warnings[2].contains("plugin.zoetrope.log's"),
            "{warnings:?}"
        );
        // Muster's own keys are as they were.
        assert_eq!(
            keymap.action(Screen::Sessions, chord("T")),
            Some(Action::Shell)
        );
    }

    #[test]
    fn the_person_s_key_for_a_panel_takes_the_place_of_its_suggestion() {
        let keys = set(&[("plugin.zoetrope.graph", "G"), ("plugin.gone.panel", "Y")]);
        let keymap = Keymap::with(&keys)
            .unwrap()
            .with_panels([zoetrope("graph", Some("Z"))]);
        assert_eq!(keymap.panel_labels("plugin.zoetrope.graph"), "Shift-G");
        assert!(keymap.panel_is_set("plugin.zoetrope.graph"));
        assert_eq!(keymap.panel(Screen::Sessions, chord("Z")), None);
        // A key for a panel no plugin has now is still only warned of.
        let warnings = keymap.warnings();
        assert_eq!(
            warnings,
            [KeysWarning::NotAnAction("plugin.gone.panel".to_owned())]
        );
        // Theirs that Muster uses is dropped too, said so.
        let keymap = Keymap::with(&set(&[("plugin.zoetrope.graph", "d")]))
            .unwrap()
            .with_panels([zoetrope("graph", Some("Z"))]);
        assert_eq!(keymap.panel_label("plugin.zoetrope.graph"), "none");
        assert!(
            keymap.warnings()[0]
                .to_string()
                .starts_with("keys.plugin.zoetrope.graph: d is Muster's for show_changes"),
            "{:?}",
            keymap.warnings()
        );
    }

    #[test]
    fn a_suggestion_never_takes_a_key_the_person_gave_another_panel() {
        // `aaa` sorts first and suggests G, which the person gave zoetrope.
        let mut first = zoetrope("files", Some("G"));
        first.plugin = "aaa".to_owned();
        let keymap = Keymap::with(&set(&[("plugin.zoetrope.graph", "G")]))
            .unwrap()
            .with_panels([first, zoetrope("graph", Some("Z"))]);
        assert_eq!(keymap.panel_label("plugin.zoetrope.graph"), "Shift-G");
        assert_eq!(keymap.panel_label("plugin.aaa.files"), "none");
        // Listed as given all the same.
        let names: Vec<String> = keymap.panels().map(|(action, _)| action.name()).collect();
        assert_eq!(names, ["plugin.aaa.files", "plugin.zoetrope.graph"]);
        assert!(
            keymap.warnings()[0]
                .to_string()
                .contains("suggests G, which is plugin.zoetrope.graph's"),
            "{:?}",
            keymap.warnings()
        );
    }

    #[test]
    fn setting_a_key_checks_it_against_muster_s_and_the_panels() {
        let keymap = Keymap::default()
            .with_panels([zoetrope("graph", Some("Z")), zoetrope("files", Some("G"))]);
        let graph = "plugin.zoetrope.graph";
        assert_eq!(keymap.check_panel(graph, chord("Y")), Ok(()));
        assert_eq!(keymap.check_panel(graph, chord("Z")), Ok(()), "its own");
        assert!(matches!(
            keymap.check_panel(graph, chord("G")),
            Err(KeysError::Taken { other, .. }) if other == "plugin.zoetrope.files"
        ));
        assert!(matches!(
            keymap.check_panel(graph, chord("s")),
            Err(KeysError::Taken { other, .. }) if other == "stop_session"
        ));
        assert!(matches!(
            keymap.check_panel(graph, chord("up")),
            Err(KeysError::Kept { .. })
        ));
        // And one of Muster's set to a panel's key is refused.
        assert!(matches!(
            keymap.check(Action::Shell, chord("Z")),
            Err(KeysError::Taken { other, .. }) if other == graph
        ));
        // On a screen with no panels, it is free.
        assert_eq!(keymap.check(Action::ReadSettings, chord("Z")), Ok(()));
    }
}
