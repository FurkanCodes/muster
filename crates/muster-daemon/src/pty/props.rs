//! The terminal scanners against anything a program could write: no bytes
//! panic them, and output read in any pieces is scanned as if read whole,
//! as a terminal's reads split it anywhere.

use proptest::collection::vec;
use proptest::prelude::*;

use super::colours::{Colour, Ending};
use super::osc::{OscScanner, Report};
use super::queries::{QueryScanner, Reply};

/// `bytes` cut at `cuts`, each taken modulo its length.
fn pieces<'a>(bytes: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut at: Vec<usize> = cuts.iter().map(|cut| cut % (bytes.len() + 1)).collect();
    at.sort_unstable();
    let mut pieces = Vec::new();
    let mut from = 0;
    for cut in at {
        pieces.push(&bytes[from..cut]);
        from = cut;
    }
    pieces.push(&bytes[from..]);
    pieces
}

/// Mostly escape sequences and their parts, so the scanners' states are
/// all reached; any byte at all now and then.
fn output() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just(b"\x1b".to_vec()),
        Just(b"]".to_vec()),
        Just(b"[".to_vec()),
        Just(b"\x07".to_vec()),
        Just(b"\x1b\\".to_vec()),
        Just(b"0;title".to_vec()),
        Just(b"9;4;3".to_vec()),
        Just(b"9;4;0".to_vec()),
        Just(b"?2004h".to_vec()),
        Just(b"?1049l".to_vec()),
        Just(b">5u".to_vec()),
        Just(b"6n".to_vec()),
        Just(b"c".to_vec()),
        Just(b"10;?".to_vec()),
        Just(b"11;?".to_vec()),
        Just(b"4;1;?".to_vec()),
        Just(b";?".to_vec()),
        Just(b"14t".to_vec()),
        Just(b"16t".to_vec()),
        Just(b"\x18".to_vec()),
        vec(any::<u8>(), 0..8),
    ];
    vec(piece, 0..40).prop_map(|pieces| pieces.concat())
}

/// One part of the replies owed, however the output was read.
#[derive(Debug, PartialEq, Eq)]
enum Owed {
    Bytes(Vec<u8>),
    Cursor,
    Colour(Colour, Ending),
    Pixels(bool),
}

/// Replies as one stream: bytes run together, each cursor and colour
/// question kept.
fn stream(replies: impl IntoIterator<Item = Reply>) -> Vec<Owed> {
    let mut stream: Vec<Owed> = Vec::new();
    for reply in replies {
        match (reply, stream.last_mut()) {
            (Reply::Bytes(bytes), Some(Owed::Bytes(pending))) => pending.extend(bytes),
            (Reply::Bytes(bytes), _) => stream.push(Owed::Bytes(bytes)),
            (Reply::CursorPosition { .. }, _) => stream.push(Owed::Cursor),
            (Reply::Colour { colour, ending }, _) => stream.push(Owed::Colour(colour, ending)),
            (Reply::PixelSize { cell }, _) => stream.push(Owed::Pixels(cell)),
        }
    }
    stream
}

proptest! {
    #[test]
    fn titles_and_progress_read_the_same_in_any_pieces(
        bytes in output(),
        cuts in vec(any::<usize>(), 0..10),
    ) {
        let whole = OscScanner::default().scan(&bytes);
        let mut scanner = OscScanner::default();
        let pieced: Vec<Report> = pieces(&bytes, &cuts)
            .into_iter()
            .flat_map(|piece| scanner.scan(piece))
            .collect();
        prop_assert_eq!(pieced, whole);
    }

    #[test]
    fn questions_and_modes_read_the_same_in_any_pieces(
        bytes in output(),
        cuts in vec(any::<usize>(), 0..10),
    ) {
        let mut one = QueryScanner::default();
        let whole = stream(one.scan(&bytes).replies);
        let mut scanner = QueryScanner::default();
        let pieced = stream(
            pieces(&bytes, &cuts)
                .into_iter()
                .flat_map(|piece| scanner.scan(piece).replies),
        );
        prop_assert_eq!(pieced, whole);
        prop_assert_eq!(scanner.modes(), one.modes());
    }
}
