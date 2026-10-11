//! Frames over any async byte stream: the IO half of `muster_core::frame`.

use std::io;

use muster_core::frame::{self, FrameDecoder, FrameError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Why a frame could not be sent or received.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connection error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("the connection closed in the middle of a frame")]
    Truncated,
}

/// Reads whole frames from a byte stream.
#[derive(Debug)]
pub struct FramedRead<R> {
    reader: R,
    decoder: FrameDecoder,
    chunk: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FramedRead<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: FrameDecoder::new(),
            chunk: vec![0; 16 * 1024],
        }
    }

    /// The next frame, or `None` when the peer closed the stream cleanly
    /// between frames.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<Option<T>, TransportError> {
        loop {
            if let Some(message) = self.decoder.next_frame()? {
                return Ok(Some(message));
            }
            let read = self.reader.read(&mut self.chunk).await?;
            if read == 0 {
                return match self.decoder.has_partial_frame() {
                    true => Err(TransportError::Truncated),
                    false => Ok(None),
                };
            }
            self.decoder.push(&self.chunk[..read]);
        }
    }
}

/// Writes whole frames to a byte stream.
#[derive(Debug)]
pub struct FramedWrite<W> {
    writer: W,
}

impl<W: AsyncWrite + Unpin> FramedWrite<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Encodes and writes one frame.
    pub async fn send<T: Serialize>(&mut self, message: &T) -> Result<(), TransportError> {
        let bytes = frame::encode(message)?;
        self.send_bytes(&bytes).await
    }

    /// Writes bytes as they are. For tests that must send malformed frames.
    pub async fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.writer.write_all(bytes).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Closes the writing side.
    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        Ok(self.writer.shutdown().await?)
    }
}

#[cfg(test)]
mod tests {
    use muster_core::protocol::ClientFrame;

    use super::*;

    #[tokio::test]
    async fn frames_cross_a_stream_and_eof_ends_cleanly() {
        let (client, server) = tokio::io::duplex(64);
        let (mut write, mut read) = (FramedWrite::new(client), FramedRead::new(server));
        write.send(&ClientFrame::Subscribe).await.unwrap();
        write.shutdown().await.unwrap();
        drop(write);
        assert_eq!(read.next().await.unwrap(), Some(ClientFrame::Subscribe));
        assert_eq!(read.next::<ClientFrame>().await.unwrap(), None);
    }

    #[tokio::test]
    async fn eof_inside_a_frame_is_an_error() {
        let (client, server) = tokio::io::duplex(64);
        let mut write = FramedWrite::new(client);
        write.send_bytes(&[10, 0, 0, 0, 1]).await.unwrap();
        drop(write);
        let error = FramedRead::new(server).next::<ClientFrame>().await;
        assert!(matches!(error, Err(TransportError::Truncated)));
    }
}
