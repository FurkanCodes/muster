use std::path::PathBuf;
use std::time::Duration;

use super::*;

const NAMES: (&str, &str) = ("muster.toml", "local overrides");

fn parse(repository: &str) -> Result<ProjectConfig, ConfigError> {
    ProjectConfig::parse(Some(repository), None, NAMES)
}

fn message(repository: &str) -> String {
    parse(repository).unwrap_err().to_string()
}

/// The example in ARCHITECTURE.md §12.
const LAMPHAUS: &str = r#"
[project]
base_branch = "main"
dockerfile = ".muster/Dockerfile"
memory = "6g"
cpus = 4

[checks]
commands = ["./gradlew --no-daemon testDebugUnitTest", "./gradlew --no-daemon lint"]
timeout = "30m"

[agents]
builder = "claude"
reviewer = "claude"        # a different harness from M4
instructions = ["AGENTS.md", "RULES.md"]

[limits]
attempts = 3
run_timeout = "90m"
max_cost_usd = 10

[intake]
label = "muster"
allowed_accounts = ["furkan"]
"#;

#[test]
fn the_documented_example_reads_as_written() {
    let config = parse(LAMPHAUS).unwrap();
    assert_eq!(config.base_branch.as_deref(), Some("main"));
    assert_eq!(config.dockerfile, Some(PathBuf::from(".muster/Dockerfile")));
    assert_eq!(config.memory_bytes, 6 * 1024 * 1024 * 1024);
    assert_eq!(config.millicpus, 4_000);
    assert_eq!(config.checks.commands.len(), 2);
    assert_eq!(config.checks.timeout, Duration::from_secs(30 * 60));
    assert_eq!(
        config.agents.instructions,
        [PathBuf::from("AGENTS.md"), PathBuf::from("RULES.md")]
    );
    assert_eq!(config.limits.attempts, 3);
    assert_eq!(config.limits.run_timeout, Duration::from_secs(90 * 60));
    assert_eq!(config.limits.max_cost_usd, Some(10.0));
    assert_eq!(config.intake.label.as_deref(), Some("muster"));
    assert_eq!(config.intake.allowed_accounts, ["furkan"]);
    assert!(config.ready_for_runs().is_ok());
}

#[test]
fn nothing_written_means_the_defaults_and_no_runs() {
    for config in [
        ProjectConfig::parse(None, None, NAMES).unwrap(),
        parse("").unwrap(),
    ] {
        assert_eq!(config, ProjectConfig::default());
        assert!(
            config
                .ready_for_runs()
                .unwrap_err()
                .contains("runs need checks")
        );
    }
}

#[test]
fn local_overrides_win_key_by_key() {
    let local = r#"
[project]
memory = "8g"

[checks]
timeout = "1h"
"#;
    let config = ProjectConfig::parse(Some(LAMPHAUS), Some(local), NAMES).unwrap();
    assert_eq!(config.memory_bytes, 8 * 1024 * 1024 * 1024);
    assert_eq!(config.checks.timeout, Duration::from_secs(3600));
    // What the override does not mention stays.
    assert_eq!(config.millicpus, 4_000);
    assert_eq!(config.checks.commands.len(), 2);
    // A list is replaced, not appended to.
    let config = ProjectConfig::parse(
        Some(LAMPHAUS),
        Some("[checks]\ncommands = [\"make test\"]"),
        NAMES,
    )
    .unwrap();
    assert_eq!(config.checks.commands, ["make test"]);
}

#[test]
fn a_misspelt_key_is_an_error_not_ignored() {
    let error = message("[checks]\ncomands = [\"make test\"]");
    assert!(error.contains("muster.toml"), "{error}");
    assert!(error.contains("comands"), "{error}");
    assert!(message("[projet]\nmemory = \"6g\"").contains("projet"));
}

#[test]
fn errors_name_the_file_they_are_in() {
    let error = ProjectConfig::parse(Some(LAMPHAUS), Some("[limits]\nattempts = 0"), NAMES)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("muster.toml with local overrides"),
        "{error}"
    );
    let syntax = ProjectConfig::parse(None, Some("[checks"), NAMES)
        .unwrap_err()
        .to_string();
    assert!(
        syntax.starts_with("local overrides is not valid TOML"),
        "{syntax}"
    );
}

#[test]
fn bad_values_are_explained() {
    for (toml, wanted) in [
        ("[project]\nmemory = \"lots\"", "not a size"),
        ("[project]\nmemory = \"100m\"", "below the 512m"),
        ("[project]\ncpus = 0", "from 0.1 to 256"),
        ("[project]\ncpus = \"four\"", "from 0.1 to 256"),
        ("[project]\nbase_branch = \" \"", "must name a branch"),
        (
            "[project]\ndockerfile = \"../outside/Dockerfile\"",
            "inside the repository",
        ),
        (
            "[project]\ndockerfile = \"/etc/Dockerfile\"",
            "inside the repository",
        ),
        ("[checks]\ncommands = [\"make\", \"  \"]", "empty command"),
        ("[checks]\ntimeout = \"soon\"", "not a duration"),
        ("[checks]\ntimeout = \"0m\"", "not a duration"),
        ("[agents]\nbuilder = \"\"", "must name a harness"),
        (
            "[agents]\ninstructions = [\"../../.ssh/id_rsa\"]",
            "inside the repository",
        ),
        ("[limits]\nattempts = 0", "from 1 to 20"),
        ("[limits]\nattempts = 99", "from 1 to 20"),
        ("[limits]\nmax_cost_usd = -1", "positive number"),
        ("[intake]\nlabel = \"\"", "must not be empty"),
        (
            "[intake]\nlabel = \"muster\"",
            "say whose label starts a run",
        ),
        ("[checks]\ncommands = \"make test\"", "invalid type"),
    ] {
        let error = message(toml);
        assert!(error.contains(wanted), "{toml:?} gave {error:?}");
    }
}

#[test]
fn sizes_and_durations_read_as_people_write_them() {
    assert_eq!(parse_size("512m"), Some(512 << 20));
    assert_eq!(parse_size("1.5g"), Some(1_610_612_736));
    assert_eq!(parse_size("6GB"), Some(6 << 30));
    assert_eq!(parse_size("1048576"), Some(1 << 20));
    for bad in ["", "g", "-1g", "1x", "nan"] {
        assert_eq!(parse_size(bad), None, "{bad:?}");
    }
    assert_eq!(parse_duration("90m"), Some(Duration::from_secs(5400)));
    assert_eq!(parse_duration("1h30m"), Some(Duration::from_secs(5400)));
    assert_eq!(parse_duration("45s"), Some(Duration::from_secs(45)));
    for bad in ["", "90", "m", "1.5h", "1d", "h1"] {
        assert_eq!(parse_duration(bad), None, "{bad:?}");
    }
}
