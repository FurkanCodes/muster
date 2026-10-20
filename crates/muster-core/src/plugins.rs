//! Plugins (`ARCHITECTURE.md` §7a, `PRD-15`): a folder with a
//! `muster-plugin.toml` and programs the daemon runs out of process
//! (`ARC-23`). Here is what is pure about them: reading a manifest
//! ([`manifest`]), filling in a panel's placeholders ([`placeholders`]),
//! and the registry of what the person installed, as Muster's own settings
//! hold it ([`registry`]). The daemon fetches, hashes and moves the files.

pub mod manifest;
pub mod placeholders;
pub mod registry;

pub use manifest::{Manifest, ManifestError, Panel, PanelOn, TicketSetting, TicketSource};
pub use placeholders::{Context, Missing, Placeholder};
pub use registry::{PluginEntry, PluginSource, RegistryError};

/// The first protocol whose daemon keeps a registry of plugins and takes
/// [`crate::protocol::Command::Plugin`] (`ARC-09`).
pub const PLUGINS_PROTOCOL: u32 = 41;

/// The Muster reading manifests, which a manifest's `muster` must not be
/// newer than.
pub const MUSTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The longest id a plugin or one of its panels has.
pub const ID_MAX: usize = 32;

/// Ids kept for Muster's own plugins, besides `muster` and those starting
/// `muster-`.
pub const RESERVED: [&str; 4] = ["github", "gitlab", "jira", "linear"];

/// Whether `text` is a plugin's or a panel's id: 1 to [`ID_MAX`] of `a-z`,
/// `0-9` and `-`, starting with a letter or digit, so it is safe as a
/// folder's name and never reads as an option on a command line.
pub fn is_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= ID_MAX
        && !text.starts_with('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether `id` is kept for Muster's own plugins.
pub fn is_reserved(id: &str) -> bool {
    RESERVED.contains(&id) || id == "muster" || id.starts_with("muster-")
}

/// A version as `0.4.0` or `0.4` writes it, for comparing; `None` for
/// anything else.
pub fn version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().split('.');
    let number = |part: Option<&str>| {
        part.filter(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
            .and_then(|part| part.parse::<u64>().ok())
    };
    let major = number(parts.next())?;
    let minor = number(parts.next())?;
    let patch = match parts.next() {
        None => 0,
        part => number(part)?,
    };
    parts.next().is_none().then_some((major, minor, patch))
}

/// Whether `text` is a git commit's id: 40 hexadecimal digits, or 64 in a
/// repository of SHA-256 objects, in lowercase as git prints them.
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Whether `text` is a SHA-256 in lowercase hexadecimal, as an approval
/// is written.
pub fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Whether `text` can name a branch or tag to fetch: not empty, at most
/// 200 characters, none of them a space, a control or one git refuses in
/// a ref, not starting with `-` (an option to git) and without `..`; git
/// refuses the rest it would not take.
pub fn is_git_ref(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 200
        && !text.starts_with('-')
        && !text.starts_with('/')
        && !text.ends_with('/')
        && !text.contains("..")
        && !text.contains("@{")
        && !text.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
}

/// Why a git repository's address is not one Muster fetches a plugin
/// from.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    #[error(
        "give a git repository's address (https://, ssh://, user@host:path or file://) or its folder"
    )]
    Empty,
    #[error(
        "{scheme}:// is refused: anyone between could change what is fetched; use https:// or ssh://"
    )]
    Unprotected { scheme: String },
    #[error(
        "the address holds a password, which Muster would keep in its settings; let git's \
         credential helper hold it instead"
    )]
    Password,
    #[error("the address must not start with -, or git would read it as an option")]
    Option,
    #[error("the address holds a control character")]
    Control,
    #[error(
        "{scheme} is not a way Muster fetches a plugin: use https://, ssh://, user@host:path or \
         file://"
    )]
    Scheme { scheme: String },
}

/// Checks `address`, a git repository's: `https://`, `ssh://`, `file://`,
/// `user@host:path`, or a folder's absolute path. Plain `http://` and
/// `git://` are refused, as anyone between could change what is fetched,
/// and so is one holding a password (`ARC-20`).
pub fn check_address(address: &str) -> Result<(), AddressError> {
    if address.trim().is_empty() {
        return Err(AddressError::Empty);
    }
    if address.starts_with('-') {
        return Err(AddressError::Option);
    }
    if address.chars().any(char::is_control) {
        return Err(AddressError::Control);
    }
    if address.starts_with('/') {
        return Ok(());
    }
    // `<transport>::<address>` hands the address to a program of git's,
    // `ext::` to a shell's command line.
    if let Some((transport, _)) = address.split_once("::") {
        return Err(AddressError::Scheme {
            scheme: format!("{transport}::"),
        });
    }
    if let Some((scheme, rest)) = address.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        return match scheme.as_str() {
            "http" | "git" => Err(AddressError::Unprotected { scheme }),
            "https" | "ssh" => {
                let authority = rest.split('/').next().unwrap_or_default();
                match authority.rsplit_once('@') {
                    Some((user, _)) if user.contains(':') => Err(AddressError::Password),
                    _ => Ok(()),
                }
            }
            "file" => Ok(()),
            _ => Err(AddressError::Scheme {
                scheme: format!("{scheme}://"),
            }),
        };
    }
    // git's own short form for ssh: `user@host:path`, or `host:path`.
    match address.split_once(':') {
        Some((host, path))
            if !host.is_empty() && !host.contains(['/', ' ']) && !path.is_empty() =>
        {
            Ok(())
        }
        _ => Err(AddressError::Empty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_short_lowercase_and_never_an_option() {
        for good in ["zoetrope", "a", "my-panel", "2d", "x".repeat(32).as_str()] {
            assert!(is_id(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            "Zoe",
            "my_panel",
            "a.b",
            "a/b",
            "x".repeat(33).as_str(),
        ] {
            assert!(!is_id(bad), "{bad}");
        }
    }

    #[test]
    fn muster_s_own_ids_are_kept_for_it() {
        for kept in ["github", "gitlab", "jira", "linear", "muster", "muster-x"] {
            assert!(is_reserved(kept), "{kept}");
        }
        for free in ["youtrack", "musterd", "my-muster"] {
            assert!(!is_reserved(free), "{free}");
        }
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert_eq!(version("0.4.0"), Some((0, 4, 0)));
        assert_eq!(version("0.4"), Some((0, 4, 0)));
        assert_eq!(version(" 1.10.2 "), Some((1, 10, 2)));
        assert!(version("0.10.0") > version("0.9.9"));
        for bad in ["", "1", "1.x", "1.2.3.4", "v1.2", "1..2", "1.2-beta"] {
            assert_eq!(version(bad), None, "{bad}");
        }
        assert!(version(MUSTER_VERSION).is_some());
    }

    #[test]
    fn commits_and_approvals_are_lowercase_hexadecimal() {
        assert!(is_commit(&"a1".repeat(20)));
        assert!(is_commit(&"0f".repeat(32)));
        assert!(!is_commit(&"A1".repeat(20)));
        assert!(!is_commit("a1b2c3d"));
        assert!(is_sha256(&"9b".repeat(32)));
        assert!(!is_sha256(&"9b".repeat(20)));
        assert!(!is_sha256(&"zz".repeat(32)));
    }

    #[test]
    fn a_ref_is_one_git_would_take_and_never_an_option() {
        for good in ["main", "v0.1", "release/2026-10", "feature/x_y"] {
            assert!(is_git_ref(good), "{good}");
        }
        for bad in [
            "", "-x", "a b", "a..b", "a:b", "a~1", "x^", "/a", "a/", "a\nb", "@{-1}",
        ] {
            assert!(!is_git_ref(bad), "{bad}");
        }
    }

    #[test]
    fn an_address_is_fetched_only_over_a_protected_way_and_without_a_password() {
        for good in [
            "https://github.com/someone/zoetrope",
            "ssh://git@host/x.git",
            "git@github.com:someone/zoetrope.git",
            "file:///tmp/plugin",
            "/Users/me/plugin",
            "https://someone@host/x",
        ] {
            assert_eq!(check_address(good), Ok(()), "{good}");
        }
        assert!(matches!(
            check_address("http://host/x"),
            Err(AddressError::Unprotected { .. })
        ));
        assert!(matches!(
            check_address("git://host/x"),
            Err(AddressError::Unprotected { .. })
        ));
        assert_eq!(
            check_address("https://me:hunter2@host/x"),
            Err(AddressError::Password)
        );
        assert_eq!(check_address("--upload-pack=x"), Err(AddressError::Option));
        assert_eq!(check_address("https://h/x\n"), Err(AddressError::Control));
        assert_eq!(
            check_address("ext::sh -c x"),
            Err(AddressError::Scheme {
                scheme: "ext::".to_owned()
            })
        );
        assert!(matches!(
            check_address("ftp://host/x"),
            Err(AddressError::Scheme { .. })
        ));
        assert_eq!(check_address("relative/path"), Err(AddressError::Empty));
        assert_eq!(check_address(""), Err(AddressError::Empty));
    }
}
