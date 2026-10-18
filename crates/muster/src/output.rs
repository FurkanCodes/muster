//! A session's terminal stream written out as it comes, as `muster check`
//! does: each byte once, in order. A client that could not keep up is sent
//! the stream again from where it was, or, if it fell further behind than
//! the daemon keeps, from the oldest byte kept (`ARC-14`); what was not
//! kept is counted, so the gap is said rather than silent.

/// Where the stream written so far ends.
#[derive(Debug, Default)]
pub(crate) struct Position {
    next: Option<u64>,
}

/// What of a frame's bytes to write.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Fresh<'a> {
    /// The bytes not written yet.
    pub(crate) data: &'a [u8],
    /// Bytes the daemon no longer held, between what was written and these.
    pub(crate) missed: u64,
}

impl Position {
    /// Takes the bytes of a frame that start at `seq`.
    pub(crate) fn take<'a>(&mut self, seq: u64, data: &'a [u8]) -> Fresh<'a> {
        let next = self.next.unwrap_or(seq);
        let skip = usize::try_from(next.saturating_sub(seq)).unwrap_or(usize::MAX);
        let fresh = Fresh {
            data: data.get(skip..).unwrap_or_default(),
            missed: seq.saturating_sub(next),
        };
        self.next = Some(next.max(seq + data.len() as u64));
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_byte_is_written_once_and_a_gap_is_counted() {
        let mut position = Position::default();
        let first = position.take(5, b"hello");
        assert_eq!((first.data, first.missed), (&b"hello"[..], 0));
        // Sent again from where it was, overlapping what was written.
        let again = position.take(8, b"lo world");
        assert_eq!((again.data, again.missed), (&b" world"[..], 0));
        // Wholly old.
        assert_eq!(position.take(5, b"hell").data, b"");
        // Further behind than was kept: the bytes between are missed.
        let later = position.take(30, b"end");
        assert_eq!((later.data, later.missed), (&b"end"[..], 14));
    }
}
