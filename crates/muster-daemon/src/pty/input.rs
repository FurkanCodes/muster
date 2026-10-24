//! What is typed and pasted into a session's terminal, on its way to the
//! thread that writes it: never dropped while the program reads it, and
//! never waited on by the actor, but bounded by bytes (`ARC-14`), so the
//! typing of a person whose program has stopped reading (stopped with
//! Ctrl+Z, or hung) does not pile up without end.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The most that waits for the program to read it: far more than any
/// paste, and nothing a stopped program could make of more.
const MOST_WAITING: usize = 4 << 20;

/// Where the actor puts what is typed.
pub(super) struct Input {
    sender: std::sync::mpsc::Sender<Vec<u8>>,
    waiting: Arc<AtomicUsize>,
}

/// Where the writer takes it from.
pub(super) struct Typed {
    receiver: std::sync::mpsc::Receiver<Vec<u8>>,
    waiting: Arc<AtomicUsize>,
}

/// A queue of what is typed, its two ends.
pub(super) fn queue() -> (Input, Typed) {
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: bounded by bytes (MOST_WAITING); the actor must not wait on the writer"
    )]
    let (sender, receiver) = std::sync::mpsc::channel();
    let waiting = Arc::new(AtomicUsize::new(0));
    (
        Input {
            sender,
            waiting: waiting.clone(),
        },
        Typed { receiver, waiting },
    )
}

impl Input {
    /// Queues `data` for the program; refused, said in the log, while more
    /// than [`MOST_WAITING`] waits already for it to read.
    pub(super) fn send(&self, data: Vec<u8>) -> bool {
        let length = data.len();
        if self.waiting.fetch_add(length, Ordering::Relaxed) + length > MOST_WAITING {
            self.waiting.fetch_sub(length, Ordering::Relaxed);
            tracing::warn!(length, "dropped typing: the session's program reads none");
            return false;
        }
        self.sender.send(data).is_ok()
    }
}

impl Typed {
    /// The next of what was typed, or `None` once the actor has gone.
    pub(super) fn next(&self) -> Option<Vec<u8>> {
        let data = self.receiver.recv().ok()?;
        self.waiting.fetch_sub(data.len(), Ordering::Relaxed);
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_for_a_program_that_reads_none_stops_at_the_bound() {
        let (input, typed) = queue();
        let chunk = vec![b'x'; 1 << 20];
        for _ in 0..4 {
            assert!(input.send(chunk.clone()));
        }
        assert!(!input.send(vec![b'y']), "past the bound");
        // Once the program reads, there is room again.
        assert_eq!(typed.next().map(|data| data.len()), Some(1 << 20));
        assert!(input.send(vec![b'y']));
    }
}
