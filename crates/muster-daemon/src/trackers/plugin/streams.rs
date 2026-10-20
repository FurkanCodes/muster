//! What a ticket plugin's pipes carry, bounded (`ARC-14`): its lines, at
//! most so long each, and the end of what it says on stderr, its log.

use std::collections::VecDeque;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::secrets::Token;

/// The most of a plugin's stderr kept, as its log.
pub(crate) const MOST_LOG: usize = 64 * 1024;

/// Lines read from a pipe, each at most so long. Cancel-safe: what has been
/// read of a line is kept here, so a `select!` may drop a call and make
/// another.
#[derive(Debug)]
pub(crate) struct Lines<R> {
    reader: BufReader<R>,
    line: Vec<u8>,
    most: usize,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    pub(crate) fn new(reader: R, most: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            line: Vec::new(),
            most,
        }
    }

    /// The next line without its newline; none at the end. A line longer
    /// than the most is an `InvalidData` error.
    pub(crate) async fn next(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        loop {
            let read = self.reader.fill_buf().await?;
            if read.is_empty() {
                // A last line with no newline is a line too.
                return Ok((!self.line.is_empty()).then(|| std::mem::take(&mut self.line)));
            }
            let (part, used, ended) = match read.iter().position(|byte| *byte == b'\n') {
                Some(end) => (&read[..end], end + 1, true),
                None => (read, read.len(), false),
            };
            if self.line.len() + part.len() > self.most {
                self.line.clear();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("a line longer than {} bytes", self.most),
                ));
            }
            self.line.extend_from_slice(part);
            self.reader.consume(used);
            if ended {
                return Ok(Some(std::mem::take(&mut self.line)));
            }
        }
    }
}

/// The end of what a plugin said on stderr, across its restarts, never
/// showing its secrets' values (`PRD-06`).
#[derive(Debug, Default)]
pub(crate) struct Log {
    bytes: VecDeque<u8>,
    secrets: Vec<Token>,
}

/// The most secrets' values a log hides: a plugin's, as they were each
/// time it started.
const MOST_SECRETS: usize = 32;

impl Log {
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        let over = self.bytes.len().saturating_sub(MOST_LOG);
        self.bytes.drain(..over);
    }

    /// Hides `secrets`' values from now on, wherever they are in it.
    pub(crate) fn hide(&mut self, secrets: impl IntoIterator<Item = Token>) {
        for secret in secrets {
            if !secret.value().is_empty() && !self.secrets.contains(&secret) {
                self.secrets.push(secret);
            }
        }
        let over = self.secrets.len().saturating_sub(MOST_SECRETS);
        self.secrets.drain(..over);
    }

    /// What it holds, its secrets' values and anything shaped like a
    /// credential hidden.
    pub(crate) fn text(&self) -> String {
        let (front, back) = self.bytes.as_slices();
        let mut bytes = front.to_vec();
        bytes.extend_from_slice(back);
        self.hidden(&String::from_utf8_lossy(&bytes))
    }

    /// `text` with its plugin's secrets' values and anything shaped like a
    /// credential hidden, as it would show it.
    pub(crate) fn hidden(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for secret in &self.secrets {
            text = text.replace(secret.value(), muster_core::credentials::HIDDEN);
        }
        muster_core::credentials::hide(&text).0
    }

    /// Its last line with something on it, on one line, its credentials
    /// hidden and cut, to say why a plugin ended.
    pub(crate) fn last_line(&self) -> Option<String> {
        const MOST: usize = 300;
        let text = self.text();
        let line = text.lines().rev().find(|line| !line.trim().is_empty())?;
        let mut line = muster_core::text::one_line(line.trim());
        if line.len() > MOST {
            let end = (0..=MOST)
                .rev()
                .find(|index| line.is_char_boundary(*index))
                .unwrap_or(0);
            line.truncate(end);
            line.push('…');
        }
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lines_are_read_whole_and_no_longer_than_the_most() {
        let input: &[u8] = b"one\ntwo\n\nlast";
        let mut lines = Lines::new(input, 10);
        assert_eq!(lines.next().await.unwrap().unwrap(), b"one");
        assert_eq!(lines.next().await.unwrap().unwrap(), b"two");
        assert_eq!(lines.next().await.unwrap().unwrap(), b"");
        assert_eq!(lines.next().await.unwrap().unwrap(), b"last");
        assert_eq!(lines.next().await.unwrap(), None);
        let long: &[u8] = b"0123456789\n0123456789A\n";
        let mut lines = Lines::new(long, 10);
        assert_eq!(lines.next().await.unwrap().unwrap(), b"0123456789");
        let error = lines.next().await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn the_log_keeps_its_end() {
        let mut log = Log::default();
        log.push(&vec![b'a'; MOST_LOG]);
        log.push(b"\nstarting\nfailed: token ghp_abcdefghijklmnopqrstuvwxyz0123 refused\n\n");
        assert_eq!(log.bytes.len(), MOST_LOG);
        assert!(log.text().ends_with("refused\n\n"));
        assert_eq!(
            log.last_line().as_deref(),
            Some("failed: token [hidden by Muster] refused")
        );
        assert_eq!(Log::default().last_line(), None);
    }

    #[test]
    fn the_log_never_shows_its_plugin_s_secrets() {
        let mut log = Log::default();
        log.hide([Token::new("hunter2-xyz")]);
        log.push(b"signed in with hunter2-xyz\nusing hunter2-xyz again\n");
        assert!(!log.text().contains("hunter2"), "{}", log.text());
        assert_eq!(
            log.last_line().as_deref(),
            Some("using [hidden by Muster] again")
        );
    }
}
