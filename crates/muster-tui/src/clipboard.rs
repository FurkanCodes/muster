//! The person's clipboard, for text copied from a session: through the
//! system's own command where the UI runs on the person's machine, else
//! through their terminal (OSC 52), which reaches it over SSH too.
//!
//! Text gets there two ways: the person drags over a session's text, or a
//! program in the session they look at copies with OSC 52
//! (`ESC ] 52 ; c ; <base64> ST`), as it would in their own terminal. A
//! program's copy is taken only from the output it writes while the
//! person looks on, never from a session's history replayed as a client
//! attaches, which would copy again on every look; it must be text, and
//! no larger than [`MAX_COPY`]. A program asking to read the clipboard is
//! never answered. What is copied is never logged (`PRD-06`).

use std::io::Write;
use std::process::{Command, Stdio};

use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

/// The most a program may copy, decoded.
pub(crate) const MAX_COPY: usize = 192 * 1024;

/// How copied text reaches the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    /// A program that takes the text on its standard input.
    Program(&'static str, &'static [&'static str]),
    /// The person's terminal, asked with OSC 52.
    Terminal,
}

/// The way for a UI on `os`, with `set` saying which environment variables
/// are set. Over SSH the machine's clipboard is not the person's, so it is
/// their terminal's.
fn way(os: &str, set: impl Fn(&str) -> bool) -> Way {
    if set("SSH_CONNECTION") || set("SSH_TTY") {
        return Way::Terminal;
    }
    match os {
        "macos" => Way::Program("pbcopy", &[]),
        "linux" if set("WAYLAND_DISPLAY") => Way::Program("wl-copy", &[]),
        "linux" if set("DISPLAY") => Way::Program("xclip", &["-selection", "clipboard"]),
        _ => Way::Terminal,
    }
}

/// Puts `text` on the person's clipboard; through their terminal if the
/// program cannot be started.
pub(crate) fn copy(text: &str) {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if let Way::Program(program, args) = way(std::env::consts::OS, set)
        && piped(program, args, text)
    {
        return;
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(&osc52(text));
    let _ = out.flush();
}

/// Starts `program` with `text` on its standard input, written and reaped
/// in the background; whether it started.
fn piped(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let text = text.to_owned();
    // Ends once the program has its text and has exited.
    std::thread::Builder::new()
        .name("muster-clipboard".to_owned())
        .spawn(move || {
            // Closed once written, so the program goes on.
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(text.as_bytes());
            }
            let _ = child.wait();
        })
        .is_ok()
}

/// What a program's OSC 52 copy holds, from its base64 `data`: text of at
/// most [`MAX_COPY`] bytes, valid UTF-8 with no control characters but
/// tabs and line breaks, and not empty. Anything else copies nothing.
pub(crate) fn from_program(data: &[u8]) -> Option<String> {
    // Larger encoded than the most it may hold decoded: not decoded at all.
    if data.len() > MAX_COPY.div_ceil(3) * 4 {
        return None;
    }
    // Padded or not, as programs send it either way.
    let lenient = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let bytes = lenient.decode(data).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let text_only = text
        .chars()
        .all(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'));
    (!text.is_empty() && text.len() <= MAX_COPY && text_only).then_some(text)
}

/// The OSC 52 sequence asking a terminal to put `text` on the clipboard.
fn osc52(text: &str) -> Vec<u8> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    format!("\x1b]52;c;{encoded}\x07").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_s_own_program_copies_but_over_ssh_the_terminal_does() {
        let none = |_: &str| false;
        assert_eq!(way("macos", none), Way::Program("pbcopy", &[]));
        let over_ssh = |name: &str| name == "SSH_CONNECTION";
        assert_eq!(way("macos", over_ssh), Way::Terminal);
        let wayland = |name: &str| name == "WAYLAND_DISPLAY";
        assert_eq!(way("linux", wayland), Way::Program("wl-copy", &[]));
        let x11 = |name: &str| name == "DISPLAY";
        assert_eq!(
            way("linux", x11),
            Way::Program("xclip", &["-selection", "clipboard"])
        );
        // A console, with no clipboard of its own.
        assert_eq!(way("linux", none), Way::Terminal);
    }

    #[test]
    fn the_terminal_is_sent_the_text_encoded() {
        assert_eq!(osc52("copy-42\nok"), b"\x1b]52;c;Y29weS00Mgpvaw==\x07");
    }

    #[test]
    fn a_program_s_copy_is_its_text_padded_or_not() {
        assert_eq!(
            from_program(b"Y29weS00Mgpvaw==").as_deref(),
            Some("copy-42\nok")
        );
        assert_eq!(
            from_program(b"Y29weS00Mgpvaw").as_deref(),
            Some("copy-42\nok")
        );
        assert_eq!(
            from_program(b"aMOpbGxvCXdvcmxk").as_deref(),
            Some("héllo\tworld")
        );
    }

    #[test]
    fn a_program_copies_only_text_of_at_most_the_limit() {
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        // Nothing, bytes that are not UTF-8, control characters, and what
        // is not base64 at all.
        assert_eq!(from_program(b""), None);
        assert_eq!(from_program(encode(&[0xff, 0xfe, 0x00]).as_bytes()), None);
        assert_eq!(from_program(encode(b"rm -rf /\x1b[8m").as_bytes()), None);
        assert_eq!(from_program(encode(b"a\0b").as_bytes()), None);
        assert_eq!(from_program(b"!!!!"), None);
        // At the limit, and past it.
        let most = "x".repeat(MAX_COPY);
        assert_eq!(from_program(encode(most.as_bytes()).as_bytes()), Some(most));
        let more = "x".repeat(MAX_COPY + 1);
        assert_eq!(from_program(encode(more.as_bytes()).as_bytes()), None);
        assert_eq!(from_program(&vec![b'A'; 4 * MAX_COPY]), None);
    }
}
