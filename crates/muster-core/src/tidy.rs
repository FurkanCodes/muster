//! Tidying a branch's commits (`PRODUCT.md` §6, `PRD-14`): at the
//! person's request, an agent they pick rewrites the branch's commits not
//! pushed yet into clear ones, and Muster checks that the files end as
//! they began. The daemon keeps what it asked and found on the agent's
//! session (`Session::tidy`).

use serde::{Deserialize, Serialize};

/// The protocol version that knows tidying a branch's commits.
pub const TIDY_PROTOCOL: u32 = 27;

/// A tidy an agent was asked for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Tidy {
    /// How many commits it was asked to tidy.
    pub commits: u64,
    /// The commit they follow, short.
    pub from: String,
    /// What Muster found once the agent stopped; `None` until then.
    #[serde(default)]
    pub checked: Option<TidyCheck>,
}

/// What Muster found once the agent stopped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum TidyCheck {
    /// The files are as they began, in `commits` commits after the one
    /// they follow.
    Kept { commits: u64 },
    /// The commits are as they were.
    Untouched,
    /// The files are not as they began: `files` of them differ.
    FilesChanged { files: u64 },
    /// The branch went astray otherwise: `reason` says how.
    Astray { reason: String },
}

impl Tidy {
    /// What it comes to, in a few words, for the session's line.
    pub fn words(&self) -> String {
        match &self.checked {
            None => format!("tidying {} after {}", commits(self.commits), self.from),
            Some(TidyCheck::Kept { commits: after }) => format!(
                "tidied {} into {after}, files unchanged",
                commits(self.commits)
            ),
            Some(TidyCheck::Untouched) => "left the commits as they were".to_owned(),
            Some(TidyCheck::FilesChanged { files }) => {
                let files = if *files == 1 {
                    "1 file".to_owned()
                } else {
                    format!("{files} files")
                };
                format!("the tidy changed {files}: z in d puts them back")
            }
            Some(TidyCheck::Astray { reason }) => crate::text::one_line(reason),
        }
    }

    /// Whether what it did needs the person's eye: files changed, or the
    /// branch gone astray.
    pub fn went_wrong(&self) -> bool {
        matches!(
            self.checked,
            Some(TidyCheck::FilesChanged { .. } | TidyCheck::Astray { .. })
        )
    }
}

fn commits(count: u64) -> String {
    if count == 1 {
        "1 commit".to_owned()
    } else {
        format!("{count} commits")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tidy(checked: Option<TidyCheck>) -> Tidy {
        Tidy {
            commits: 5,
            from: "a1b2c3d".to_owned(),
            checked,
        }
    }

    #[test]
    fn a_tidy_says_what_it_comes_to_and_whether_it_went_wrong() {
        let said: Vec<(String, bool)> = [
            None,
            Some(TidyCheck::Kept { commits: 2 }),
            Some(TidyCheck::Untouched),
            Some(TidyCheck::FilesChanged { files: 1 }),
            Some(TidyCheck::Astray {
                reason: "it left changes\nnot committed".to_owned(),
            }),
        ]
        .into_iter()
        .map(|checked| {
            let tidy = tidy(checked);
            (tidy.words(), tidy.went_wrong())
        })
        .collect();
        assert_eq!(
            said,
            [
                ("tidying 5 commits after a1b2c3d".to_owned(), false),
                ("tidied 5 commits into 2, files unchanged".to_owned(), false),
                ("left the commits as they were".to_owned(), false),
                (
                    "the tidy changed 1 file: z in d puts them back".to_owned(),
                    true
                ),
                ("it left changes not committed".to_owned(), true),
            ]
        );
    }

    #[test]
    fn a_check_reads_as_it_was_written() {
        let checked = TidyCheck::Kept { commits: 2 };
        let text = serde_json::to_string(&checked).unwrap();
        assert_eq!(text, r#"{"outcome":"kept","commits":2}"#);
        assert_eq!(serde_json::from_str::<TidyCheck>(&text).unwrap(), checked);
    }
}
