//! Turning key presses and pastes into the bytes a session's program reads.
//!
//! Two dialects: the conventional xterm encoding for most programs, and the
//! kitty keyboard protocol's `CSI u` encoding once the program has asked for
//! it (Claude Code does; it is how Shift+Enter and Option combinations reach
//! it). The daemon reports the program's modes with every attach.
//!
//! Adapted from Nebula's `keys.rs` (MIT, Copyright (c) 2026 Cody Seibert;
//! see NOTICE). Changed: cursor keys follow application cursor mode
//! (`ESC O A` instead of `ESC [ A`), which Nebula ignores, so vim and less
//! receive the keys they asked for; and pastes are encoded here too.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use muster_core::keys::{Chord, Key};
use muster_core::protocol::TerminalModes;

/// Kitty flag: keys that would be ambiguous get `CSI u` encodings.
const DISAMBIGUATE: u8 = 0x1;
/// Kitty flag: every key is reported as an escape code, plain text too.
const REPORT_ALL: u8 = 0x8;

/// The bytes for `key`, or `None` for a key the program has no encoding for
/// (better swallowed than leaked as a bare character: Cmd+C must not type
/// `c`).
pub fn encode_key(key: &KeyEvent, modes: TerminalModes) -> Option<Vec<u8>> {
    if modes.kitty_flags & DISAMBIGUATE != 0 {
        encode_kitty(key, modes.kitty_flags)
    } else {
        encode_legacy(key, modes.application_cursor)
    }
}

/// The bytes for pasted `text`: bracketed if the program asked for it, so it
/// can tell a paste from typing, with any bracket markers inside the text
/// removed so a paste cannot end itself early.
pub fn encode_paste(text: &str, modes: TerminalModes) -> Vec<u8> {
    // Terminals send pasted newlines as carriage returns.
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if !modes.bracketed_paste {
        return text.into_bytes();
    }
    let inner = text.replace("\x1b[200~", "").replace("\x1b[201~", "");
    [b"\x1b[200~".as_slice(), inner.as_bytes(), b"\x1b[201~"].concat()
}

// The kitty dialect.

/// Kitty's modifier bits: shift 1, alt 2, ctrl 4, super 8, hyper 16, meta 32.
fn kitty_modifiers(modifiers: KeyModifiers) -> u8 {
    [
        (KeyModifiers::SHIFT, 1),
        (KeyModifiers::ALT, 2),
        (KeyModifiers::CONTROL, 4),
        (KeyModifiers::SUPER, 8),
        (KeyModifiers::HYPER, 16),
        (KeyModifiers::META, 32),
    ]
    .into_iter()
    .filter(|(modifier, _)| modifiers.contains(*modifier))
    .map(|(_, bit)| bit)
    .sum()
}

/// `CSI <code> u`, or `CSI <code> ; <modifiers + 1> u`.
fn csi_u(code: u32, bits: u8) -> Vec<u8> {
    if bits == 0 {
        format!("\x1b[{code}u").into_bytes()
    } else {
        format!("\x1b[{code};{}u", u32::from(bits) + 1).into_bytes()
    }
}

/// Arrows, Home and End keep their final byte and gain a modifier field.
fn csi_function(final_byte: char, bits: u8) -> Vec<u8> {
    if bits == 0 {
        format!("\x1b[{final_byte}").into_bytes()
    } else {
        format!("\x1b[1;{}{final_byte}", u32::from(bits) + 1).into_bytes()
    }
}

/// `CSI <n> ~`, with a modifier field when modified.
fn csi_tilde(n: u8, bits: u8) -> Vec<u8> {
    if bits == 0 {
        format!("\x1b[{n}~").into_bytes()
    } else {
        format!("\x1b[{n};{}~", u32::from(bits) + 1).into_bytes()
    }
}

fn encode_kitty(key: &KeyEvent, flags: u8) -> Option<Vec<u8>> {
    let mut bits = kitty_modifiers(key.modifiers);
    let report_all = flags & REPORT_ALL != 0;
    let plain = bits == 0 && !report_all;
    Some(match key.code {
        KeyCode::Char(c) => {
            // The key code is the lowercase character; shift rides the bits.
            let base = if c.is_uppercase() {
                bits |= 1;
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c
            };
            if bits & !1 == 0 && !report_all {
                // Unmodified or shifted text stays text.
                c.to_string().into_bytes()
            } else {
                csi_u(u32::from(base), bits)
            }
        }
        // Esc is the reason the protocol exists: always `CSI u`.
        KeyCode::Esc => csi_u(27, bits),
        KeyCode::Enter if plain => b"\r".to_vec(),
        KeyCode::Enter => csi_u(13, bits),
        KeyCode::Tab if plain => b"\t".to_vec(),
        KeyCode::Tab => csi_u(9, bits),
        KeyCode::BackTab => csi_u(9, bits | 1),
        KeyCode::Backspace if plain => vec![0x7f],
        KeyCode::Backspace => csi_u(127, bits),
        KeyCode::Up => csi_function('A', bits),
        KeyCode::Down => csi_function('B', bits),
        KeyCode::Right => csi_function('C', bits),
        KeyCode::Left => csi_function('D', bits),
        KeyCode::Home => csi_function('H', bits),
        KeyCode::End => csi_function('F', bits),
        KeyCode::Insert => csi_tilde(2, bits),
        KeyCode::Delete => csi_tilde(3, bits),
        KeyCode::PageUp => csi_tilde(5, bits),
        KeyCode::PageDown => csi_tilde(6, bits),
        KeyCode::F(n @ 1..=4) if bits == 0 => vec![0x1b, b'O', b'P' + (n - 1)],
        KeyCode::F(n @ 1..=4) => format!(
            "\x1b[1;{}{}",
            u32::from(bits) + 1,
            char::from(b'P' + (n - 1))
        )
        .into_bytes(),
        KeyCode::F(n) => csi_tilde(function_key_number(n)?, bits),
        _ => return None,
    })
}

// The legacy xterm dialect.

fn encode_legacy(key: &KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    // Cmd and friends have no legacy encoding.
    if key
        .modifiers
        .intersects(KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META)
    {
        return None;
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let mut out = Vec::with_capacity(8);
    if key.modifiers.contains(KeyModifiers::ALT) {
        out.push(0x1b);
    }
    let modified = key
        .modifiers
        .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(c) if control => out.push(control_byte(c)?),
        KeyCode::Char(c) => out.extend_from_slice(c.to_string().as_bytes()),
        KeyCode::Enter => out.push(b'\r'),
        KeyCode::Tab => out.push(b'\t'),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => out.push(0x7f),
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Up => out.extend(cursor_key(b'A', key.modifiers, application_cursor)),
        KeyCode::Down => out.extend(cursor_key(b'B', key.modifiers, application_cursor)),
        KeyCode::Right => out.extend(cursor_key(b'C', key.modifiers, application_cursor)),
        KeyCode::Left => out.extend(cursor_key(b'D', key.modifiers, application_cursor)),
        KeyCode::Home if application_cursor && !modified => out.extend_from_slice(b"\x1bOH"),
        KeyCode::End if application_cursor && !modified => out.extend_from_slice(b"\x1bOF"),
        KeyCode::Home => out.extend_from_slice(b"\x1b[H"),
        KeyCode::End => out.extend_from_slice(b"\x1b[F"),
        KeyCode::PageUp => out.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => out.extend_from_slice(b"\x1b[6~"),
        KeyCode::Delete => out.extend_from_slice(b"\x1b[3~"),
        KeyCode::Insert => out.extend_from_slice(b"\x1b[2~"),
        KeyCode::F(n) => out.extend(function_key(n)?),
        _ => return None,
    }
    Some(out)
}

/// Ctrl+A..Z become 0x01..0x1a, with a few punctuation controls.
fn control_byte(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        letter @ 'a'..='z' => letter as u8 - b'a' + 1,
        ' ' | '@' => 0,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' | '/' => 0x1f,
        _ => return None,
    })
}

/// An arrow: `ESC [ A`, `ESC O A` in application cursor mode, or
/// `ESC [ 1 ; <m> A` when modified (application mode has no modified form).
fn cursor_key(direction: u8, modifiers: KeyModifiers, application_cursor: bool) -> Vec<u8> {
    let bits = [
        (KeyModifiers::SHIFT, 1),
        (KeyModifiers::ALT, 2),
        (KeyModifiers::CONTROL, 4),
    ]
    .into_iter()
    .filter(|(modifier, _)| modifiers.contains(*modifier))
    .map(|(_, bit)| bit)
    .sum::<u8>();
    match (bits, application_cursor) {
        (0, true) => vec![0x1b, b'O', direction],
        (0, false) => vec![0x1b, b'[', direction],
        (bits, _) => format!("\x1b[1;{}{}", bits + 1, char::from(direction)).into_bytes(),
    }
}

fn function_key_number(n: u8) -> Option<u8> {
    Some(match n {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        _ => return None,
    })
}

fn function_key(n: u8) -> Option<Vec<u8>> {
    match n {
        1..=4 => Some(vec![0x1b, b'O', b'P' + (n - 1)]),
        _ => Some(format!("\x1b[{}~", function_key_number(n)?).into_bytes()),
    }
}

/// The key `key` is, as Muster's keys name it (`muster_core::keys`): a
/// capital for a letter with Shift (the kitty keyboard protocol sends `q`
/// with Shift), and Ctrl+] and Ctrl+\ for the bytes 0x1d and 0x1c, which
/// crossterm reports as Ctrl+5 and Ctrl+4 from a terminal without that
/// protocol. `None` for a key no action can have.
pub(crate) fn chord(key: &KeyEvent) -> Option<Chord> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let key = match key.code {
        KeyCode::Char('5') if ctrl => Key::Char(']'),
        KeyCode::Char('4') if ctrl => Key::Char('\\'),
        KeyCode::Char(c) if shift && c.is_ascii_lowercase() => Key::Char(c.to_ascii_uppercase()),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };
    Some(Chord { key, ctrl, alt })
}

/// The key a hint names by its label, as `Chord::label` writes one (`⏎`,
/// `esc`, `d`, `Shift-R`, `Ctrl-b`, `⇥`), for a click on the hint to press
/// it. `None` for a label of several keys (`↑↓`, `hjkl`, `1-9`) or none.
pub(crate) fn labelled(label: &str) -> Option<KeyEvent> {
    let mut modifiers = KeyModifiers::NONE;
    let mut rest = label;
    loop {
        if let Some(after) = rest.strip_prefix("Ctrl-").filter(|after| !after.is_empty()) {
            modifiers |= KeyModifiers::CONTROL;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("Alt-").filter(|after| !after.is_empty()) {
            modifiers |= KeyModifiers::ALT;
            rest = after;
        } else if let Some(after) = rest
            .strip_prefix("Shift-")
            .filter(|after| !after.is_empty())
        {
            modifiers |= KeyModifiers::SHIFT;
            rest = after;
        } else {
            break;
        }
    }
    let code = match rest {
        "⏎" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "⇥" => KeyCode::Tab,
        "⇧⇥" => KeyCode::BackTab,
        "⌫" => KeyCode::Backspace,
        "↑" => KeyCode::Up,
        "↓" => KeyCode::Down,
        "←" => KeyCode::Left,
        "→" => KeyCode::Right,
        "PgUp" => KeyCode::PageUp,
        "PgDn" => KeyCode::PageDown,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "space" => KeyCode::Char(' '),
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() && !c.is_whitespace() => {
                    if modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_alphabetic() {
                        KeyCode::Char(c.to_ascii_uppercase())
                    } else {
                        KeyCode::Char(c)
                    }
                }
                _ => match rest.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
                    Some(n @ 1..=24) => KeyCode::F(n),
                    _ => return None,
                },
            }
        }
    };
    Some(KeyEvent::new(code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: TerminalModes = TerminalModes {
        kitty_flags: 0,
        bracketed_paste: false,
        application_cursor: false,
        mouse: muster_core::protocol::MouseTracking::Off,
        mouse_encoding: muster_core::protocol::MouseEncoding::Default,
        alternate_screen: false,
    };

    fn kitty(flags: u8) -> TerminalModes {
        TerminalModes {
            kitty_flags: flags,
            ..LEGACY
        }
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn a_hint_s_label_names_the_key_a_click_on_it_presses() {
        for (label, code, modifiers) in [
            ("⏎", KeyCode::Enter, KeyModifiers::NONE),
            ("esc", KeyCode::Esc, KeyModifiers::NONE),
            ("d", KeyCode::Char('d'), KeyModifiers::NONE),
            ("?", KeyCode::Char('?'), KeyModifiers::NONE),
            ("Shift-T", KeyCode::Char('T'), KeyModifiers::SHIFT),
            ("Ctrl-b", KeyCode::Char('b'), KeyModifiers::CONTROL),
            ("Ctrl-]", KeyCode::Char(']'), KeyModifiers::CONTROL),
            ("⇥", KeyCode::Tab, KeyModifiers::NONE),
            ("F5", KeyCode::F(5), KeyModifiers::NONE),
        ] {
            assert_eq!(labelled(label), Some(key(code, modifiers)), "{label}");
            // And it is the key the label was written for.
            let chord = chord(&key(code, modifiers)).unwrap();
            assert_eq!(chord.label(), label);
        }
        for several in ["↑↓", "hjkl", "1-9", "Shift-HJKL", "", "keys"] {
            assert_eq!(labelled(several), None, "{several}");
        }
    }

    fn encode(code: KeyCode, modifiers: KeyModifiers, modes: TerminalModes) -> Option<Vec<u8>> {
        encode_key(&key(code, modifiers), modes)
    }

    #[test]
    fn legacy_text_controls_and_alt() {
        let none = KeyModifiers::NONE;
        assert_eq!(
            encode(KeyCode::Char('a'), none, LEGACY),
            Some(b"a".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Char('é'), none, LEGACY),
            Some("é".as_bytes().to_vec())
        );
        assert_eq!(
            encode(KeyCode::Char('c'), KeyModifiers::CONTROL, LEGACY),
            Some(vec![3])
        );
        assert_eq!(
            encode(KeyCode::Char('f'), KeyModifiers::ALT, LEGACY),
            Some(b"\x1bf".to_vec())
        );
        assert_eq!(encode(KeyCode::Enter, none, LEGACY), Some(b"\r".to_vec()));
        assert_eq!(encode(KeyCode::Backspace, none, LEGACY), Some(vec![0x7f]));
        assert_eq!(
            encode(KeyCode::BackTab, KeyModifiers::SHIFT, LEGACY),
            Some(b"\x1b[Z".to_vec())
        );
    }

    #[test]
    fn legacy_swallows_command_combinations() {
        assert_eq!(
            encode(KeyCode::Char('c'), KeyModifiers::SUPER, LEGACY),
            None
        );
        assert_eq!(
            encode(KeyCode::Backspace, KeyModifiers::SUPER, LEGACY),
            None
        );
    }

    #[test]
    fn cursor_keys_follow_application_cursor_mode() {
        let application = TerminalModes {
            application_cursor: true,
            ..LEGACY
        };
        let none = KeyModifiers::NONE;
        assert_eq!(encode(KeyCode::Up, none, LEGACY), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            encode(KeyCode::Up, none, application),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Home, none, application),
            Some(b"\x1bOH".to_vec())
        );
        // Modified keys have no application form.
        assert_eq!(
            encode(KeyCode::Left, KeyModifiers::CONTROL, application),
            Some(b"\x1b[1;5D".to_vec())
        );
    }

    #[test]
    fn kitty_keeps_text_as_text() {
        let flags = kitty(DISAMBIGUATE);
        assert_eq!(
            encode(KeyCode::Char('a'), KeyModifiers::NONE, flags),
            Some(b"a".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Char('A'), KeyModifiers::SHIFT, flags),
            Some(b"A".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Enter, KeyModifiers::NONE, flags),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn kitty_disambiguates_what_legacy_cannot() {
        let flags = kitty(DISAMBIGUATE);
        assert_eq!(
            encode(KeyCode::Enter, KeyModifiers::SHIFT, flags),
            Some(b"\x1b[13;2u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Esc, KeyModifiers::NONE, flags),
            Some(b"\x1b[27u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Char('p'), KeyModifiers::ALT, flags),
            Some(b"\x1b[112;3u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Char('c'), KeyModifiers::CONTROL, flags),
            Some(b"\x1b[99;5u".to_vec())
        );
        assert_eq!(
            encode(
                KeyCode::Char('P'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                flags
            ),
            Some(b"\x1b[112;6u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Backspace, KeyModifiers::SUPER, flags),
            Some(b"\x1b[127;9u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Up, KeyModifiers::NONE, flags),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Up, KeyModifiers::SUPER, flags),
            Some(b"\x1b[1;9A".to_vec())
        );
    }

    #[test]
    fn kitty_report_all_escapes_plain_keys() {
        let flags = kitty(DISAMBIGUATE | REPORT_ALL);
        assert_eq!(
            encode(KeyCode::Char('a'), KeyModifiers::NONE, flags),
            Some(b"\x1b[97u".to_vec())
        );
        assert_eq!(
            encode(KeyCode::Enter, KeyModifiers::NONE, flags),
            Some(b"\x1b[13u".to_vec())
        );
    }

    #[test]
    fn pastes_are_bracketed_when_asked_and_cannot_end_themselves() {
        assert_eq!(encode_paste("a\nb", LEGACY), b"a\rb".to_vec());
        let bracketed = TerminalModes {
            bracketed_paste: true,
            ..LEGACY
        };
        assert_eq!(
            encode_paste("x\x1b[201~y", bracketed),
            b"\x1b[200~xy\x1b[201~".to_vec()
        );
    }
}
