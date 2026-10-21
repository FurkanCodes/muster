//! The screen a terminal's output draws, kept beside the bytes.
//!
//! The ring keeps a terminal's output as bytes, so a client that missed
//! some can be sent exactly what it missed. But the ring holds only the
//! newest megabyte: one that starts over (a new client, or one that fell
//! further behind than the ring holds) is sent bytes cut from the middle of
//! the output, which cannot draw the screen on their own, since what came
//! before them (the alternate screen, the scroll region, where the cursor
//! was) is gone. So every terminal keeps its screen too, at the terminal's
//! size, drawn from its first byte (by its actor, when it has nothing else
//! to do), and a client starting over is sent it whole after the ring's
//! bytes (`ServerFrame::Replay::screen`). It also
//! answers a program asking where its cursor is (`CSI 6 n`): crossterm's
//! `cursor::position()`, which inline viewports and line editors call at
//! startup, waits for the answer and gives up after about two seconds.
//!
//! Adapted from Nebula's `pty/cursor.rs` (MIT, Copyright (c) 2026 Cody
//! Seibert; see NOTICE).

/// vt100 underflows below two cells a side, and a client can ask for a
/// terminal squeezed to nothing.
const MIN_SIDE: u16 = 2;

pub(crate) struct Screen {
    /// No scrollback: the ring holds what scrolled away.
    parser: vt100::Parser,
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screen")
            .field("cursor", &self.parser.screen().cursor_position())
            .finish()
    }
}

impl Screen {
    /// A screen at the terminal's size with `drawn` already drawn on it:
    /// nothing for a new terminal, the screen a handed-over one carried.
    pub(crate) fn new(cols: u16, rows: u16, drawn: &[u8]) -> Self {
        let (cols, rows) = floor(cols, rows);
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(drawn);
        Self { parser }
    }

    pub(crate) fn feed(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = floor(cols, rows);
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// The answer to `CSI 6 n`: `CSI row ; col R`, counting from 1.
    pub(crate) fn report(&self) -> Vec<u8> {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        // After writing the last column, vt100 parks the cursor one past it
        // until the wrap happens; a terminal reports the last column.
        let col = col.min(screen.size().1 - 1);
        format!("\x1b[{};{}R", row + 1, col + 1).into_bytes()
    }

    /// The screen drawn whole, for a terminal that shows anything at all:
    /// the main screen or the alternate one, as the program has it, the
    /// whole of it scrolling and counted from its top, then every cell,
    /// the cursor and whether it shows, the colours being drawn in, and the
    /// input modes. What scrolled away is not in it.
    pub(crate) fn picture(&self) -> Vec<u8> {
        let screen = self.parser.screen();
        let mut picture = Vec::new();
        picture.extend_from_slice(if screen.alternate_screen() {
            b"\x1b[?1049h"
        } else {
            b"\x1b[?1049l"
        });
        picture.extend_from_slice(b"\x1b[r\x1b[?6l");
        picture.extend(screen.state_formatted());
        picture
    }
}

fn floor(cols: u16, rows: u16) -> (u16, u16) {
    (cols.max(MIN_SIDE), rows.max(MIN_SIDE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_places_the_cursor() {
        assert_eq!(
            Screen::new(80, 24, b"$ muse\r\nhello").report(),
            b"\x1b[2;6R"
        );
    }

    #[test]
    fn output_moves_it() {
        let mut screen = Screen::new(80, 24, b"");
        assert_eq!(screen.report(), b"\x1b[1;1R");
        screen.feed(b"\x1b[10;20H");
        assert_eq!(screen.report(), b"\x1b[10;20R");
    }

    #[test]
    fn a_pending_wrap_reports_the_last_column() {
        let mut screen = Screen::new(5, 3, b"abcde");
        assert_eq!(screen.report(), b"\x1b[1;5R");
        screen.feed(b"f");
        assert_eq!(screen.report(), b"\x1b[2;2R");
    }

    #[test]
    fn a_resize_clamps_the_cursor() {
        let mut screen = Screen::new(80, 24, b"\x1b[20;70H");
        screen.resize(40, 10);
        assert_eq!(screen.report(), b"\x1b[10;40R");
    }

    #[test]
    fn a_squeezed_terminal_does_not_panic() {
        let nasty = "wrap wrap\r\n漢字😀\x1b[999;999Hx\n\n".as_bytes();
        for (cols, rows) in [(0, 0), (1, 1), (1, 5), (5, 1), (2, 2)] {
            let mut screen = Screen::new(cols, rows, nasty);
            screen.resize(cols, rows);
            screen.feed(nasty);
            let corner = format!("\x1b[{};{}R", rows.max(2), cols.max(2));
            assert_eq!(screen.report(), corner.as_bytes(), "{cols}x{rows}");
            let _ = screen.picture();
        }
    }

    /// A client's screen: what it shows after `before`, then the picture.
    fn drawn_over(before: &[u8], picture: &[u8], cols: u16, rows: u16) -> vt100::Parser {
        let mut client = vt100::Parser::new(rows, cols, 0);
        client.process(before);
        client.process(picture);
        client
    }

    fn same(client: &vt100::Parser, program: &Screen) {
        let (client, program) = (client.screen(), program.parser.screen());
        assert_eq!(client.contents_formatted(), program.contents_formatted());
        assert_eq!(client.cursor_position(), program.cursor_position());
        assert_eq!(client.alternate_screen(), program.alternate_screen());
        assert_eq!(client.hide_cursor(), program.hide_cursor());
        assert_eq!(client.bracketed_paste(), program.bracketed_paste());
        assert_eq!(client.application_cursor(), program.application_cursor());
    }

    /// What a program like an agent draws: a prompt line redrawn in place,
    /// in colour, under lines it printed once.
    const AGENT: &[u8] = b"\x1b[?2004h\x1b[?1h\x1b[32mdone\x1b[m one\r\n\
        two\r\n\x1b[1m> \x1b[mtyping\x1b[K\r\x1b[2A\x1b[2K\x1b[33mredrawn\x1b[m\r\n\
        \r\n\x1b[1m> \x1b[mtyping more\x1b[?25l";

    #[test]
    fn the_picture_draws_the_program_s_screen_exactly() {
        let program = Screen::new(30, 6, AGENT);
        same(&drawn_over(b"", &program.picture(), 30, 6), &program);
    }

    #[test]
    fn the_picture_mends_a_screen_drawn_from_bytes_cut_partway() {
        let program = Screen::new(30, 6, AGENT);
        // What a client starting over draws from a ring that kept only the
        // newest bytes: the middle of a sequence, a cursor in the wrong
        // place, a scroll region and the origin mode set, colours on.
        let cut = b"5;1H\x1b[2;4r\x1b[?6h\x1b[41mstray\x1b[3Bmore\x1b[1;1Hx";
        let client = drawn_over(cut, &program.picture(), 30, 6);
        same(&client, &program);
        // Output after the picture lands where the program means it to,
        // the whole screen scrolling.
        let (mut client, mut program) = (client, program);
        let next = b"\x1b[6;1Hlast\r\n\r\nscrolled";
        client.process(next);
        program.feed(next);
        same(&client, &program);
    }

    #[test]
    fn the_picture_takes_the_client_to_the_program_s_screen_main_or_alternate() {
        let full = Screen::new(20, 4, b"shell\r\n\x1b[?1049h\x1b[Heditor");
        let client = drawn_over(b"", &full.picture(), 20, 4);
        same(&client, &full);
        // A client left on the alternate screen by bytes cut partway is
        // brought back to the main one the program is on.
        let shell = Screen::new(20, 4, b"$ ls\r\nfile");
        same(
            &drawn_over(b"\x1b[?1049hvim", &shell.picture(), 20, 4),
            &shell,
        );
    }
}
