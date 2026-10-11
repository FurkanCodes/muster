//! Tickets and the work on them. An agent works on a ticket in a worktree
//! on a branch named for it, `muster/issue-<n>-<title>` (or `-2`, `-3`… for
//! further agents), so the branch says which ticket its work is for.

/// The issue a branch Muster named for one is about: `muster/issue-42`,
/// `muster/issue-42-fix-the-login`, `muster/issue-42-fix-the-login-2`.
pub fn issue_of_branch(branch: &str) -> Option<u64> {
    let rest = branch.strip_prefix("muster/issue-")?;
    let digits: &str = rest.split('-').next()?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_named_for_an_issue_names_it_and_no_other_does() {
        assert_eq!(issue_of_branch("muster/issue-42"), Some(42));
        assert_eq!(issue_of_branch("muster/issue-42-fix-the-login"), Some(42));
        assert_eq!(issue_of_branch("muster/issue-42-fix-the-login-2"), Some(42));
        assert_eq!(issue_of_branch("muster/issue-4"), Some(4));
        assert_eq!(issue_of_branch("muster/issue-x-fix"), None);
        assert_eq!(issue_of_branch("muster/issue-"), None);
        assert_eq!(issue_of_branch("feature/issue-42"), None);
        assert_eq!(issue_of_branch("muster/ab12cd"), None);
        assert_eq!(issue_of_branch("detached at 1a2b3c4"), None);
    }
}
