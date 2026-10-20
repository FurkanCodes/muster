//! The lines appended to a file since it was last read, as agents append
//! to their records while they work.
//!
//! A record can be long (a transcript of a day's work runs past 100 MB),
//! so it is read a chunk at a time and each whole line handed on as it is
//! found: what is held is a chunk and the line still being read, never the
//! file (`ARC-14`).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// How much is read at a time, and the most room a line read and let go
/// leaves behind.
const CHUNK: usize = 64 * 1024;

/// A file read up to `offset`; a last line still being written waits in
/// `partial` for the rest.
#[derive(Debug)]
pub(super) struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
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

    /// Hands `each` every whole line added since the last read, blank ones
    /// left out. Returns whether the file is shorter than what was read of
    /// it, so it was written anew and its lines start from its beginning.
    /// A file that is gone reads as nothing added.
    pub(super) fn read(&mut self, mut each: impl FnMut(&str)) -> std::io::Result<bool> {
        let mut file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let length = file.metadata()?.len();
        let restarted = length < self.offset;
        if restarted {
            self.offset = 0;
            self.partial.clear();
        }
        if length == self.offset {
            return Ok(restarted);
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut added = file.take(length - self.offset);
        let mut chunk = vec![0; CHUNK];
        loop {
            let read = match added.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            self.offset += read as u64;
            let held = self.partial.len();
            self.partial.extend_from_slice(&chunk[..read]);
            // Only the chunk just read can end a line: what was held has none.
            if let Some(last) = chunk[..read].iter().rposition(|byte| *byte == b'\n') {
                let whole = held + last + 1;
                for line in self.partial[..whole].split(|byte| *byte == b'\n') {
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    let line = String::from_utf8_lossy(line);
                    if !line.trim().is_empty() {
                        each(&line);
                    }
                }
                self.partial.drain(..whole);
            }
        }
        // A long line, read and let go, leaves no room behind it.
        self.partial.shrink_to(CHUNK);
        Ok(restarted)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn lines(tail: &mut Tail) -> (bool, Vec<String>) {
        let mut found = Vec::new();
        let restarted = tail.read(|line| found.push(line.to_owned())).unwrap();
        (restarted, found)
    }

    #[test]
    fn only_whole_new_lines_are_read_and_a_rewritten_file_from_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.jsonl");
        let mut tail = Tail::new(path.clone());
        assert_eq!(lines(&mut tail), (false, Vec::new()), "not there yet");
        std::fs::write(&path, "one\r\ntwo\n\nthr").unwrap();
        assert_eq!(lines(&mut tail).1, ["one", "two"]);
        assert!(lines(&mut tail).1.is_empty());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"ee\nfour\n").unwrap();
        assert_eq!(lines(&mut tail).1, ["three", "four"]);
        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(lines(&mut tail), (true, vec!["new".to_owned()]));
    }

    #[test]
    fn a_long_record_is_read_whole_but_never_held_whole_arc_14() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.jsonl");
        // Lines across many chunks, and one longer than a chunk.
        let short = "x".repeat(1000);
        let long = "y".repeat(CHUNK * 3 + 17);
        let mut text = String::new();
        for _ in 0..500 {
            text.push_str(&short);
            text.push('\n');
        }
        text.push_str(&long);
        text.push('\n');
        std::fs::write(&path, &text).unwrap();
        let mut tail = Tail::new(path);
        let (mut count, mut longest) = (0, 0);
        tail.read(|line| {
            count += 1;
            longest = longest.max(line.len());
        })
        .unwrap();
        assert_eq!((count, longest), (501, long.len()));
        assert!(
            tail.partial.capacity() <= CHUNK,
            "{}",
            tail.partial.capacity()
        );
    }
}
