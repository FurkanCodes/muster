//! Terminal streams. The daemon streams a session's terminal to one
//! connection once, so the link shares that stream among those reading it:
//! the webview's terminal, and the screen of a session asking the person,
//! read for its question (`questions`). Each reader keeps its own place in
//! the session's output; the link asks the daemon from the earliest place
//! any of them holds and hands each only what it lacks, so a second reader
//! never makes the first start over.
//!
//! Output for the webview's terminal is held and handed on at most once a
//! frame (`PATCH_EVERY`), in one message however many the daemon sent
//! (`ARC-25`): the actor flushes it (`flush_output`). Anything else for
//! that terminal (a replay, an event) first hands on what is held, so the
//! order never changes.

use std::collections::{BTreeMap, HashMap, HashSet};

use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, DetachReason, ServerFrame, TermSize, TerminalModes};

use super::FrameSink;
use super::questions::Screen;
use crate::contract::{TermEvent, frame};

/// Where a reader of a stream is in the session's output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Cursor {
    /// The next byte it lacks; `None` before it has any.
    next: Option<u64>,
    /// What it holds came from another connection, whose daemon numbers
    /// output afresh: it starts over at the next replay.
    stale: bool,
}

/// What a reader is handed of a piece of the stream.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Piece<'a> {
    /// Clear what is held, then take this.
    Reset { seq: u64, data: &'a [u8] },
    /// The first bytes, or more after those held.
    More { seq: u64, data: &'a [u8] },
    /// Nothing it lacks.
    Nothing,
}

impl Cursor {
    /// A reader that holds the bytes before `next`, if it says so.
    pub(super) fn at(next: Option<u64>) -> Self {
        Self { next, stale: false }
    }

    /// A reader whose bytes came from a connection gone since.
    pub(super) fn go_stale(&mut self) {
        self.stale = self.next.is_some();
    }

    /// Whether it holds a place this connection's daemon can stream from.
    fn place(self) -> Option<u64> {
        self.next.filter(|_| !self.stale)
    }

    /// A replay: the stream (re)starts at `seq`. `reset` says the daemon no
    /// longer holds what came before, or a new program started; but where
    /// the link itself asked for everything held (`asked`), a replay that
    /// still covers the reader's place continues it.
    pub(super) fn replay<'a>(
        &mut self,
        seq: u64,
        data: &'a [u8],
        reset: bool,
        asked: bool,
    ) -> Piece<'a> {
        let end = seq + data.len() as u64;
        let piece = match self.next {
            None => Piece::More { seq, data },
            Some(_) if self.stale => Piece::Reset { seq, data },
            Some(next) if seq <= next && next <= end && (!reset || asked) => {
                match usize::try_from(next - seq) {
                    Ok(skip) if skip < data.len() => Piece::More {
                        seq: next,
                        data: &data[skip..],
                    },
                    _ => Piece::Nothing,
                }
            }
            Some(_) => Piece::Reset { seq, data },
        };
        self.next = Some(end);
        self.stale = false;
        piece
    }

    /// Output that follows the replay; `seq` is the place of its first
    /// byte.
    pub(super) fn output<'a>(&mut self, seq: u64, data: &'a [u8]) -> Piece<'a> {
        let end = seq + data.len() as u64;
        match self.next {
            // Before its replay, or waiting for the one that starts it over:
            // output in flight from an earlier attach.
            None => Piece::Nothing,
            Some(_) if self.stale => Piece::Nothing,
            Some(next) if end <= next => Piece::Nothing,
            Some(next) => {
                self.next = Some(end);
                match usize::try_from(next.saturating_sub(seq)) {
                    Ok(skip) if seq <= next => Piece::More {
                        seq: next,
                        data: &data[skip..],
                    },
                    // A gap the daemon never makes (every gap is a reset):
                    // handed on as it came.
                    _ => Piece::More { seq, data },
                }
            }
        }
    }
}

/// The webview's terminal of a session.
struct View {
    sink: Box<dyn FrameSink>,
    cursor: Cursor,
    /// Whether the person looks at it (`ClientFrame::Watch`).
    watching: bool,
    /// Output not yet handed on, and the place of its first byte.
    held: Vec<u8>,
    held_seq: u64,
}

/// One session's stream and who reads it.
#[derive(Default)]
struct Stream {
    view: Option<View>,
    question: Option<Screen>,
    /// The daemon streams it to this connection.
    attached: bool,
    /// An attach was sent whose replay has not come.
    asked: bool,
}

impl Stream {
    fn is_read(&self) -> bool {
        self.view.is_some() || self.question.is_some()
    }

    fn watching(&self) -> bool {
        self.view.as_ref().is_some_and(|view| view.watching)
    }

    /// Where to stream from: the earliest place a reader holds, or all the
    /// daemon holds when a reader holds none.
    fn earliest_place(&self) -> Option<u64> {
        let places = [
            self.view.as_ref().map(|view| view.cursor),
            self.question.as_ref().map(Screen::cursor),
        ];
        let mut from = None;
        for cursor in places.into_iter().flatten() {
            let place = cursor.place()?;
            from = Some(from.map_or(place, |earliest: u64| earliest.min(place)));
        }
        from
    }
}

/// Every stream the link reads, and the frames it has for the daemon.
#[derive(Default)]
pub(super) struct Streams {
    /// Each session's stream, while anything reads it.
    held: HashMap<SessionId, Stream>,
    /// The connection, by number, while there is one.
    connection: Option<u64>,
    /// The connection whose bytes each session's webview terminal holds,
    /// so a screen kept from another daemon starts over (entries leave with
    /// their sessions, `prune`).
    fed_by: HashMap<SessionId, u64>,
    /// The size the webview last gave each session's terminal, which its
    /// program draws for: a question's screen is read at it.
    sizes: HashMap<SessionId, TermSize>,
    /// Frames for the daemon, in order (`drain`).
    outgoing: Vec<ClientFrame>,
    /// A question's lines may have changed since `questions` was last read.
    changed: bool,
}

impl Streams {
    /// The frames for the daemon since the last call.
    pub(super) fn drain(&mut self) -> Vec<ClientFrame> {
        std::mem::take(&mut self.outgoing)
    }

    /// Whether a question's lines may have changed, cleared as it is read.
    pub(super) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// A new connection, numbered `connection`. Streams are attached again
    /// once the state says their sessions run (`settle`).
    pub(super) fn connected(&mut self, connection: u64) {
        self.connection = Some(connection);
    }

    /// The connection was lost: nothing is attached, and what every reader
    /// holds was numbered by a daemon that may not be the next one.
    pub(super) fn disconnected(&mut self) {
        self.connection = None;
        self.outgoing.clear();
        for stream in self.held.values_mut() {
            stream.attached = false;
            stream.asked = false;
            if let Some(view) = &mut stream.view {
                view.cursor.go_stale();
            }
            if let Some(question) = &mut stream.question {
                question.go_stale();
            }
        }
    }

    /// Asks the daemon for `session`'s stream, from where its readers are.
    fn attach(&mut self, session: SessionId) {
        if self.connection.is_none() {
            return;
        }
        let Some(stream) = self.held.get_mut(&session) else {
            return;
        };
        stream.attached = true;
        stream.asked = true;
        self.outgoing.push(ClientFrame::Attach {
            session,
            from_seq: stream.earliest_place(),
            watching: stream.watching(),
        });
    }

    /// Lets the daemon stop streaming `session` once nobody reads it.
    fn let_go_if_unread(&mut self, session: SessionId) {
        let Some(stream) = self.held.get(&session) else {
            return;
        };
        if stream.is_read() {
            return;
        }
        if stream.attached && self.connection.is_some() {
            self.outgoing.push(ClientFrame::Detach { session });
        }
        self.held.remove(&session);
    }

    /// The webview streams `session`'s terminal to `sink`, its screen
    /// holding the bytes before `from_seq`, if it says so.
    pub(super) fn view_attach(
        &mut self,
        session: SessionId,
        from_seq: Option<u64>,
        watching: bool,
        sink: Box<dyn FrameSink>,
    ) {
        let mut cursor = Cursor::at(from_seq);
        // Bytes fed by another connection's daemon mean nothing to this one.
        if self.connection.is_none() || self.fed_by.get(&session) != self.connection.as_ref() {
            cursor.go_stale();
        }
        let stream = self.held.entry(session).or_default();
        stream.view = Some(View {
            sink,
            cursor,
            watching,
            held: Vec::new(),
            held_seq: 0,
        });
        self.attach(session);
    }

    /// The webview no longer streams `session`'s terminal.
    pub(super) fn view_detach(&mut self, session: SessionId) {
        let Some(stream) = self.held.get_mut(&session) else {
            return;
        };
        if stream.view.take().is_none() {
            return;
        }
        if stream.question.is_some() && stream.attached && self.connection.is_some() {
            // Its question is still read, by nobody looking.
            self.outgoing.push(ClientFrame::Watch {
                session,
                watching: false,
            });
        }
        self.let_go_if_unread(session);
    }

    /// The webview's every terminal goes: it was loaded again.
    pub(super) fn drop_views(&mut self) {
        let viewed: Vec<SessionId> = self
            .held
            .iter()
            .filter(|(_, stream)| stream.view.is_some())
            .map(|(session, _)| *session)
            .collect();
        for session in viewed {
            self.view_detach(session);
        }
    }

    /// Whether the person looks at `session`'s terminal in the webview.
    pub(super) fn view_watch(&mut self, session: SessionId, watching: bool) {
        let Some(view) = self
            .held
            .get_mut(&session)
            .and_then(|stream| stream.view.as_mut())
        else {
            return;
        };
        view.watching = watching;
        if self.held.get(&session).is_some_and(|s| s.attached) && self.connection.is_some() {
            self.outgoing.push(ClientFrame::Watch { session, watching });
        }
    }

    /// The webview sized `session`'s terminal: its program draws for it.
    pub(super) fn resized(&mut self, session: SessionId, size: TermSize) {
        self.sizes.insert(session, size);
        if let Some(question) = self
            .held
            .get_mut(&session)
            .and_then(|stream| stream.question.as_mut())
        {
            question.resize(size);
            self.changed = true;
        }
    }

    /// Reads the screens of exactly the sessions in `asking` for their
    /// questions: a new one is streamed, unwatched, from all the daemon
    /// holds; one no longer asking is let go.
    pub(super) fn ask(&mut self, asking: &HashSet<SessionId>) {
        let gone: Vec<SessionId> = self
            .held
            .iter()
            .filter(|(session, stream)| stream.question.is_some() && !asking.contains(session))
            .map(|(session, _)| *session)
            .collect();
        for session in gone {
            if let Some(stream) = self.held.get_mut(&session) {
                stream.question = None;
                self.changed = true;
            }
            self.let_go_if_unread(session);
        }
        for &session in asking {
            let stream = self.held.entry(session).or_default();
            if stream.question.is_some() {
                continue;
            }
            stream.question = Some(Screen::new(self.sizes.get(&session).copied()));
            // Read from its start, whatever the webview's terminal holds.
            self.attach(session);
        }
    }

    /// Attaches again each stream still read but not streamed, whose
    /// session `runs`: after connecting, or once a session that had ended
    /// runs again.
    pub(super) fn settle(&mut self, runs: impl Fn(SessionId) -> bool) {
        if self.connection.is_none() {
            return;
        }
        let waiting: Vec<SessionId> = self
            .held
            .iter()
            .filter(|(session, stream)| !stream.attached && stream.is_read() && runs(**session))
            .map(|(session, _)| *session)
            .collect();
        for session in waiting {
            self.attach(session);
        }
    }

    /// Forgets what it keeps of sessions that are no longer (`ARC-14`).
    pub(super) fn prune(&mut self, exists: impl Fn(SessionId) -> bool) {
        self.fed_by.retain(|session, _| exists(*session));
        self.sizes.retain(|session, _| exists(*session));
    }

    /// The questions read so far: each asking session's last lines, by
    /// session; one whose screen has nothing on it is left out.
    pub(super) fn questions(&mut self) -> BTreeMap<SessionId, Vec<String>> {
        self.held
            .iter_mut()
            .filter_map(|(session, stream)| {
                let lines = stream.question.as_mut()?.lines();
                (!lines.is_empty()).then(|| (*session, lines.to_vec()))
            })
            .collect()
    }

    /// A frame of a terminal stream from the daemon.
    pub(super) fn on_frame(&mut self, frame: ServerFrame) {
        match frame {
            ServerFrame::Replay {
                session,
                seq,
                data,
                reset,
                ..
            } => self.replay(session, seq, &data, reset),
            ServerFrame::Output { session, seq, data } => self.output(session, seq, &data),
            ServerFrame::Modes { session, modes } => self.modes(session, modes),
            ServerFrame::Detached { session, reason } => self.detached(session, reason),
            _ => {}
        }
    }

    fn replay(&mut self, session: SessionId, seq: u64, data: &[u8], reset: bool) {
        self.hand_on(session);
        let Some(stream) = self.held.get_mut(&session) else {
            return;
        };
        let asked = std::mem::take(&mut stream.asked);
        if let Some(question) = &mut stream.question {
            self.changed |= question.replay(seq, data, reset, asked);
        }
        let Some(view) = &mut stream.view else {
            return;
        };
        let sent = match view.cursor.replay(seq, data, reset, asked) {
            Piece::Reset { seq, data } => {
                view.sink
                    .send(frame::encode(frame::REPLAY_RESET, seq, data))
            }
            Piece::More { seq, data } => view.sink.send(frame::encode(frame::REPLAY, seq, data)),
            Piece::Nothing => true,
        };
        self.fed(session, sent);
    }

    fn output(&mut self, session: SessionId, seq: u64, data: &[u8]) {
        let Some(stream) = self.held.get_mut(&session) else {
            return;
        };
        if let Some(question) = &mut stream.question {
            self.changed |= question.output(seq, data);
        }
        let Some(view) = &mut stream.view else {
            return;
        };
        match view.cursor.output(seq, data) {
            Piece::More { seq, data } => {
                // Held with what came before it, if it follows on.
                if view.held.is_empty() || view.held_seq + view.held.len() as u64 == seq {
                    if view.held.is_empty() {
                        view.held_seq = seq;
                    }
                    view.held.extend_from_slice(data);
                } else {
                    let (before, at) = (std::mem::take(&mut view.held), view.held_seq);
                    view.held_seq = seq;
                    view.held.extend_from_slice(data);
                    let sent = view.sink.send(frame::encode(frame::OUTPUT, at, &before));
                    self.fed(session, sent);
                }
            }
            Piece::Reset { seq, data } => {
                let reset = frame::encode(frame::REPLAY_RESET, seq, data);
                view.held.clear();
                let sent = view.sink.send(reset);
                self.fed(session, sent);
            }
            Piece::Nothing => {}
        }
    }

    /// Whether any terminal has output held for the webview.
    pub(super) fn holds_output(&self) -> bool {
        self.held.values().any(|stream| {
            stream
                .view
                .as_ref()
                .is_some_and(|view| !view.held.is_empty())
        })
    }

    /// Hands every terminal's held output to the webview, one message each.
    pub(super) fn flush_output(&mut self) {
        let holding: Vec<SessionId> = self
            .held
            .iter()
            .filter(|(_, stream)| {
                stream
                    .view
                    .as_ref()
                    .is_some_and(|view| !view.held.is_empty())
            })
            .map(|(session, _)| *session)
            .collect();
        for session in holding {
            self.hand_on(session);
        }
    }

    /// Hands `session`'s held output to the webview, if it holds any.
    fn hand_on(&mut self, session: SessionId) {
        let Some(view) = self
            .held
            .get_mut(&session)
            .and_then(|stream| stream.view.as_mut())
        else {
            return;
        };
        if view.held.is_empty() {
            return;
        }
        let held = std::mem::take(&mut view.held);
        let sent = view
            .sink
            .send(frame::encode(frame::OUTPUT, view.held_seq, &held));
        self.fed(session, sent);
    }

    /// The webview's terminal of `session` was handed bytes; one nobody
    /// reads any more is let go.
    fn fed(&mut self, session: SessionId, sent: bool) {
        if sent {
            if let Some(connection) = self.connection {
                self.fed_by.insert(session, connection);
            }
        } else {
            self.view_detach(session);
        }
    }

    fn modes(&mut self, session: SessionId, modes: TerminalModes) {
        self.event(session, &TermEvent::Modes { modes });
    }

    fn detached(&mut self, session: SessionId, reason: DetachReason) {
        if let Some(stream) = self.held.get_mut(&session) {
            // A program that exited streams on into the next one; a session
            // holding no terminal is streamed again should it run again.
            if reason == DetachReason::NotRunning {
                stream.attached = false;
                stream.asked = false;
            }
        }
        self.event(session, &TermEvent::Ended { reason });
    }

    fn event(&mut self, session: SessionId, event: &TermEvent) {
        self.hand_on(session);
        let Some(view) = self
            .held
            .get_mut(&session)
            .and_then(|stream| stream.view.as_mut())
        else {
            return;
        };
        let Ok(json) = serde_json::to_vec(event) else {
            return;
        };
        if !view.sink.send(frame::encode(frame::EVENT, 0, &json)) {
            self.view_detach(session);
        }
    }
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
