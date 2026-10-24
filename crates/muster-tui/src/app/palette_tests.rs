//! The look as Muster's own settings say: whose colours it draws in
//! (`Preferences::palette`), the list unframed beside a framed pane, and a
//! session's line on two lines or one (`Preferences::session_lines`).

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use muster_core::protocol::{ClientFrame, Command, Reply, ServerFrame};
use muster_core::settings::{Palette, PreferenceChange, SessionLines};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use super::look_tests::buffer;
use super::pointer_tests::mouse;
use super::tests::{World, command, one_line, preferences, press, render, world};
use super::{Action, Mode, SettingRow, View};
use crate::model::Row;
use crate::model::tests::session;
use crate::view::logos::Graphics;
use crate::view::theme;

/// Where `text` first starts in `cells`, scanning the columns from `from`.
fn find(cells: &Buffer, text: &str, from: u16) -> (u16, u16) {
    for y in 0..cells.area.height {
        let line: String = (from..cells.area.width)
            .map(|x| cells[(x, y)].symbol())
            .collect();
        if let Some(at) = line.find(text) {
            let x = from + u16::try_from(line[..at].chars().count()).unwrap();
            return (x, y);
        }
    }
    panic!("no {text:?} on screen");
}

#[test]
fn muster_s_own_colours_frame_list_and_pane_and_light_what_has_the_keys() {
    let mut world = world();
    let theme = theme::of(&world.app);
    assert_eq!(theme.palette, Palette::Muster);
    world.app.selected = Some(Row::Session(world.session.id));
    let cells = buffer(&world.app);
    let screen = render(&world.app);
    // The list is framed square, its label in its border, frame and label
    // lit while it has the keys, and its selected line lit too.
    assert!(screen.lines().nth(1).unwrap().starts_with("┌ projects ─"));
    assert_eq!(cells[(2, 1)].fg, theme.accent);
    assert_eq!(cells[(0, 3)].symbol(), "│");
    assert_eq!(cells[(0, 3)].fg, theme.accent);
    // Its text in Muster's own colour, not the terminal's.
    let (x, y) = find(&cells, "Shell", 0);
    assert_eq!(cells[(x, y)].fg, theme.text);
    // The selected session's line in the list.
    let selected = find(&cells, "• Shell", 0);
    assert_eq!(cells[selected].bg, theme.lit);
    // The pane is framed, square, and quiet while the list has the keys.
    let (corner, top) = find(&cells, "┌", 1);
    assert_eq!((corner, top), (30, 1));
    assert_eq!(cells[(corner, top)].fg, theme.line);
    // The screen shown is lit in the accent.
    let (tab, _) = find(&cells, "Sessions", 0);
    assert_eq!(
        (cells[(tab, 0)].fg, cells[(tab, 0)].bg),
        (theme.on_accent, theme.accent)
    );
    assert_eq!(cells[(tab, 0)].bg, theme.accent);

    // Typing in the session: its frame and its name lit, the list's frame
    // and label quiet, its selected line only tinted.
    press(&mut world.app, KeyCode::Enter);
    assert_eq!(world.app.mode, Mode::Terminal(world.session.id));
    let cells = buffer(&world.app);
    assert_eq!(cells[(corner, top)].fg, theme.accent);
    let (name, row) = find(&cells, "Shell", corner + 1);
    assert_eq!(row, top);
    assert_eq!(cells[(name, row)].fg, theme.accent);
    assert!(cells[(name, row)].modifier.contains(Modifier::BOLD));
    assert_ne!(cells[(2, 1)].fg, theme.accent);
    assert_eq!(cells[(0, 3)].fg, theme.line);
    assert_eq!(cells[selected].bg, theme.tint);
}

#[test]
fn the_terminal_s_own_colours_draw_as_they_always_did() {
    let mut world = world();
    preferences(&mut world.app, |preferences| {
        preferences.palette = Palette::Terminal;
    });
    let cells = buffer(&world.app);
    let (tab, _) = find(&cells, "Sessions", 0);
    assert!(cells[(tab, 0)].modifier.contains(Modifier::REVERSED));
    let (x, y) = find(&cells, "▾ app", 0);
    // The project's edge and name in the terminal's own blue, the selected
    // line tinted in its dark grey.
    assert_eq!(cells[(x - 1, y)].fg, Color::Blue);
    assert_eq!(cells[(x + 2, y)].bg, Color::DarkGray);
    let (x, y) = find(&cells, "Shell", 0);
    assert_eq!(cells[(x, y)].fg, Color::Reset);
}

#[test]
fn a_terminal_s_own_background_shades_the_selected_line() {
    let mut world = world();
    world.app.colours.background = Some(muster_core::colours::Rgb {
        red: 0,
        green: 0,
        blue: 0,
    });
    // Lit while the list has the keys: 24% of the way to the accent.
    let cells = buffer(&world.app);
    let (x, y) = find(&cells, "▾ app", 0);
    assert_eq!(cells[(x + 2, y)].bg, Color::Rgb(0x2c, 0x28, 0x3c));
    // Tinted while a session has them: 9% of the way to the text.
    world.app.mode = Mode::Terminal(world.session.id);
    let cells = buffer(&world.app);
    assert_eq!(cells[(x + 2, y)].bg, Color::Rgb(0x13, 0x14, 0x15));
}

/// The look's rows on the Settings screen, `row` selected.
fn on_look_row(world: &mut World, row: SettingRow) {
    press(&mut world.app, KeyCode::Char('4'));
    assert_eq!(world.app.view, View::Settings);
    world.app.settings.selected = Some(row);
}

#[test]
fn the_colours_are_turned_on_the_settings_screen_one_change_at_a_time() {
    let mut world = world();
    on_look_row(&mut world, SettingRow::Colours);
    let screen = render(&world.app);
    assert!(screen.contains("Colours"), "{screen}");
    assert!(screen.contains("Muster's"), "{screen}");
    assert!(screen.contains("⏎ the terminal's"), "{screen}");

    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::ChangePreference(PreferenceChange::Palette(Palette::Terminal))
    );
    assert!(
        press(&mut world.app, KeyCode::Enter)
            .iter()
            .all(|action| !matches!(action, Action::Send(ClientFrame::Command { .. }))),
        "one change at a time"
    );
    preferences(&mut world.app, |preferences| {
        preferences.palette = Palette::Terminal;
    });
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
    let screen = render(&world.app);
    assert!(
        screen.contains("Muster draws in your terminal's colours."),
        "{screen}"
    );
    assert!(screen.contains("the terminal's"), "{screen}");
    assert_eq!(theme::of(&world.app).palette, Palette::Terminal);
    // And back.
    let (_, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::ChangePreference(PreferenceChange::Palette(Palette::Muster))
    );
}

#[test]
fn the_lines_a_session_takes_are_turned_there_too() {
    let mut world = world();
    on_look_row(&mut world, SettingRow::SessionLines);
    let screen = render(&world.app);
    assert!(screen.contains("Lines for each session"), "{screen}");
    assert!(screen.contains("⏎ one line"), "{screen}");
    let (req_id, asked) = command(&press(&mut world.app, KeyCode::Enter));
    assert_eq!(
        asked,
        Command::ChangePreference(PreferenceChange::SessionLines(SessionLines::One))
    );
    one_line(&mut world.app);
    world.app.on_frame(ServerFrame::Reply {
        req_id,
        outcome: Ok(Reply::Done),
    });
    assert!(render(&world.app).contains("Each session takes one line."));
    assert_eq!(world.app.session_height(), 1);
}

/// The world with a second session under the first.
fn two_sessions() -> (World, muster_core::SessionId) {
    let mut world = world();
    let second = session(world.worktree.id, "Claude", 4);
    let id = second.id;
    world.app.on_frame(ServerFrame::Delta(
        muster_core::protocol::Delta::SessionUpserted(second),
    ));
    (world, id)
}

#[test]
fn a_click_on_either_line_of_a_session_selects_it() {
    let (mut world, second) = two_sessions();
    // Under the label: the project, the worktree, then two lines each.
    let click = |world: &mut World, row| {
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 8, row));
        world.app.selected
    };
    assert_eq!(click(&mut world, 4), Some(Row::Session(world.session.id)));
    assert_eq!(click(&mut world, 5), Some(Row::Session(world.session.id)));
    assert_eq!(click(&mut world, 6), Some(Row::Session(second)));
    assert_eq!(click(&mut world, 7), Some(Row::Session(second)));
    assert_eq!(click(&mut world, 3), Some(Row::Worktree(world.worktree.id)));
}

#[test]
fn a_picture_sits_on_each_session_s_first_line() {
    let (mut world, second) = two_sessions();
    world.app.graphics = Graphics::Iterm;
    for id in [world.session.id, second] {
        let mut claude = world.app.model.session(id).unwrap().clone();
        claude.harness = "claude".to_owned();
        world.app.on_frame(ServerFrame::Delta(
            muster_core::protocol::Delta::SessionUpserted(claude),
        ));
    }
    let cells = buffer(&world.app);
    let rows: Vec<u16> = (0..cells.area.height)
        .filter(|y| (0..34).any(|x| cells[(x, *y)].symbol().starts_with("\x1b]1337;File=")))
        .collect();
    assert_eq!(rows, [4, 6], "on each session's first line, not its second");
}

#[test]
fn on_two_lines_a_name_has_its_first_line_to_itself() {
    let (mut world, _) = two_sessions();
    let mut long = world.session.clone();
    long.title = "Fix the login redirect loop on Safari".to_owned();
    world.app.on_frame(ServerFrame::Delta(
        muster_core::protocol::Delta::SessionUpserted(long),
    ));
    world.app.selected = Some(Row::Session(world.session.id));
    // The 34 cells inside the list, as wide as it goes for a long name,
    // less the line's edge and indent, the mark and the agent's.
    assert_eq!(crate::view::cut_name(&world.app), Some((37, 26)));
    let screen = render(&world.app);
    assert!(
        screen.contains("○ • Fix the login redirect lo…"),
        "{screen}"
    );
}

/// The picture drawn in the list on `row`: its size and its pixels, or
/// `None` where it is drawn as it is, not laid on a colour.
fn picture_on(cells: &Buffer, row: u16) -> Option<((u32, u32), Vec<u8>)> {
    use base64::Engine as _;
    let sequence = (0..34)
        .map(|x| cells[(x, row)].symbol())
        .find(|symbol| symbol.starts_with("\x1b]1337;File="))
        .expect("a picture on the line");
    let data = sequence.split_once(':').unwrap().1.trim_end_matches('\x07');
    let file = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    // Laid pictures are Muster's own, red, green and blue; the logo as it
    // is keeps its transparency.
    (file[25] == 2).then(|| crate::view::picture::read_png(&file))
}

#[test]
fn a_logo_is_laid_on_the_colour_of_its_line_so_no_square_shows_around_it() {
    let (mut world, second) = two_sessions();
    world.app.graphics = Graphics::Iterm;
    for id in [world.session.id, second] {
        let mut claude = world.app.model.session(id).unwrap().clone();
        claude.harness = "claude".to_owned();
        world.app.on_frame(ServerFrame::Delta(
            muster_core::protocol::Delta::SessionUpserted(claude),
        ));
    }
    world.app.selected = Some(Row::Session(world.session.id));
    // A terminal that says no colours: an unselected line's logo is drawn
    // as it is, its own transparency over the terminal's background.
    let cells = buffer(&world.app);
    assert_eq!(picture_on(&cells, 6), None);
    // The selected line's is laid on its colour, lit while the list has
    // the keys, corner to corner.
    let lit = theme::of(&world.app).lit;
    let Color::Rgb(red, green, blue) = lit else {
        panic!("{lit:?}")
    };
    let (size, pixels) = picture_on(&cells, 4).expect("laid on the lit line");
    assert_eq!(
        size,
        (64, 64),
        "square where the cells' pixels are not said"
    );
    assert_eq!(&pixels[..3], [red, green, blue]);
    assert_eq!(&pixels[pixels.len() - 3..], [red, green, blue]);

    // Said: every line's is laid, as wide as its two cells are, the
    // unselected on the terminal's own background.
    world.app.colours.background = Some(muster_core::colours::Rgb {
        red: 0x1e1e,
        green: 0x1e1e,
        blue: 0x2e2e,
    });
    world.app.cell_pixels = Some(muster_core::protocol::PixelSize {
        width: 8,
        height: 18,
    });
    let cells = buffer(&world.app);
    let (size, pixels) = picture_on(&cells, 6).expect("laid on the background");
    assert_eq!(size, (64, 72));
    assert_eq!(&pixels[..3], [0x1e, 0x1e, 0x2e]);
    let Color::Rgb(red, green, blue) = theme::of(&world.app).lit else {
        unreachable!()
    };
    let (_, pixels) = picture_on(&cells, 4).unwrap();
    assert_eq!(&pixels[..3], [red, green, blue]);
}
