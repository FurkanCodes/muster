//! A repository's git remote, read for where it is hosted: which service,
//! on which host, and the repository's path there. Pure: the daemon asks
//! git for the remote's address.

use crate::config::HostKind;

/// A remote's address, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    /// The host, lowercased, without a user or port: `github.com`.
    pub host: String,
    /// The repository's path on it, without `.git`: `owner/repo`, or
    /// `group/subgroup/repo`, or a Data Center's `scm/PROJ/repo`.
    pub path: String,
    /// Whether it was given over `https`, so the host's web address is
    /// `https://<host>` (an `ssh` remote's port says nothing about it).
    pub https: bool,
}

impl Remote {
    /// Reads `url`: `https://host/owner/repo(.git)`, with or without a
    /// user (and password, which is never kept), `ssh://user@host[:port]/
    /// owner/repo`, or scp's `user@host:owner/repo`.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (rest, https) = if let Some(rest) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
        {
            (rest, true)
        } else if let Some(rest) = url
            .strip_prefix("ssh://")
            .or_else(|| url.strip_prefix("git+ssh://"))
        {
            (rest, false)
        } else if !url.contains("://") && url.contains(':') {
            // scp style: `git@host:owner/repo`.
            let (authority, path) = url.split_once(':')?;
            return Self::from_parts(authority, path, false);
        } else {
            return None;
        };
        let (authority, path) = rest.split_once('/')?;
        Self::from_parts(authority, path, https)
    }

    fn from_parts(authority: &str, path: &str, https: bool) -> Option<Self> {
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.split(':').next()?.to_ascii_lowercase();
        let path = path.trim_matches('/');
        let path = path
            .strip_suffix(".git")
            .unwrap_or(path)
            .trim_end_matches('/');
        let valid = |part: &str| {
            !part.is_empty()
                && part != ".."
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.~".contains(c))
        };
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-.".contains(c))
        {
            return None;
        }
        if path.split('/').count() < 2 || !path.split('/').all(valid) {
            return None;
        }
        Some(Self {
            host,
            path: path.to_owned(),
            https,
        })
    }

    /// Which service it is on, by its host's name: GitHub, GitLab's own or
    /// a `gitlab.` host, Bitbucket Cloud. Any other needs saying in
    /// `[pull_requests]`.
    pub fn kind(&self) -> Option<HostKind> {
        let host = self.host.as_str();
        if host == "github.com" || host.ends_with(".github.com") || host.ends_with(".ghe.com") {
            Some(HostKind::GitHub)
        } else if host == "gitlab.com" || host.starts_with("gitlab.") {
            Some(HostKind::GitLab)
        } else if host == "bitbucket.org" {
            Some(HostKind::Bitbucket)
        } else {
            None
        }
    }

    /// Whether it is Bitbucket Cloud, rather than a Data Center.
    pub fn is_bitbucket_cloud(&self) -> bool {
        self.host == "bitbucket.org"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(url: &str) -> (String, String) {
        let remote = Remote::parse(url).unwrap_or_else(|| panic!("{url}"));
        (remote.host, remote.path)
    }

    #[test]
    fn every_form_of_remote_is_read_and_its_secrets_left_behind() {
        let expected = ("bitbucket.org".to_owned(), "ws/repo".to_owned());
        for url in [
            "https://bitbucket.org/ws/repo.git",
            "https://bitbucket.org/ws/repo",
            "https://user@bitbucket.org/ws/repo.git",
            "https://x-token-auth:secret@bitbucket.org/ws/repo.git/",
            "git@bitbucket.org:ws/repo.git",
            "ssh://git@bitbucket.org/ws/repo.git",
            "ssh://git@Bitbucket.org:22/ws/repo",
        ] {
            assert_eq!(read(url), expected, "{url}");
        }
        let remote = Remote::parse("https://x-token-auth:secret@bitbucket.org/ws/repo").unwrap();
        assert!(!format!("{remote:?}").contains("secret"));
        assert_eq!(
            read("git@gitlab.example.com:group/sub/project.git"),
            (
                "gitlab.example.com".to_owned(),
                "group/sub/project".to_owned()
            )
        );
        assert_eq!(
            read("https://bitbucket.example.com/scm/PROJ/repo.git"),
            (
                "bitbucket.example.com".to_owned(),
                "scm/PROJ/repo".to_owned()
            )
        );
        assert!(Remote::parse("https://github.com/o/r").unwrap().https);
        assert!(!Remote::parse("git@github.com:o/r").unwrap().https);
    }

    #[test]
    fn what_is_not_a_remote_is_not_read() {
        for url in [
            "",
            "/local/path/repo",
            "file:///tmp/repo",
            "https://github.com/",
            "https://github.com/only",
            "git@github.com:o/../r",
            "https://evil host/o/r",
            "https://github.com/o/r;rm",
        ] {
            assert_eq!(Remote::parse(url), None, "{url}");
        }
    }

    #[test]
    fn the_service_is_known_by_its_host() {
        let kind = |url: &str| Remote::parse(url).unwrap().kind();
        assert_eq!(kind("git@github.com:o/r.git"), Some(HostKind::GitHub));
        assert_eq!(kind("https://gitlab.com/g/r"), Some(HostKind::GitLab));
        assert_eq!(
            kind("https://gitlab.example.com/g/r"),
            Some(HostKind::GitLab)
        );
        assert_eq!(kind("git@bitbucket.org:ws/r"), Some(HostKind::Bitbucket));
        assert_eq!(kind("https://code.example.com/g/r"), None);
        assert!(
            Remote::parse("git@bitbucket.org:ws/r")
                .unwrap()
                .is_bitbucket_cloud()
        );
    }
}
