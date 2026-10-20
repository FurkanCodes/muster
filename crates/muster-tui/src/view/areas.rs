//! Window geometry cached by every input to its layout, never by output or
//! animation state. Two entries hold the browsing and typing bottom bars;
//! the cache belongs to the app and ends with it (ARC-14).

use muster_core::layout::{Layout, Panel};
use ratatui::layout::Rect;

use super::{App, BrowseAreas, list_width};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    area: Rect,
    bottom: u16,
    list: u16,
    layout: Layout,
    folded: bool,
    narrow: bool,
    fills: bool,
    changes: bool,
    shell: bool,
    plugin: bool,
    keys: Panel,
}

impl Key {
    pub(super) fn of(app: &App, area: Rect, bottom: u16, beside_session: bool) -> Self {
        Self {
            area,
            bottom,
            list: list_width(app, beside_session, area.width),
            layout: app.panels().clone(),
            folded: app.list_folded(),
            narrow: app.narrow(),
            fills: app.pane_fills_width(),
            changes: app.changes_shown(),
            shell: app.shell_beside().is_some(),
            plugin: app.plugin_beside().is_some(),
            keys: app.panel_with_keys(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Cache {
    entries: [Option<(Key, BrowseAreas)>; 2],
    next: usize,
}

impl Cache {
    pub(super) fn get(&mut self, key: Key, solve: impl FnOnce() -> BrowseAreas) -> BrowseAreas {
        if let Some((_, areas)) = self
            .entries
            .iter()
            .flatten()
            .find(|(found, _)| found == &key)
        {
            return areas.clone();
        }
        let areas = solve();
        self.entries[self.next] = Some((key, areas.clone()));
        self.next = (self.next + 1) % self.entries.len();
        areas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muster_core::protocol::TermSize;

    #[test]
    fn geometry_is_reused_and_every_layout_input_invalidates_it_arc_14() {
        let app = App::new(
            std::path::PathBuf::new(),
            TermSize {
                cols: 120,
                rows: 35,
            },
        );
        let area = Rect::new(0, 0, 120, 36);
        let key = Key::of(&app, area, 1, true);
        let expected = super::super::areas_uncached(&app, area, 1, true);
        let mut cache = Cache::default();
        assert_eq!(cache.get(key.clone(), || expected.clone()), expected);
        assert_eq!(
            cache.get(key.clone(), || panic!("must reuse geometry")),
            expected
        );
        let mut changes = Vec::new();
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut changed = key.clone();
                changed.$field = $value;
                changes.push(changed);
            }};
        }
        changed!(area, Rect::new(0, 0, 80, 24));
        changed!(bottom, 2);
        changed!(list, 36);
        changed!(
            layout,
            Layout::parse("session | list | (changes / shell)").unwrap()
        );
        changed!(folded, !key.folded);
        changed!(narrow, !key.narrow);
        changed!(fills, !key.fills);
        changed!(changes, !key.changes);
        changed!(shell, !key.shell);
        changed!(plugin, !key.plugin);
        changed!(keys, Panel::Shell);
        for changed in changes {
            let mut cache = Cache::default();
            cache.get(key.clone(), || expected.clone());
            let mut solved = false;
            cache.get(changed, || {
                solved = true;
                expected.clone()
            });
            assert!(solved);
        }
        // The two bottom bars can alternate without evicting each other.
        let mut other = key.clone();
        other.bottom = 2;
        cache.get(other.clone(), || expected.clone());
        cache.get(key, || panic!("typing geometry kept"));
        cache.get(other, || panic!("browsing geometry kept"));
    }
}
