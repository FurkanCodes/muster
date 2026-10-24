//! Typed identifiers. Every entity has its own ID type, so a session ID can
//! never be passed where a project ID is expected.
//!
//! IDs are ULIDs: unique, and printed with a short prefix (`ses_01J…`) so
//! logs say what kind of thing they name. A new one is made from the
//! system clock and its randomness, the one thing this crate takes from
//! the system (`ARC-01`); its time is only a tie-break, never a rule, as
//! two made in one millisecond come in no set order.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Why a string could not be read as an ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseIdError {
    expected_prefix: &'static str,
    input: String,
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a valid ID; expected `{}_` followed by a ULID",
            self.input, self.expected_prefix
        )
    }
}

impl std::error::Error for ParseIdError {}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
        #[serde(transparent)]
        pub struct $name(#[cfg_attr(feature = "ts", ts(type = "string"))] Ulid);

        impl $name {
            /// The prefix used when this ID is printed.
            pub const PREFIX: &'static str = $prefix;

            /// A new, unique identifier.
            #[expect(
                clippy::new_without_default,
                reason = "each new id is unique; a Default would read as a fixed value"
            )]
            pub fn new() -> Self {
                Self(Ulid::generate())
            }

            /// Wraps an existing ULID, for example one read from the store.
            pub const fn from_ulid(ulid: Ulid) -> Self {
                Self(ulid)
            }

            /// The underlying ULID.
            pub const fn ulid(self) -> Ulid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", Self::PREFIX, self.0)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let error = || ParseIdError {
                    expected_prefix: Self::PREFIX,
                    input: s.to_owned(),
                };
                let ulid = s
                    .strip_prefix(Self::PREFIX)
                    .and_then(|rest| rest.strip_prefix('_'))
                    .ok_or_else(error)?;
                Ulid::from_string(ulid).map(Self).map_err(|_| error())
            }
        }
    };
}

id_type!(
    /// A repository Muster knows about.
    ProjectId,
    "prj"
);
id_type!(
    /// Where a session's code lives: a git worktree or a container.
    WorkspaceId,
    "wsp"
);
id_type!(
    /// One agent process in one workspace.
    SessionId,
    "ses"
);
id_type!(
    /// One ticket going through the autopilot's stages.
    RunId,
    "run"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_round_trip() {
        let id = SessionId::new();
        let printed = id.to_string();
        assert!(printed.starts_with("ses_"));
        assert_eq!(printed.parse::<SessionId>(), Ok(id));
    }

    #[test]
    fn parse_rejects_another_kinds_prefix() {
        let printed = ProjectId::new().to_string();
        let error = printed.parse::<SessionId>().unwrap_err();
        assert!(error.to_string().contains("expected `ses_`"));
    }

    #[test]
    fn parse_rejects_a_missing_prefix_and_bad_ulids() {
        let bare = SessionId::new().ulid().to_string();
        assert!(bare.parse::<SessionId>().is_err());
        assert!("ses_not-a-ulid".parse::<SessionId>().is_err());
        assert!("ses".parse::<SessionId>().is_err());
    }

    #[test]
    fn serializes_as_a_plain_ulid_string() {
        let id = WorkspaceId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{}\"", id.ulid()));
        assert_eq!(serde_json::from_str::<WorkspaceId>(&json).unwrap(), id);
    }
}
