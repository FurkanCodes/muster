//! The layout read, written, completed, moved and sized.

use super::*;

fn read(text: &str) -> Layout {
    Layout::parse(text).unwrap()
}

#[test]
fn the_default_reads_and_writes_as_it_is_written() {
    assert_eq!(Layout::default().to_string(), DEFAULT);
}

#[test]
fn a_layout_writes_as_it_reads() {
    for text in [
        "list | session | changes | shell | plugin",
        "list 30 | session 60% | (changes / shell / plugin)",
        "session | list | changes | (shell / plugin)",
        "(list / shell) 40% | session | (changes / plugin)",
        "list | (session / (changes | shell | plugin)) 70%",
    ] {
        assert_eq!(read(text).to_string(), text);
    }
    // Spaces and case are the person's; the layout writes its own way.
    assert_eq!(
        read("LIST|session|( changes/shell/Plugin )").to_string(),
        "list | session | (changes / shell / plugin)"
    );
}

#[test]
fn a_run_of_one_sign_nests_to_the_right() {
    let layout = read("list | session | changes | shell | plugin");
    let Part::Split(Split::Across, first, rest) = &layout.root().part else {
        panic!("{layout:?}");
    };
    assert_eq!(first.part, Part::Panel(Panel::List));
    assert!(matches!(rest.part, Part::Split(Split::Across, ..)));
}

#[test]
fn what_does_not_read_is_named() {
    let error = |text: &str| Layout::parse(text).unwrap_err();
    assert_eq!(
        error("list | list | session | (changes / shell)"),
        LayoutError::Twice("list".to_owned())
    );
    assert_eq!(
        error("list | terminal | changes | shell"),
        LayoutError::NotAPanel("terminal".to_owned())
    );
    for size in ["0%", "100%", "0", "6x"] {
        assert_eq!(
            error(&format!("list {size} | session | changes | shell")),
            LayoutError::NotASize(size.to_owned())
        );
    }
    assert_eq!(
        error("list | session / changes | shell"),
        LayoutError::Mixed
    );
    assert_eq!(
        error("(list | session | changes | shell"),
        LayoutError::Brackets
    );
    assert_eq!(
        error("list | session | changes | shell)"),
        LayoutError::Brackets
    );
    assert_eq!(
        error("list | | session | changes | shell"),
        LayoutError::Incomplete
    );
    assert_eq!(
        error("list session | changes | shell"),
        LayoutError::Incomplete
    );
    assert!(
        error("list | terminal | changes | shell")
            .to_string()
            .contains("the panels are list, session, changes, shell and plugin")
    );
}

#[test]
fn a_layout_written_before_a_panel_was_muster_s_gets_it_where_the_default_puts_it() {
    // Muster's own default until the plugin's panel: today's.
    assert_eq!(
        read("list | session 60% | (changes / shell)"),
        Layout::default()
    );
    for (written, read_as) in [
        // Moved and sized by the person: the plugin's panel under the
        // shell, wherever that is.
        (
            "session | list 30 | (changes / shell)",
            "session | list 30 | (changes / shell / plugin)",
        ),
        (
            "list 40 | session 60% | (changes / shell)",
            "list 40 | session 60% | (changes / shell / plugin)",
        ),
        (
            "(list / session) | (changes / shell)",
            "(list / session) | (changes / shell / plugin)",
        ),
        (
            "list | (session / shell) 60% | changes",
            "list | (session / shell / plugin) 60% | changes",
        ),
        (
            "(session 60% | (changes / shell)) | list",
            "(session 60% | (changes / shell / plugin)) | list",
        ),
        (
            "shell | list | session | changes",
            "(shell / plugin) | list | session | changes",
        ),
        // Several missing: each beside the nearest the layout has, in the
        // order the default names them.
        (
            "list | session | changes",
            "list | session | (changes / shell / plugin)",
        ),
        ("session", "list | session | (changes / shell / plugin)"),
        (
            "plugin | session",
            "(changes / shell / plugin) | list | session",
        ),
    ] {
        assert_eq!(read(written).to_string(), read_as, "{written}");
    }
    // Written again as read, it reads the same.
    let completed = read("session | list 30 | (changes / shell)");
    assert_eq!(read(&completed.to_string()), completed);
}

#[test]
fn a_client_older_than_a_panel_is_given_the_layout_without_it() {
    let layout = Layout::default();
    let older = layout.without(Panel::Plugin);
    assert_eq!(older.to_string(), "list | session 60% | (changes / shell)");
    // What it reads, a client that knows the panel reads as it was.
    assert_eq!(read(&older.to_string()), layout);
    let moved = read("plugin | list 30 | session | (changes / shell)");
    assert_eq!(
        moved.without(Panel::Plugin).to_string(),
        "list 30 | session | (changes / shell)"
    );
}

#[test]
fn a_panel_moved_to_an_edge_of_the_window_runs_along_it() {
    let layout = read("list | session 60% | (changes / shell / plugin)");
    assert_eq!(
        layout
            .moved(Panel::List, Place::Window(Edge::Right))
            .to_string(),
        "(session 60% | (changes / shell / plugin)) | list"
    );
    assert_eq!(
        layout
            .moved(Panel::Shell, Place::Window(Edge::Bottom))
            .to_string(),
        "(list | session 60% | (changes / plugin)) / shell"
    );
}

#[test]
fn a_panel_moved_beside_another_shares_its_place() {
    let layout = read("list | session 60% | (changes / shell / plugin)");
    assert_eq!(
        layout
            .moved(Panel::Shell, Place::Beside(Panel::Session, Edge::Bottom))
            .to_string(),
        "list | (session / shell) 60% | (changes / plugin)"
    );
    assert_eq!(
        layout
            .moved(Panel::Changes, Place::Beside(Panel::List, Edge::Top))
            .to_string(),
        "(changes / list) | session 60% | (shell / plugin)"
    );
}

#[test]
fn a_panel_keeps_its_width_in_cells_but_not_a_share() {
    let layout = read("list 30 | session 60% | (changes / shell / plugin)");
    assert_eq!(
        layout
            .moved(Panel::List, Place::Window(Edge::Right))
            .to_string(),
        "(session 60% | (changes / shell / plugin)) | list 30"
    );
    assert_eq!(
        layout
            .moved(Panel::Session, Place::Beside(Panel::Shell, Edge::Top))
            .to_string(),
        "list 30 | (changes / (session / shell) / plugin)"
    );
}

#[test]
fn a_panel_moved_beside_itself_stays() {
    let layout = Layout::default();
    assert_eq!(
        layout.moved(Panel::List, Place::Beside(Panel::List, Edge::Right)),
        layout
    );
}

#[test]
fn a_split_is_sized_from_either_side() {
    let layout = read("list | session 60% | (changes / shell / plugin)");
    let sized = layout.sized(&[], Branch::First, Size::Cells(28));
    assert_eq!(
        sized.to_string(),
        "list 28 | session 60% | (changes / shell / plugin)"
    );
    let sized = layout.sized(&[Branch::Second], Branch::Second, Size::Share(30));
    assert_eq!(
        sized.to_string(),
        "list | session | (changes / shell / plugin) 30%"
    );
    // A path that leads nowhere changes nothing.
    assert_eq!(
        layout.sized(&[Branch::First], Branch::First, Size::Share(5)),
        layout
    );
}

#[test]
fn panels_not_shown_leave_their_place_to_the_other_of_their_two() {
    let layout = read("list 30 | session 60% | (changes / shell / plugin)");
    let shown = layout
        .shown(|panel| matches!(panel, Panel::List | Panel::Session))
        .unwrap();
    // The session takes the place it shared with the changes, the shell
    // and the plugin's panel, and the size the two had there.
    assert_eq!(Layout { root: shown }.to_string(), "list 30 | session");
    assert!(layout.shown(|_| false).is_none());
    let alone = layout.shown(|panel| panel == Panel::Session).unwrap();
    assert_eq!(alone.part, Part::Panel(Panel::Session));
}

#[test]
fn the_split_between_panels_shown_is_found_past_those_not_shown() {
    let layout = read("list | session 60% | (changes / shell / plugin)");
    assert_eq!(
        layout.split_between(&[Panel::List], &[Panel::Session]),
        Some(vec![])
    );
    // The changes not shown, the session and the shell share a line.
    assert_eq!(
        layout.split_between(&[Panel::Session], &[Panel::Shell]),
        Some(vec![Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Changes], &[Panel::Shell]),
        Some(vec![Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Changes], &[Panel::Plugin]),
        Some(vec![Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Shell], &[Panel::Plugin]),
        Some(vec![Branch::Second, Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Shell], &[Panel::Changes]),
        None,
        "the other way round"
    );
}

#[test]
fn a_layout_travels_as_its_text() {
    let layout = read("session | list 30 | (changes / shell / plugin)");
    let json = serde_json::to_string(&layout).unwrap();
    assert_eq!(json, "\"session | list 30 | (changes / shell / plugin)\"");
    // From a daemon before the plugin's panel, it comes with it.
    let older: Layout = serde_json::from_str("\"session | list 30 | (changes / shell)\"").unwrap();
    assert_eq!(older, layout);
    assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), layout);
    assert!(serde_json::from_str::<Layout>("\"list | terminal\"").is_err());
}

#[test]
fn a_session_pinned_reads_writes_moves_and_goes_as_any_panel() {
    let one = SessionId::new();
    let two = SessionId::new();
    let pinned = Layout::default().pinning(one, Place::Beside(Panel::Session, Edge::Left));
    assert_eq!(
        pinned.to_string(),
        format!("list | (session:{one} | session) 60% | (changes / shell / plugin)")
    );
    // It reads back as it was written, any number of them, each once.
    assert_eq!(read(&pinned.to_string()), pinned);
    let both = pinned.pinning(two, Place::Window(Edge::Bottom));
    assert_eq!(both.pinned(), [one, two]);
    assert_eq!(read(&both.to_string()), both);
    assert_eq!(
        Layout::parse(&format!("session:{one} | {pinned}")).unwrap_err(),
        LayoutError::Twice(format!("session:{one}"))
    );
    // Pinned again, it moves.
    let moved = both.pinning(one, Place::Window(Edge::Right));
    assert_eq!(moved.pinned(), [two, one]);
    // Unpinned, its place goes to the other of its two.
    assert_eq!(moved.unpinned(one).unpinned(two), Layout::default());
    // A session's id that does not read is no panel.
    assert!(matches!(
        Layout::parse("list | session:ses_nope | session | changes | shell | plugin"),
        Err(LayoutError::NotAPanel(_))
    ));
}

#[test]
fn two_panels_swap_places_each_place_keeping_its_share() {
    let one = SessionId::new();
    let layout = read(&format!(
        "list | session 60% | (session:{one} / shell / plugin) | changes"
    ));
    let swapped = layout.swapped(Panel::Session, Panel::Pinned(one));
    assert_eq!(
        swapped.to_string(),
        format!("list | session:{one} 60% | (session / shell / plugin) | changes")
    );
    // Swapped back, it is as it was.
    assert_eq!(swapped.swapped(Panel::Pinned(one), Panel::Session), layout);
    // Panels in different parts of the tree swap as well.
    assert_eq!(
        layout.swapped(Panel::List, Panel::Changes).to_string(),
        format!("changes | session 60% | (session:{one} / shell / plugin) | list")
    );
}

#[test]
fn a_width_in_cells_goes_with_its_panel_when_swapped() {
    let layout = read("list 30 | session 60% | (changes / shell / plugin)");
    // The list keeps the width it was dragged to; the session, coming into
    // the list's place, takes what is left there.
    assert_eq!(
        layout.swapped(Panel::List, Panel::Session).to_string(),
        "session | list 30 | (changes / shell / plugin)"
    );
    assert_eq!(
        layout.swapped(Panel::Session, Panel::List).to_string(),
        "session | list 30 | (changes / shell / plugin)"
    );
}

#[test]
fn a_panel_swapped_with_itself_or_one_not_there_stays() {
    let layout = Layout::default();
    assert_eq!(layout.swapped(Panel::Shell, Panel::Shell), layout);
    let absent = Panel::Pinned(SessionId::new());
    assert_eq!(layout.swapped(Panel::Shell, absent), layout);
    assert_eq!(layout.swapped(absent, Panel::Shell), layout);
}
