//! Framing without IO: `[u32 little-endian length][MessagePack payload]`.
//!
//! The transport (in `muster-client`) moves bytes; this module turns them into
//! messages and back. A frame's length is checked before anything is buffered
//! for it, so a hostile peer cannot make the reader allocate. The header
//! never changes: every protocol version's handshake is frozen with it
//! (`tests/frozen_protocol.rs`, `ARC-09`).

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The largest payload either side accepts.
pub const MAX_FRAME_LEN: usize = 4 * 1024 * 1024;

const HEADER_LEN: usize = 4;

/// Why a frame could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("a frame of {0} bytes is over the {MAX_FRAME_LEN}-byte limit")]
    TooLarge(usize),
    #[error("could not encode a frame: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("could not decode a frame: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// Encodes `message` as one frame, header included.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, FrameError> {
    let mut frame = vec![0; HEADER_LEN];
    rmp_serde::encode::write_named(&mut frame, message)?;
    let len = frame.len() - HEADER_LEN;
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }
    let header = u32::try_from(len).map_err(|_| FrameError::TooLarge(len))?;
    frame[..HEADER_LEN].copy_from_slice(&header.to_le_bytes());
    Ok(frame)
}

/// Collects bytes as they arrive and hands out whole frames.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds bytes read from the transport.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole frame, if one has arrived. After an error the stream is
    /// out of step and the connection must be closed.
    pub fn next_frame<T: DeserializeOwned>(&mut self) -> Result<Option<T>, FrameError> {
        let Some(header) = self.buffer.first_chunk::<HEADER_LEN>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(*header) as usize;
        if len > MAX_FRAME_LEN {
            return Err(FrameError::TooLarge(len));
        }
        if self.buffer.len() < HEADER_LEN + len {
            return Ok(None);
        }
        let message = rmp_serde::from_slice(&self.buffer[HEADER_LEN..HEADER_LEN + len]);
        self.buffer.drain(..HEADER_LEN + len);
        Ok(Some(message?))
    }

    /// Whether part of a frame is waiting for more bytes.
    pub fn has_partial_frame(&self) -> bool {
        !self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::protocol::{ClientFrame, Command, Delta, Hello, ServerFrame};
    use crate::session::{SessionStatus, WaitReason};
    use crate::{SessionId, Timestamp};

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(message: T) {
        let mut decoder = FrameDecoder::new();
        decoder.push(&encode(&message).unwrap());
        assert_eq!(decoder.next_frame::<T>().unwrap(), Some(message));
        assert!(!decoder.has_partial_frame());
    }

    #[test]
    fn protocol_messages_round_trip() {
        round_trip(ClientFrame::Hello(Hello::current("test 0.1")));
        round_trip(ClientFrame::Subscribe);
        round_trip(ClientFrame::Command {
            req_id: 7,
            command: Command::AddProject {
                root: PathBuf::from("/src/app"),
                name: None,
                space: None,
            },
        });
        round_trip(ServerFrame::Delta(Delta::SessionStatus {
            session: SessionId::new(),
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            unseen: true,
            since: crate::Timestamp::from_millis(1_760_000_000_000),
        }));
        round_trip(ServerFrame::Delta(Delta::SessionActivity {
            session: SessionId::new(),
            activity: Some(crate::Activity {
                text: "editing view.rs".to_owned(),
                since: crate::Timestamp::from_millis(1_760_000_000_000),
            }),
        }));
        round_trip(ServerFrame::Output {
            session: SessionId::new(),
            seq: 1 << 40,
            data: b"\x1b[31mred\x1b[0m".to_vec(),
        });
        round_trip(Timestamp::from_millis(1_760_000_000_000));
    }

    #[test]
    fn terminal_bytes_are_sent_as_binary_not_as_a_list_of_numbers() {
        let data = vec![0xff; 1000];
        let frame = encode(&ServerFrame::Output {
            session: SessionId::new(),
            seq: 0,
            data,
        })
        .unwrap();
        assert!(frame.len() < 1100, "{} bytes", frame.len());
    }

    #[test]
    fn frames_split_across_reads_are_reassembled() {
        let first = encode(&ClientFrame::Subscribe).unwrap();
        let second = encode(&ClientFrame::Hello(Hello::current("t"))).unwrap();
        let stream = [first, second].concat();

        let mut decoder = FrameDecoder::new();
        let mut frames = Vec::new();
        for byte in stream {
            decoder.push(&[byte]);
            while let Some(frame) = decoder.next_frame::<ClientFrame>().unwrap() {
                frames.push(frame);
            }
        }
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], ClientFrame::Subscribe);
    }

    #[test]
    fn an_oversized_length_is_refused_before_its_payload_arrives() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&(u32::MAX).to_le_bytes());
        assert!(matches!(
            decoder.next_frame::<ClientFrame>(),
            Err(FrameError::TooLarge(_))
        ));
    }

    #[test]
    fn an_oversized_message_is_not_encoded() {
        let big = "x".repeat(MAX_FRAME_LEN);
        assert!(matches!(encode(&big), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn garbage_is_a_decode_error() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&3u32.to_le_bytes());
        decoder.push(&[0xc1, 0xc1, 0xc1]);
        assert!(matches!(
            decoder.next_frame::<ClientFrame>(),
            Err(FrameError::Decode(_))
        ));
    }

    #[test]
    fn named_fields_let_old_readers_skip_new_fields() {
        #[derive(Serialize)]
        struct NewerHello {
            min_version: u32,
            max_version: u32,
            client: String,
            capabilities: Vec<String>,
            added_later: bool,
        }
        let mut decoder = FrameDecoder::new();
        decoder.push(
            &encode(&NewerHello {
                min_version: 1,
                max_version: 3,
                client: "future".to_owned(),
                capabilities: vec!["x".to_owned()],
                added_later: true,
            })
            .unwrap(),
        );
        let hello: Hello = decoder.next_frame().unwrap().unwrap();
        assert_eq!(hello.max_version, 3);
    }
}
