//! A version's numbers, compared number by number (`0.5.10` after
//! `0.5.9`), as Muster's own, a plugin's needs and a machine's Muster are
//! written. Pure (`ARC-01`).

/// `major.minor.patch`; a missing patch is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// A version as a program says it: `0.5.7`, `0.5`, or with what follows
    /// a `-` or `+` passed over (`0.3.10-dev`, `1.2.0+abc`). `None` for
    /// anything else.
    pub fn of(text: &str) -> Option<Self> {
        let text = text.trim();
        let numbers = text.split(['-', '+']).next().unwrap_or(text);
        Self::exact(numbers)
    }

    /// A version written as numbers alone, as a plugin's manifest writes
    /// what it needs: `0.4` or `0.4.0`, nothing after.
    pub fn exact(text: &str) -> Option<Self> {
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
        parts.next().is_none().then_some(Self(major, minor, patch))
    }

    /// This build's own.
    pub fn own() -> Self {
        Self::of(env!("CARGO_PKG_VERSION")).unwrap_or(Self(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert_eq!(Version::exact("0.4"), Some(Version(0, 4, 0)));
        assert_eq!(Version::exact(" 1.10.2 "), Some(Version(1, 10, 2)));
        assert!(Version::of("0.5.10") > Version::of("0.5.9"));
        assert_eq!(Version::of("0.3.10-dev"), Some(Version(0, 3, 10)));
        assert_eq!(Version::of("1.2.0-beta.3+abc"), Some(Version(1, 2, 0)));
        for bad in ["", "1", "1.x", "1.2.3.4", "v1.2", "1..2", "dev", "unknown"] {
            assert_eq!(Version::of(bad), None, "{bad}");
        }
        assert_eq!(
            Version::exact("1.2-beta"),
            None,
            "a manifest writes numbers alone"
        );
        assert_eq!(Version::own().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
