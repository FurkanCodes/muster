//! Spaces: groups of projects and terminals the person makes
//! (`PRODUCT.md` §2, `ARCHITECTURE.md` §2). A space only groups: it never
//! changes where or how anything runs.

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectId, SessionId, SpaceId};
use crate::time::Timestamp;

/// The most spaces there can be.
pub const MAX_SPACES: usize = 64;

/// The longest a space's name may be, in characters.
pub const MAX_NAME: usize = 40;

/// A group of projects and terminals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    /// What is in it, each once, in the order it was added.
    pub members: Vec<Member>,
    pub created_at: Timestamp,
}

impl Space {
    /// Whether `member` is in it.
    pub fn holds(&self, member: Member) -> bool {
        self.members.contains(&member)
    }
}

/// What a space holds: a project, or a terminal in no project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Member {
    Project(ProjectId),
    Session(SessionId),
}

/// Why a name cannot be a space's.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NameRefusal {
    #[error("a space needs a name")]
    Empty,
    #[error("a space's name is one line")]
    Lines,
    #[error("a space's name is at most {MAX_NAME} characters")]
    TooLong,
    #[error("there is a space called {0} already")]
    Taken(String),
}

/// `name` as a space's name, trimmed, where it is one: a line of at most
/// [`MAX_NAME`] characters that none of `others` has, case aside.
pub fn check_name<'a>(
    name: &str,
    others: impl IntoIterator<Item = &'a str>,
) -> Result<String, NameRefusal> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameRefusal::Empty);
    }
    if name.chars().any(char::is_control) {
        return Err(NameRefusal::Lines);
    }
    if name.chars().count() > MAX_NAME {
        return Err(NameRefusal::TooLong);
    }
    if let Some(taken) = others
        .into_iter()
        .find(|other| other.to_lowercase() == name.to_lowercase())
    {
        return Err(NameRefusal::Taken(taken.to_owned()));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_one_line_short_and_unused() {
        assert_eq!(check_name("  Work ", []), Ok("Work".to_owned()));
        assert_eq!(check_name(" ", []), Err(NameRefusal::Empty));
        assert_eq!(check_name("a\nb", []), Err(NameRefusal::Lines));
        assert_eq!(check_name(&"x".repeat(41), []), Err(NameRefusal::TooLong));
        assert!(check_name(&"é".repeat(40), []).is_ok());
        assert_eq!(
            check_name("work", ["Home", "Work"]),
            Err(NameRefusal::Taken("Work".to_owned()))
        );
    }
}
