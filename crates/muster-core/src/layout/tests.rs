//! The layout read, written, completed, moved, sized and swapped.

use super::*;

fn read(text: &str) -> Layout {
    Layout::parse(text).unwrap()
}

/// A session's panel as the layout's text names it.
fn placed(session: SessionId) -> String {
    Panel::Placed(session).to_string()
}

#[test]
fn the_default_reads_and_writes_as_it_is_written() {
    assert_eq!(Layout::default().to_string(), DEFAULT);
}

#[test]
fn a_layout_writes_as_it_reads() {
    let one = placed(SessionId::new());
    for text in [
        "list | session | changes | plugin".to_owned(),
        "list 30 | session 60% | (changes / plugin)".to_owned(),
        "session | list | (changes / plugin)".to_owned(),
        format!("(list / {one}) 40% | session | (changes / plugin)"),
        format!("list | (session / (changes | {one} | plugin)) 70%"),
    ] {
        assert_eq!(read(&text).to_string(), text);
    }
    // Spaces and case are the person's; the layout writes its own way.
    assert_eq!(
        read("LIST|session|( changes/Plugin )").to_string(),
        "list | session | (changes / plugin)"
    );
}

#[test]
fn a_run_of_one_sign_nests_to_the_right() {
    let layout = read("list | session | changes | plugin");
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
        error("list | list | session | (changes / plugin)"),
        LayoutError::Twice("list".to_owned())
    );
    assert_eq!(
        error("list | terminal | changes | plugin"),
        LayoutError::NotAPanel("terminal".to_owned())
    );
    for size in ["0%", "100%", "0", "6x"] {
        assert_eq!(
            error(&format!("list {size} | session | changes | plugin")),
            LayoutError::NotASize(size.to_owned())
        );
    }
    assert_eq!(
        error("list | session / changes | plugin"),
        LayoutError::Mixed
    );
    assert_eq!(
        error("(list | session | changes | plugin"),
        LayoutError::Brackets
    );
    assert_eq!(
        error("list | session | changes | plugin)"),
        LayoutError::Brackets
    );
    assert_eq!(
        error("list | | session | changes | plugin"),
        LayoutError::Incomplete
    );
    assert_eq!(
        error("list session | changes | plugin"),
        LayoutError::Incomplete
    );
    assert!(
        error("list | terminal | changes | plugin")
            .to_string()
            .contains("the panels are list, session, changes and plugin")
    );
}

#[test]
fn a_layout_written_before_a_panel_was_muster_s_gets_it_where_the_default_puts_it() {
    for (written, read_as) in [
        // Moved and sized by the person: the plugin's panel under the
        // changes, wherever they are.
        (
            "session | list 30 | changes",
            "session | list 30 | (changes / plugin)",
        ),
        (
            "list 40 | session 60% | changes",
            "list 40 | session 60% | (changes / plugin)",
        ),
        (
            "(list / session) | changes",
            "(list / session) | (changes / plugin)",
        ),
        (
            "(session 60% | changes) | list",
            "(session 60% | (changes / plugin)) | list",
        ),
        // Several missing: each beside the nearest the layout has, in the
        // order the default names them.
        ("list | session", "list | session | (changes / plugin)"),
        ("session", "list | session | (changes / plugin)"),
        ("plugin | session", "(changes / plugin) | list | session"),
    ] {
        assert_eq!(read(written).to_string(), read_as, "{written}");
    }
    // Written again as read, it reads the same.
    let completed = read("session | list 30 | changes");
    assert_eq!(read(&completed.to_string()), completed);
}

#[test]
fn a_layout_naming_the_retired_shell_panel_reads_without_it() {
    let one = placed(SessionId::new());
    for (written, read_as) in [
        // Muster's own default before shells were placed: today's.
        (
            "list | session 60% | (changes / shell / plugin)",
            DEFAULT.to_owned(),
        ),
        ("list | session 60% | (changes / shell)", DEFAULT.to_owned()),
        // Its place goes to the other of its two, with their size.
        (
            "list 30 | (session / shell) 70% | (changes / plugin)",
            "list 30 | session 70% | (changes / plugin)".to_owned(),
        ),
        (
            "shell | list | session | (changes / plugin)",
            "list | session | (changes / plugin)".to_owned(),
        ),
        (
            "(shell 40%) | list | session | (changes / plugin)",
            "list | session | (changes / plugin)".to_owned(),
        ),
        // The terminals the person placed stay where they were.
        (
            &format!("{one} | list | session 60% | (changes / Shell / plugin)"),
            format!("{one} | list | session 60% | (changes / plugin)"),
        ),
    ] {
        assert_eq!(read(written).to_string(), read_as, "{written}");
    }
    // Named alone, it is Muster's own layout.
    assert_eq!(read("shell"), Layout::default());
    assert_eq!(read("(shell)"), Layout::default());
}

#[test]
fn a_client_older_than_a_panel_is_given_the_layout_without_it() {
    let layout = Layout::default();
    let older = layout.without(Panel::Plugin);
    assert_eq!(older.to_string(), "list | session 60% | changes");
    // What it reads, a client that knows the panel reads as it was.
    assert_eq!(read(&older.to_string()), layout);
    let moved = read("plugin | list 30 | session | changes");
    assert_eq!(
        moved.without(Panel::Plugin).to_string(),
        "list 30 | session | changes"
    );
}

#[test]
fn a_panel_moved_to_an_edge_of_the_window_runs_along_it() {
    let layout = Layout::default();
    assert_eq!(
        layout
            .moved(Panel::List, Place::Window(Edge::Right))
            .to_string(),
        "(session 60% | (changes / plugin)) | list"
    );
    assert_eq!(
        layout
            .moved(Panel::Plugin, Place::Window(Edge::Bottom))
            .to_string(),
        "(list | session 60% | changes) / plugin"
    );
}

#[test]
fn a_panel_moved_beside_another_shares_its_place() {
    let layout = Layout::default();
    assert_eq!(
        layout
            .moved(Panel::Plugin, Place::Beside(Panel::Session, Edge::Bottom))
            .to_string(),
        "list | (session / plugin) 60% | changes"
    );
    assert_eq!(
        layout
            .moved(Panel::Changes, Place::Beside(Panel::List, Edge::Top))
            .to_string(),
        "(changes / list) | session 60% | plugin"
    );
}

#[test]
fn a_panel_keeps_its_width_in_cells_but_not_a_share() {
    let layout = read("list 30 | session 60% | (changes / plugin)");
    assert_eq!(
        layout
            .moved(Panel::List, Place::Window(Edge::Right))
            .to_string(),
        "(session 60% | (changes / plugin)) | list 30"
    );
    assert_eq!(
        layout
            .moved(Panel::Session, Place::Beside(Panel::Plugin, Edge::Top))
            .to_string(),
        "list 30 | (changes / session / plugin)"
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
    let layout = Layout::default();
    let sized = layout.sized(&[], Branch::First, Size::Cells(28));
    assert_eq!(
        sized.to_string(),
        "list 28 | session 60% | (changes / plugin)"
    );
    let sized = layout.sized(&[Branch::Second], Branch::Second, Size::Share(30));
    assert_eq!(sized.to_string(), "list | session | (changes / plugin) 30%");
    // A path that leads nowhere changes nothing.
    assert_eq!(
        layout.sized(&[Branch::First], Branch::First, Size::Share(5)),
        layout
    );
}

#[test]
fn panels_not_shown_leave_their_place_to_the_other_of_their_two() {
    let layout = read("list 30 | session 60% | (changes / plugin)");
    let shown = layout
        .shown(|panel| matches!(panel, Panel::List | Panel::Session))
        .unwrap();
    // The session takes the place it shared with the changes and the
    // plugin's panel, and the size the two had there.
    assert_eq!(Layout { root: shown }.to_string(), "list 30 | session");
    assert!(layout.shown(|_| false).is_none());
    let alone = layout.shown(|panel| panel == Panel::Session).unwrap();
    assert_eq!(alone.part, Part::Panel(Panel::Session));
}

#[test]
fn the_split_between_panels_shown_is_found_past_those_not_shown() {
    let one = SessionId::new();
    let layout = read(&format!(
        "list | session 60% | (changes / {} / plugin)",
        placed(one)
    ));
    let one = Panel::Placed(one);
    assert_eq!(
        layout.split_between(&[Panel::List], &[Panel::Session]),
        Some(vec![])
    );
    // The changes not shown, the session and the placed one share a line.
    assert_eq!(
        layout.split_between(&[Panel::Session], &[one]),
        Some(vec![Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Changes], &[one]),
        Some(vec![Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[Panel::Changes], &[Panel::Plugin]),
        Some(vec![Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[one], &[Panel::Plugin]),
        Some(vec![Branch::Second, Branch::Second, Branch::Second])
    );
    assert_eq!(
        layout.split_between(&[one], &[Panel::Changes]),
        None,
        "the other way round"
    );
}

#[test]
fn a_layout_travels_as_its_text() {
    let layout = read("session | list 30 | (changes / plugin)");
    let json = serde_json::to_string(&layout).unwrap();
    assert_eq!(json, "\"session | list 30 | (changes / plugin)\"");
    // From a daemon before the plugin's panel, it comes with it.
    let older: Layout = serde_json::from_str("\"session | list 30 | changes\"").unwrap();
    assert_eq!(older, layout);
    assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), layout);
    assert!(serde_json::from_str::<Layout>("\"list | terminal\"").is_err());
}

#[test]
fn a_session_placed_reads_writes_moves_and_goes_as_any_panel() {
    let one = SessionId::new();
    let two = SessionId::new();
    let placed = Layout::default().placing(one, Place::Beside(Panel::Session, Edge::Left));
    assert_eq!(
        placed.to_string(),
        format!("list | (session:{one} | session) 60% | (changes / plugin)")
    );
    // It reads back as it was written, any number of them, each once.
    assert_eq!(read(&placed.to_string()), placed);
    let both = placed.placing(two, Place::Window(Edge::Bottom));
    assert_eq!(both.placed(), [one, two]);
    assert_eq!(read(&both.to_string()), both);
    assert_eq!(
        Layout::parse(&format!("session:{one} | {placed}")).unwrap_err(),
        LayoutError::Twice(format!("session:{one}"))
    );
    // Placed again, it moves.
    let moved = both.placing(one, Place::Window(Edge::Right));
    assert_eq!(moved.placed(), [two, one]);
    // Taken off, its place goes to the other of its two.
    assert_eq!(moved.unplaced(one).unplaced(two), Layout::default());
    // A session's id that does not read is no panel.
    assert!(matches!(
        Layout::parse("list | session:ses_nope | session | changes | plugin"),
        Err(LayoutError::NotAPanel(_))
    ));
}

#[test]
fn two_panels_swap_places_each_place_keeping_its_share() {
    let one = SessionId::new();
    let layout = read(&format!(
        "list | session 60% | (session:{one} / plugin) | changes"
    ));
    let swapped = layout.swapped(Panel::Session, Panel::Placed(one));
    assert_eq!(
        swapped.to_string(),
        format!("list | session:{one} 60% | (session / plugin) | changes")
    );
    // Swapped back, it is as it was.
    assert_eq!(swapped.swapped(Panel::Placed(one), Panel::Session), layout);
    // Panels in different parts of the tree swap as well.
    assert_eq!(
        layout.swapped(Panel::List, Panel::Changes).to_string(),
        format!("changes | session 60% | (session:{one} / plugin) | list")
    );
}

#[test]
fn a_width_in_cells_goes_with_its_panel_when_swapped() {
    let layout = read("list 30 | session 60% | (changes / plugin)");
    // The list keeps the width it was dragged to; the session, coming into
    // the list's place, takes what is left there.
    assert_eq!(
        layout.swapped(Panel::List, Panel::Session).to_string(),
        "session | list 30 | (changes / plugin)"
    );
    assert_eq!(
        layout.swapped(Panel::Session, Panel::List).to_string(),
        "session | list 30 | (changes / plugin)"
    );
}

#[test]
fn a_panel_swapped_with_itself_or_one_not_there_stays() {
    let layout = Layout::default();
    assert_eq!(layout.swapped(Panel::Plugin, Panel::Plugin), layout);
    let absent = Panel::Placed(SessionId::new());
    assert_eq!(layout.swapped(Panel::Plugin, absent), layout);
    assert_eq!(layout.swapped(absent, Panel::Plugin), layout);
}

#[test]
fn a_panel_dropped_on_another_s_middle_swaps_with_it() {
    let one = SessionId::new();
    let layout = Layout::default().placing(one, Place::Beside(Panel::Session, Edge::Right));
    let over = layout.moved(Panel::Placed(one), Place::Over(Panel::List));
    assert_eq!(over, layout.swapped(Panel::Placed(one), Panel::List));
    assert_eq!(layout.moved(Panel::List, Place::Over(Panel::List)), layout);
    // A session not placed yet has no place to give: it goes beside.
    let two = SessionId::new();
    assert_eq!(
        layout.placing(two, Place::Over(Panel::Changes)),
        layout.placing(two, Place::Beside(Panel::Changes, Edge::Right))
    );
    // Placed already, it swaps.
    assert_eq!(
        layout.placing(one, Place::Over(Panel::Changes)),
        layout.swapped(Panel::Placed(one), Panel::Changes)
    );
}
