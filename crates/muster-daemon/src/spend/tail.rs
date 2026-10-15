//! The lines appended to a file since it was last read, as agents append
//! to their records while they work.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A file read up to `offset`; a last line still being written waits in
/// `partial` for the rest.
#[derive(Debug)]
pub(super) struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}

/// What a read found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Lines {
    /// The file is shorter than what was read of it, so it was written
    /// anew, and `lines` start from its beginning.
    pub(super) restarted: bool,
    pub(super) lines: Vec<String>,
}

impl Tail {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: Vec::new(),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// The whole lines added since the last read. A file that is gone
    /// reads as nothing added.
    pub(super) fn read(&mut self) -> std::io::Result<Lines> {
        let mut file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Lines::default());
            }
            Err(error) => return Err(error),
        };
        let length = file.metadata()?.len();
        let restarted = length < self.offset;
        if restarted {
            self.offset = 0;
            self.partial.clear();
        }
        if length == self.offset {
            return Ok(Lines {
                restarted,
                lines: Vec::new(),
            });
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut added = Vec::new();
        file.take(length - self.offset).read_to_end(&mut added)?;
        self.offset += added.len() as u64;
        self.partial.extend_from_slice(&added);
        let whole = self
            .partial
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        let lines = String::from_utf8_lossy(&self.partial[..whole])
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect();
        self.partial.drain(..whole);
        Ok(Lines { restarted, lines })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn only_whole_new_lines_are_read_and_a_rewritten_file_from_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.jsonl");
        let mut tail = Tail::new(path.clone());
        assert_eq!(tail.read().unwrap(), Lines::default(), "not there yet");
        std::fs::write(&path, "one\ntwo\nthr").unwrap();
        assert_eq!(tail.read().unwrap().lines, ["one", "two"]);
        assert!(tail.read().unwrap().lines.is_empty());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"ee\nfour\n").unwrap();
        assert_eq!(tail.read().unwrap().lines, ["three", "four"]);
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(
            tail.read().unwrap(),
            Lines {
                restarted: true,
                lines: vec!["new".to_owned()]
            }
        );
    }
}
