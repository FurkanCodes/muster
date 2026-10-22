//! Repositories on GitHub the person can clone as a project (`PRODUCT.md`
//! §6, §6a), through their own `gh`: its sign-in reaches their private
//! repositories and their organisations', and `gh repo clone` clones over
//! the protocol (HTTPS or SSH) they chose for it. Behind a trait, as every
//! system outside the daemon is (`ARC-03`).

use std::fmt::Debug;
use std::path::Path;
use std::time::Duration;

use muster_core::protocol::repositories::HostedRepository;
use serde::Deserialize;

use crate::github::{gh_with, gh_within};
use crate::service::{BoxFuture, ServiceError};

/// What the daemon asks of GitHub to clone a project.
pub(crate) trait Repositories: Send + Sync + Debug {
    /// The repositories the person can reach, the most recently pushed first.
    fn list(&self) -> BoxFuture<'_, Result<Vec<HostedRepository>, ServiceError>>;
    /// Clones `name` (`owner/name`, checked) to `folder`, which is not there yet.
    fn clone_to<'a>(
        &'a self,
        name: &'a str,
        folder: &'a Path,
    ) -> BoxFuture<'a, Result<(), ServiceError>>;
}

/// GitHub through the person's own `gh`.
#[derive(Debug)]
pub(crate) struct Gh;

/// The person's own repositories, those they work on and their
/// organisations', the most recently pushed first: a hundred, as GitHub
/// gives at most that in one answer; any other is typed by its name.
const LISTED: &str =
    "user/repos?affiliation=owner,collaborator,organization_member&sort=pushed&per_page=100";

/// How long a clone may take: as long as the repository is big, but not for ever.
const CLONE_LIMIT: Duration = Duration::from_mins(30);

/// What a description may hold: GitHub's own limit is 350 characters.
const DESCRIPTION: usize = 350;

impl Repositories for Gh {
    fn list(&self) -> BoxFuture<'_, Result<Vec<HostedRepository>, ServiceError>> {
        Box::pin(async {
            let listed = gh_with(Path::new("/"), &["api", LISTED], None).await?;
            listing(&listed)
        })
    }

    fn clone_to<'a>(
        &'a self,
        name: &'a str,
        folder: &'a Path,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let parent = folder.parent().unwrap_or(Path::new("/"));
            let target = folder.to_string_lossy();
            // `name` is checked to be `owner/name` (`repository_name`), never an option.
            gh_within(
                parent,
                &["repo", "clone", name, &target, "--", "--quiet"],
                None,
                CLONE_LIMIT,
            )
            .await?;
            Ok(())
        })
    }
}

/// GitHub's answer, read field by field; descriptions, which anyone can
/// write, cleaned of control characters and cut to their length.
fn listing(json: &str) -> Result<Vec<HostedRepository>, ServiceError> {
    #[derive(Deserialize)]
    struct Listed {
        full_name: String,
        description: Option<String>,
        #[serde(default)]
        private: bool,
    }
    let listed: Vec<Listed> = serde_json::from_str(json).map_err(|error| {
        ServiceError::Unreadable(format!("GitHub's list of repositories: {error}"))
    })?;
    Ok(listed
        .into_iter()
        .filter_map(|repository| {
            let name = muster_core::protocol::repositories::repository_name(&repository.full_name)?;
            let description = repository
                .description
                .map(|text| muster_core::text::one_line(&text))
                .map(|text| text.chars().take(DESCRIPTION).collect::<String>())
                .filter(|text| !text.trim().is_empty());
            Some(HostedRepository {
                name,
                description,
                private: repository.private,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_s_list_is_read_as_names_cleaned_descriptions_and_privacy() {
        let json = r#"[
            {"full_name": "o/app", "description": "The app\u001b[31m red", "private": true, "id": 1},
            {"full_name": "o/lib", "description": null, "private": false},
            {"full_name": "not a name", "description": "skipped", "private": false},
            {"full_name": "o/blank", "description": "  ", "private": false}
        ]"#;
        let listed = listing(json).unwrap();
        assert_eq!(
            listed,
            [
                HostedRepository {
                    name: "o/app".to_owned(),
                    description: Some("The app[31m red".to_owned()),
                    private: true,
                },
                HostedRepository {
                    name: "o/lib".to_owned(),
                    description: None,
                    private: false,
                },
                HostedRepository {
                    name: "o/blank".to_owned(),
                    description: None,
                    private: false,
                },
            ]
        );
        assert!(matches!(listing("{}"), Err(ServiceError::Unreadable(_))));
    }
}
