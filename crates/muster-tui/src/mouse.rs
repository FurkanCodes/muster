//! Turning the person's mouse into the reports a session's program asked
//! for, in xterm's encodings: the original `ESC [ M` with three bytes, its
//! UTF-8 variant for wide screens, and SGR (`ESC [ < b ; x ; y M`).

use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use muster_core::protocol::{MouseEncoding, MouseTracking, TerminalModes};

/// The bytes reporting a mouse event at `column`, `row` (counted from 0 on
/// the program's screen), or `None` if the program did not ask for this
/// kind of event or the encoding cannot carry it.
pub(crate) fn encode_mouse(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    column: u16,
    row: u16,
    modes: TerminalModes,
) -> Option<Vec<u8>> {
    let wanted = match kind {
        MouseEventKind::Down(_)
        | MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => modes.mouse != MouseTracking::Off,
        MouseEventKind::Up(_) => matches!(
            modes.mouse,
            MouseTracking::PressRelease | MouseTracking::ButtonMotion | MouseTracking::AnyMotion
        ),
        MouseEventKind::Drag(_) => matches!(
            modes.mouse,
            MouseTracking::ButtonMotion | MouseTracking::AnyMotion
        ),
        MouseEventKind::Moved => modes.mouse == MouseTracking::AnyMotion,
    };
    if !wanted {
        return None;
    }
    let release = matches!(kind, MouseEventKind::Up(_));
    let mut code = match kind {
        MouseEventKind::Down(button) | MouseEventKind::Up(button) => button_code(button),
        MouseEventKind::Drag(button) => button_code(button) + 32,
        // Movement with no button held.
        MouseEventKind::Moved => 3 + 32,
        MouseEventKind::ScrollUp => 64,
        MouseEventKind::ScrollDown => 65,
        MouseEventKind::ScrollLeft => 66,
        MouseEventKind::ScrollRight => 67,
    };
    // X10 mode reports buttons alone.
    if modes.mouse != MouseTracking::Press {
        for (modifier, bit) in [
            (KeyModifiers::SHIFT, 4),
            (KeyModifiers::ALT, 8),
            (KeyModifiers::CONTROL, 16),
        ] {
            if modifiers.contains(modifier) {
                code += bit;
            }
        }
    }
    let (x, y) = (u32::from(column) + 1, u32::from(row) + 1);
    match modes.mouse_encoding {
        MouseEncoding::Sgr => {
            let end = if release { 'm' } else { 'M' };
            Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes())
        }
        MouseEncoding::Default | MouseEncoding::Utf8 => {
            // The old encodings cannot say which button was let go.
            let code = if release { 3 + (code & !3) } else { code };
            let mut bytes = b"\x1b[M".to_vec();
            for value in [code + 32, x + 32, y + 32] {
                push_coded(&mut bytes, value, modes.mouse_encoding)?;
            }
            Some(bytes)
        }
    }
}

fn button_code(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

/// One value as a byte, or in UTF-8 for the UTF-8 encoding; `None` past
/// what the encoding can carry.
fn push_coded(bytes: &mut Vec<u8>, value: u32, encoding: MouseEncoding) -> Option<()> {
    match encoding {
        MouseEncoding::Utf8 if value < 2048 => {
            let character = char::from_u32(value)?;
            let mut buffer = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
        MouseEncoding::Default if value < 256 => bytes.push(u8::try_from(value).ok()?),
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(mouse: MouseTracking, mouse_encoding: MouseEncoding) -> TerminalModes {
        TerminalModes {
            mouse,
            mouse_encoding,
            ..TerminalModes::default()
        }
    }

    fn encode(kind: MouseEventKind, modes: TerminalModes) -> Option<Vec<u8>> {
        encode_mouse(kind, KeyModifiers::NONE, 9, 4, modes)
    }

    #[test]
    fn nothing_is_sent_to_a_program_that_did_not_ask() {
        let off = TerminalModes::default();
        assert_eq!(encode(MouseEventKind::ScrollUp, off), None);
        assert_eq!(encode(MouseEventKind::Down(MouseButton::Left), off), None);
    }

    #[test]
    fn sgr_reports_presses_releases_and_the_wheel() {
        let sgr = modes(MouseTracking::PressRelease, MouseEncoding::Sgr);
        let left = MouseButton::Left;
        assert_eq!(
            encode(MouseEventKind::Down(left), sgr).unwrap(),
            b"\x1b[<0;10;5M"
        );
        assert_eq!(
            encode(MouseEventKind::Up(left), sgr).unwrap(),
            b"\x1b[<0;10;5m"
        );
        assert_eq!(
            encode(MouseEventKind::ScrollDown, sgr).unwrap(),
            b"\x1b[<65;10;5M"
        );
        let shifted = encode_mouse(
            MouseEventKind::Down(MouseButton::Right),
            KeyModifiers::SHIFT | KeyModifiers::CONTROL,
            0,
            0,
            sgr,
        );
        assert_eq!(shifted.unwrap(), b"\x1b[<22;1;1M");
        // Movement only in the modes that want it.
        assert_eq!(encode(MouseEventKind::Drag(left), sgr), None);
        let motion = modes(MouseTracking::ButtonMotion, MouseEncoding::Sgr);
        assert_eq!(
            encode(MouseEventKind::Drag(left), motion).unwrap(),
            b"\x1b[<32;10;5M"
        );
        assert_eq!(encode(MouseEventKind::Moved, motion), None);
        let any = modes(MouseTracking::AnyMotion, MouseEncoding::Sgr);
        assert_eq!(
            encode(MouseEventKind::Moved, any).unwrap(),
            b"\x1b[<35;10;5M"
        );
    }

    #[test]
    fn the_original_encoding_offsets_by_32_and_reports_release_as_3() {
        let normal = modes(MouseTracking::PressRelease, MouseEncoding::Default);
        assert_eq!(
            encode(MouseEventKind::Down(MouseButton::Middle), normal).unwrap(),
            [0x1b, b'[', b'M', 32 + 1, 32 + 10, 32 + 5]
        );
        assert_eq!(
            encode(MouseEventKind::Up(MouseButton::Middle), normal).unwrap(),
            [0x1b, b'[', b'M', 32 + 3, 32 + 10, 32 + 5]
        );
        // Too far right for one byte: not sent rather than sent wrong.
        let far = encode_mouse(
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::NONE,
            300,
            0,
            normal,
        );
        assert_eq!(far, None);
        let utf8 = modes(MouseTracking::PressRelease, MouseEncoding::Utf8);
        let wide = encode_mouse(
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::NONE,
            300,
            0,
            utf8,
        )
        .unwrap();
        assert_eq!(&wide[..4], [0x1b, b'[', b'M', 32]);
        assert_eq!(std::str::from_utf8(&wide[4..6]).unwrap(), "ō");
    }

    #[test]
    fn x10_mode_reports_presses_without_modifiers() {
        let x10 = modes(MouseTracking::Press, MouseEncoding::Default);
        assert_eq!(encode(MouseEventKind::Up(MouseButton::Left), x10), None);
        let pressed = encode_mouse(
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::SHIFT,
            0,
            0,
            x10,
        );
        assert_eq!(pressed.unwrap(), [0x1b, b'[', b'M', 32, 33, 33]);
    }
}
