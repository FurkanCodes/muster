//! Samples of what a terminal's messages say (what a session shows, from
//! protocol 51; the command line an agent is run with in a terminal, from
//! 54, `ARC-28`), for `protocol_round_trip`.

use muster_core::protocol::Reply;

/// What a session shows, and the command line an agent is run with in a
/// terminal (`ARC-28`).
pub(crate) fn replies() -> [Reply; 2] {
    [
        Reply::SessionText {
            text: Some("Fixed it.".to_owned()),
        },
        Reply::AgentCommand {
            program: "claude".to_owned(),
            args: vec!["--session-id".to_owned(), "0f8b".to_owned()],
            env: vec![("MUSTER_AGENT".to_owned(), "claude".to_owned())],
        },
    ]
}
