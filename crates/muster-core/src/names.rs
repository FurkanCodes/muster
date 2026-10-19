//! What a chat is called (`PRODUCT.md` §6): the first line of the prompt
//! that began it, its credentials hidden (`PRD-06`), short enough for a
//! line in the list; or the name the person gave it.

use crate::credentials;
use crate::text::one_line;

/// The protocol version that names chats by their first prompt and lets
/// the person rename them.
pub const NAMES_PROTOCOL: u32 = 34;

/// The most characters a name taken from a prompt keeps.
pub const FROM_PROMPT: usize = 60;

/// The most characters a name the person gives keeps.
pub const MOST: usize = 80;

/// A chat's name from the prompt that began it: its first line with
/// anything in it, credentials hidden, cut at a word to [`FROM_PROMPT`]
/// characters; `None` for a prompt with nothing to say.
pub fn from_prompt(prompt: &str) -> Option<String> {
    let (hidden, _) = credentials::hide(prompt);
    let line = hidden.lines().map(words).find(|line| !line.is_empty())?;
    if line.chars().count() <= FROM_PROMPT {
        return Some(line);
    }
    let cut: String = line.chars().take(FROM_PROMPT - 1).collect();
    // Whole words: back to the last space, unless the cut falls between
    // words already, or backing off would leave too little.
    let between_words = line.chars().nth(FROM_PROMPT - 1) == Some(' ');
    let at_word = if between_words {
        cut.as_str()
    } else {
        cut.rfind(' ')
            .filter(|at| *at >= FROM_PROMPT / 2)
            .map_or(cut.as_str(), |at| &cut[..at])
    };
    Some(format!("{}…", at_word.trim_end()))
}

/// A name the person gave: on one line, its spaces tidied, credentials
/// hidden, and no longer than [`MOST`] characters; `None` where nothing is
/// left.
pub fn given(name: &str) -> Option<String> {
    let (hidden, _) = credentials::hide(name);
    let name: String = words(&hidden).chars().take(MOST).collect();
    let name = name.trim_end().to_owned();
    (!name.is_empty()).then_some(name)
}

/// `text` on one line, its runs of spaces made one.
fn words(text: &str) -> String {
    one_line(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_names_its_chat_by_its_first_line_with_words() {
        assert_eq!(
            from_prompt("\n  Fix the login   redirect\nIt lands on /404.").as_deref(),
            Some("Fix the login redirect")
        );
        assert_eq!(from_prompt(" \n\t\n"), None);
    }

    #[test]
    fn a_long_prompt_is_cut_at_a_word() {
        let named = from_prompt(
            "Refactor the session store so that every write goes through one transaction and \
             nothing else",
        )
        .unwrap();
        assert_eq!(
            named,
            "Refactor the session store so that every write goes through…"
        );
        assert!(named.chars().count() <= FROM_PROMPT);
        // A cut falling inside a word goes back to the word before.
        let named = from_prompt(
            "Refactor the session store so that every write goes throughout the daemon",
        )
        .unwrap();
        assert_eq!(
            named,
            "Refactor the session store so that every write goes…"
        );
        // One long word is cut where it must be.
        let named = from_prompt(&"x".repeat(100)).unwrap();
        assert_eq!(named.chars().count(), FROM_PROMPT);
    }

    #[test]
    fn credentials_never_name_a_chat() {
        let token = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0".repeat(2));
        let named = from_prompt(&format!("use {token} to push")).unwrap();
        assert!(!named.contains(&token), "{named}");
        assert!(named.contains(credentials::HIDDEN), "{named}");
        assert!(
            !given(&format!("push with {token}"))
                .unwrap()
                .contains(&token)
        );
    }

    #[test]
    fn a_given_name_is_one_tidy_line_of_a_bounded_length() {
        assert_eq!(given("  Login\n fix ").as_deref(), Some("Login fix"));
        assert_eq!(given(" \n "), None);
        assert_eq!(given(&"y".repeat(200)).unwrap().chars().count(), MOST);
    }
}
