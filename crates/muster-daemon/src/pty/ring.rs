//! A session's recent terminal output, numbered by byte.
//!
//! Every byte a session writes has a sequence number: the count of bytes it
//! wrote before it. A client that remembers the next number it expects can
//! re-attach and receive exactly what it missed, if the ring still holds it.

use std::collections::VecDeque;

/// How much output each session keeps.
pub(crate) const RING_CAPACITY: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Ring {
    bytes: VecDeque<u8>,
    capacity: usize,
    /// The sequence number of the oldest byte held.
    start: u64,
}

/// What a re-attaching client should be sent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Replay {
    /// The sequence number of the first byte of `data`.
    pub(crate) seq: u64,
    pub(crate) data: Vec<u8>,
    /// The client's position is no longer held (or it had none), so it must
    /// clear its screen before drawing `data`.
    pub(crate) reset: bool,
}

impl Ring {
    #[cfg(test)]
    pub(crate) fn new(capacity: usize) -> Self {
        Self::starting_at(capacity, 0)
    }

    /// A ring whose first byte will be numbered `start`.
    pub(crate) fn starting_at(capacity: usize, start: u64) -> Self {
        Self {
            bytes: VecDeque::with_capacity(capacity.min(64 * 1024)),
            capacity,
            start,
        }
    }

    /// The sequence number the next byte will get.
    pub(crate) fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }

    /// Appends output, dropping the oldest bytes beyond the capacity, and
    /// returns the sequence number of `data`'s first byte.
    pub(crate) fn push(&mut self, data: &[u8]) -> u64 {
        let seq = self.end();
        // Only the newest `capacity` bytes of an oversized chunk can be kept.
        let kept = &data[data.len().saturating_sub(self.capacity)..];
        let overflow = (self.bytes.len() + kept.len()).saturating_sub(self.capacity);
        self.bytes.drain(..overflow);
        self.bytes.extend(kept);
        // Whatever is not held now has been dropped, so the ring still ends
        // at the total number of bytes ever written.
        self.start = seq + data.len() as u64 - self.bytes.len() as u64;
        seq
    }

    /// The output from `from` on, or everything held when `from` is missing,
    /// already dropped, or beyond what was written.
    pub(crate) fn replay_from(&self, from: Option<u64>) -> Replay {
        match from {
            Some(from) if (self.start..=self.end()).contains(&from) => {
                let skip = usize::try_from(from - self.start).unwrap_or(usize::MAX);
                Replay {
                    seq: from,
                    data: self.bytes.iter().skip(skip).copied().collect(),
                    reset: false,
                }
            }
            _ => Replay {
                seq: self.start,
                data: self.bytes.iter().copied().collect(),
                reset: true,
            },
        }
    }
}
