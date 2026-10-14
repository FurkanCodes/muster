//! Whether this UI is older than the daemon it talks to. `muster upgrade`
//! moves the daemon onto the new muster in place, and the UI reconnects to
//! it; but the UI itself still runs the muster it was started from until
//! it is started again.

/// This UI's own version.
const OWN: &str = env!("CARGO_PKG_VERSION");

/// What to tell the person when this UI is older than a daemon of
/// `daemon_version`; `None` when it is not.
pub(super) fn stale_warning(daemon_version: &str) -> Option<String> {
    warning(OWN, daemon_version)
}

fn warning(own: &str, daemon: &str) -> Option<String> {
    (numbers(own) < numbers(daemon)).then(|| {
        format!(
            "This window runs muster {own}; the daemon is {daemon}. Quit with q and run muster \
             again for the new one: every session keeps running."
        )
    })
}

/// A version's numbers: `0.3.10-dev` is `[0, 3, 10]`, and what is missing
/// or no number counts as 0.
fn numbers(version: &str) -> [u64; 3] {
    let mut numbers = [0; 3];
    for (number, part) in numbers.iter_mut().zip(version.trim().split('.')) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        *number = digits.parse().unwrap_or(0);
    }
    numbers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_newer_daemon_is_warned_of() {
        let warned = warning("0.3.2", "0.3.3").unwrap();
        assert!(warned.contains("runs muster 0.3.2; the daemon is 0.3.3"));
        assert!(warned.contains("q"));
        assert!(
            warning("0.3.9", "0.3.10").is_some(),
            "by number, not by text"
        );
        assert!(warning("0.9.0", "1.0.0").is_some());
        for (own, daemon) in [
            ("0.3.3", "0.3.3"),
            ("0.3.3", "0.3.2"),
            ("0.3.10", "0.3.9"),
            ("0.3.3", "0.3.3-dev"),
            // A version it cannot read is no reason to nag.
            ("0.3.3", "unknown"),
            ("0.3.3", ""),
        ] {
            assert_eq!(warning(own, daemon), None, "{own} against {daemon}");
        }
    }

    #[test]
    fn this_ui_measures_against_its_own_version() {
        assert!(stale_warning(OWN).is_none());
        assert!(stale_warning("0.0.1").is_none());
        assert!(stale_warning("999.0.0").is_some());
    }
}
