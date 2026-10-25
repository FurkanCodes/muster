//! The mouse after anything: random presses, drags, releases, turns of the
//! wheel, keys, output, program modes and window sizes, anywhere and in any
//! order; and then the clicks and the wheel a person reaches for still do
//! what they say. A failure names the seed and the steps that led to it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::protocol::{MouseTracking, ServerFrame, TermSize, TerminalModes};
use ratatui::layout::Rect;

use super::pointer_tests::{mouse, mouse_mode, open_with_output, set_modes};
use super::tests::{SIZE, World, press, shift_q};
use super::{Action, Mode, View};
use crate::model::Row;

/// Numbers that look random, the same for a seed every time.
struct Dice(u64);

impl Dice {
    fn below(&mut self, n: u16) -> u16 {
        // xorshift64
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        u16::try_from(self.0 % u64::from(n.max(1))).unwrap()
    }

    fn one_in(&mut self, n: u16) -> bool {
        self.below(n) == 0
    }
}

fn window(world: &World) -> Rect {
    let size = world.app.terminal_size;
    Rect::new(0, 0, size.cols, size.rows + 1)
}

/// One random thing a person, the program or the daemon does; said, for
/// the failure's story.
fn step(world: &mut World, dice: &mut Dice, modes_change: bool) -> String {
    let id = world.session.id;
    let area = window(world);
    let (column, row) = (dice.below(area.width), dice.below(area.height));
    let modifiers = if dice.one_in(4) {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    match dice.below(16) {
        0..=2 => {
            let button = if dice.one_in(5) {
                MouseButton::Right
            } else {
                MouseButton::Left
            };
            send(world, MouseEventKind::Down(button), column, row, modifiers)
        }
        3..=5 => send(
            world,
            MouseEventKind::Drag(MouseButton::Left),
            column,
            row,
            modifiers,
        ),
        6 | 7 => send(
            world,
            MouseEventKind::Up(MouseButton::Left),
            column,
            row,
            modifiers,
        ),
        8 => send(world, MouseEventKind::ScrollUp, column, row, modifiers),
        9 => send(world, MouseEventKind::ScrollDown, column, row, modifiers),
        10 => {
            let code = [
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Esc,
                KeyCode::Tab,
                KeyCode::Char('1'),
                KeyCode::Char('f'),
                KeyCode::Enter,
            ][usize::from(dice.below(7))];
            world.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            format!("key {code:?}")
        }
        11 => {
            world.app.on_key(shift_q());
            "Shift-Q".to_owned()
        }
        12 => {
            let seq = world.app.screens[&id].next_seq().unwrap_or(0);
            let lines = dice.below(40);
            #[expect(clippy::format_collect, reason = "test input")]
            let data: String = (0..lines).map(|n| format!("more{n}\r\n")).collect();
            world.app.on_frame(ServerFrame::Output {
                session: id,
                seq,
                data: data.into_bytes(),
            });
            format!("{lines} lines of output")
        }
        13 if modes_change => {
            let modes = match dice.below(3) {
                0 => TerminalModes::default(),
                1 => mouse_mode(),
                _ => TerminalModes {
                    mouse: MouseTracking::ButtonMotion,
                    ..mouse_mode()
                },
            };
            set_modes(world, id, modes);
            format!("modes {:?}", modes.mouse)
        }
        14 => {
            let size = TermSize {
                cols: 60 + dice.below(80),
                rows: 10 + dice.below(30),
            };
            world.app.on_resize(size);
            format!("resized to {}x{}", size.cols, size.rows)
        }
        _ => {
            world.app.on_focus(dice.one_in(2));
            "focus".to_owned()
        }
    }
}

fn send(
    world: &mut World,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> String {
    world.app.on_mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    });
    format!("{kind:?} at {column},{row} {modifiers:?}")
}

fn click(world: &mut World, column: u16, row: u16) -> Vec<Action> {
    let mut actions =
        world
            .app
            .on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    actions.extend(
        world
            .app
            .on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), column, row)),
    );
    actions
}

/// What a person reaches for, from wherever the steps left the UI.
fn still_works(world: &mut World, story: &str) {
    let id = world.session.id;
    world.app.on_resize(SIZE);
    set_modes(world, id, TerminalModes::default());
    // Back to the session list, by the mouse where it can. A click on a
    // button may have left a dialog, a menu, the help, the changes or the
    // panel key open: given up.
    for _ in 0..4 {
        let open = matches!(
            world.app.mode,
            Mode::Dialog(_) | Mode::Menu(_) | Mode::Help(_) | Mode::Changes
        ) || world.app.panels.waiting.is_some()
            || world.app.panels.resizing.is_some();
        if open {
            press(&mut world.app, KeyCode::Esc);
        }
    }
    // A panel zoomed is put back by a click on ZOOM, and the list folded
    // away brought back by its button.
    if world.app.panels.zoomed.is_some() {
        let top = crate::view::browse_areas(&world.app, window(world)).top;
        let (_, zoom) = crate::view::top_bar::top_controls(&world.app, top)
            .into_iter()
            .find(|(control, _)| *control == crate::view::top_bar::TopControl::Zoom)
            .unwrap_or_else(|| panic!("ZOOM in the top bar: {story}"));
        click(world, zoom.x + 1, zoom.y);
        assert_eq!(world.app.panels.zoomed, None, "ZOOM: {story}");
    }
    if world.app.mode == Mode::Terminal(id) && world.app.full_screen {
        click(world, 2, 0);
        assert_eq!(world.app.mode, Mode::Browse, "the top bar: {story}");
    }
    if let Mode::Terminal(_) = world.app.mode {
        click(world, 2, 2);
        assert_eq!(world.app.mode, Mode::Browse, "the list: {story}");
    }
    press(&mut world.app, KeyCode::Char('1'));
    assert_eq!(world.app.view, View::Sessions, "{story}");
    if world.app.list_folded() {
        let areas = crate::view::browse_areas(&world.app, window(world));
        let fold = crate::view::fold_button(&world.app, &areas)
            .unwrap_or_else(|| panic!("the fold button: {story}"));
        click(world, fold.x, fold.y);
        assert!(!world.app.list_folded(), "the fold button: {story}");
    }

    // A click on the session's line selects it, once what a click on a
    // triangle folded away is unfolded.
    for row in [
        Row::Project(world.project.id),
        Row::Worktree(world.worktree.id),
    ] {
        if world.app.is_folded(row) {
            world.app.fold(row);
        }
    }
    let areas = crate::view::browse_areas(&world.app, window(world));
    let rows = world.app.rows();
    let index = rows
        .iter()
        .position(|row| *row == Row::Session(id))
        .unwrap();
    let line = u16::try_from(index - world.app.list_offset).unwrap();
    // The project's line first, so a preview folded away comes back.
    if let Some(project) = rows
        .iter()
        .position(|row| *row == Row::Project(world.project.id))
        .and_then(|at| at.checked_sub(world.app.list_offset))
    {
        // Past its triangle, which would fold it.
        let project = u16::try_from(project).unwrap();
        click(world, areas.list.x + 8, areas.list.y + 1 + project);
    }
    click(world, areas.list.x + 2, areas.list.y + 1 + line);
    assert_eq!(
        world.app.selected,
        Some(Row::Session(id)),
        "the session's line: {story}"
    );

    // A click on its pane gives it the keys.
    let areas = crate::view::browse_areas(&world.app, window(world));
    click(world, areas.preview.x + 10, areas.preview.y + 5);
    assert_eq!(world.app.mode, Mode::Terminal(id), "its pane: {story}");
    assert!(!world.app.full_screen, "{story}");

    // The wheel takes it back and forward again.
    world.app.on_mouse(mouse(MouseEventKind::ScrollUp, 50, 10));
    assert!(
        world.app.screens[&id].scrolled_back() > 0,
        "the wheel: {story}"
    );
    for _ in 0..1000 {
        if world.app.screens[&id].scrolled_back() == 0 {
            break;
        }
        world
            .app
            .on_mouse(mouse(MouseEventKind::ScrollDown, 50, 10));
    }
    assert_eq!(world.app.screens[&id].scrolled_back(), 0, "{story}");

    // A drag over its first line copies what is there.
    let pane = crate::view::session_area(&world.app, window(world));
    let wanted = world.app.screens[&id]
        .screen()
        .contents_between(0, 0, 0, 6)
        .trim_end()
        .to_owned();
    let mut actions = world.app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        pane.x,
        pane.y,
    ));
    for kind in [MouseEventKind::Drag, MouseEventKind::Up] {
        actions.extend(
            world
                .app
                .on_mouse(mouse(kind(MouseButton::Left), pane.x + 5, pane.y)),
        );
    }
    assert!(
        actions.contains(&Action::Copy(wanted.clone())),
        "the drag, for {wanted:?}: {actions:?}: {story}"
    );

    // And a click on the list gives it the keys back.
    click(world, areas.list.x + 2, areas.list.y + 1);
    assert_eq!(world.app.mode, Mode::Browse, "back to the list: {story}");
    assert_eq!(world.app.selection, None, "{story}");
}

fn sweep(seed: u64, modes_change: bool) {
    let mut dice = Dice(seed);
    for round in 0..40 {
        let (mut world, _) = open_with_output(100, dice.one_in(3));
        let mut story = vec![format!("seed {seed}, round {round}")];
        for _ in 0..40 {
            story.push(step(&mut world, &mut dice, modes_change));
            // Text is marked only in the session with the keys.
            if let Some(selection) = world.app.selection {
                assert_eq!(
                    world.app.mode,
                    Mode::Terminal(selection.session),
                    "{}",
                    story.join("\n  ")
                );
            }
        }
        still_works(&mut world, &story.join("\n  "));
    }
}

#[test]
fn after_any_clicks_drags_and_keys_the_mouse_still_works() {
    sweep(0x5eed_0001, false);
}

#[test]
fn also_when_the_program_takes_the_mouse_and_lets_it_go() {
    sweep(0x5eed_0002, true);
}
