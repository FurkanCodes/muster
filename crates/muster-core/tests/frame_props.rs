//! The frame decoder against anything a client could send: bytes in any
//! pieces never panic it, and frames split anywhere arrive whole.

use muster_core::frame::{FrameDecoder, encode};
use proptest::collection::vec;
use proptest::prelude::*;

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

type Message = (u32, String);

fn message() -> impl Strategy<Value = Message> {
    (
        any::<u32>(),
        vec(any::<char>(), 0..40).prop_map(String::from_iter),
    )
}

proptest! {
    #[test]
    fn any_bytes_in_any_pieces_never_panic_it(
        bytes in vec(any::<u8>(), 0..2048),
        cuts in vec(any::<usize>(), 0..8),
    ) {
        let mut decoder = FrameDecoder::new();
        for piece in pieces(&bytes, &cuts) {
            decoder.push(piece);
            // After an error the stream is out of step and is closed.
            while let Ok(Some(_)) = decoder.next_frame::<Message>() {}
        }
    }

    #[test]
    fn frames_split_anywhere_arrive_whole(
        messages in vec(message(), 1..8),
        cuts in vec(any::<usize>(), 0..12),
    ) {
        let bytes: Vec<u8> = messages
            .iter()
            .flat_map(|message| encode(message).unwrap())
            .collect();
        let mut decoder = FrameDecoder::new();
        let mut arrived = Vec::new();
        for piece in pieces(&bytes, &cuts) {
            decoder.push(piece);
            while let Some(message) = decoder.next_frame::<Message>().unwrap() {
                arrived.push(message);
            }
        }
        prop_assert_eq!(arrived, messages);
        prop_assert!(!decoder.has_partial_frame());
    }
}
