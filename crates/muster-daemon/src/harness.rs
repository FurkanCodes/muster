//! What to run for a session (`ARCHITECTURE.md` §7).
//!
//! The harnesses: `shell`, the person's own shell in the workspace; the
//! coding agents `claude` (Claude Code), `codex` (Codex), `gemini` (Gemini
//! CLI), `opencode` (`OpenCode`) and `copilot` (GitHub Copilot CLI); and two
//! Muster runs by hand, `checks` and `review`. A harness says how to start,
//! resume and hear from its program; the daemon does the rest. Each agent
//! is heard the way it allows without editing the person's config
//! (`PRD-13`): hooks passed at launch, a plugin or settings file of Muster's
//! own that it is pointed at, or what it shows in its window title.

pub(crate) mod activity;
pub(crate) mod claude;
pub(crate) mod claude_stream;
pub(crate) mod codex;
pub(crate) mod copilot;
pub(crate) mod gemini;
pub(crate) mod opencode;
mod scrub;
pub(crate) mod support;
pub(crate) mod transcript;

use std::path::PathBuf;

use muster_core::SessionId;
use muster_core::protocol::TermSize;
use muster_state::session::HookEvent;

use crate::pty::SpawnSpec;
use crate::runs::credentials::Secret;

/// A harness Muster can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Harness {
    pub(crate) id: &'static str,
    /// The default title of its sessions.
    pub(crate) label: &'static str,
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Shell,
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Copilot,
    /// The project's checks, run by hand in a workspace.
    Checks,
    /// Claude Code reviewing a workspace's change in plan mode, which
    /// changes nothing.
    Review,
}

const SHELL: Harness = Harness {
    id: "shell",
    label: "Shell",
    kind: Kind::Shell,
};

const CLAUDE: Harness = Harness {
    id: "claude",
    label: "Claude Code",
    kind: Kind::Claude,
};

const CODEX: Harness = Harness {
    id: "codex",
    label: "Codex",
    kind: Kind::Codex,
};

const GEMINI: Harness = Harness {
    id: "gemini",
    label: "Gemini CLI",
    kind: Kind::Gemini,
};

const OPENCODE: Harness = Harness {
    id: "opencode",
    label: "OpenCode",
    kind: Kind::OpenCode,
};

const COPILOT: Harness = Harness {
    id: "copilot",
    label: "Copilot CLI",
    kind: Kind::Copilot,
};

const CHECKS: Harness = Harness {
    id: "checks",
    label: "Checks",
    kind: Kind::Checks,
};

const REVIEW: Harness = Harness {
    id: "review",
    label: "Review",
    kind: Kind::Review,
};

/// The harness called `id`, if Muster knows it.
pub(crate) fn find(id: &str) -> Option<Harness> {
    [
        SHELL, CLAUDE, CODEX, GEMINI, OPENCODE, COPILOT, CHECKS, REVIEW,
    ]
    .into_iter()
    .find(|harness| harness.id == id)
}

/// What the project's checks are, for a session that runs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckCommands {
    pub(crate) commands: Vec<String>,
    /// How long each command may take.
    pub(crate) limit: std::time::Duration,
}

/// Runs the checks: `$1` is the time limit per command in seconds, the
/// rest the commands, each in a shell of its own, stopping at the first
/// that fails. The limit needs `timeout`, which macOS lacks unless GNU's
/// coreutils are installed; without it a command runs until it ends or
/// the person stops it.
const CHECKS_BY_HAND: &str = r#"limit="$1"; shift
for command in "$@"; do
  printf '\033[1m$ %s\033[0m\n' "$command"
  if command -v timeout >/dev/null 2>&1; then
    timeout "$limit" sh -c "$command"
  elif command -v gtimeout >/dev/null 2>&1; then
    gtimeout "$limit" sh -c "$command"
  else
    sh -c "$command"
  fi
  code=$?
  if [ "$code" -ne 0 ]; then
    if [ "$code" -eq 124 ]; then echo "(stopped after ${limit}s)"; else echo "(exit $code)"; fi
    printf '\033[1;31mThe checks failed.\033[0m\n'
    exit "$code"
  fi
done
printf '\033[1;32mAll checks passed.\033[0m\n'
"#;

/// What a harness's window title says about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TitleState {
    Working,
    Ready,
    /// Waiting on the person, for what it does not say.
    Waiting,
}

impl Harness {
    /// Whether it is a coding agent, rather than a shell.
    pub(crate) fn is_agent(self) -> bool {
        matches!(
            self.kind,
            Kind::Claude
                | Kind::Review
                | Kind::Codex
                | Kind::Gemini
                | Kind::OpenCode
                | Kind::Copilot
        )
    }

    /// Whether it runs the project's checks.
    pub(crate) fn is_checks(self) -> bool {
        self.kind == Kind::Checks
    }

    /// Whether the program reports its state to the hook socket, and so
    /// needs a hook token at launch.
    pub(crate) fn has_hooks(self) -> bool {
        self.is_agent()
    }

    /// Whether Muster chooses the agent's session id at launch, as Claude
    /// Code, Gemini CLI and Copilot CLI let it. Codex and `OpenCode` choose
    /// their own, which their reports then carry.
    pub(crate) fn chooses_session_id(self) -> bool {
        matches!(
            self.kind,
            Kind::Claude | Kind::Review | Kind::Gemini | Kind::Copilot
        )
    }

    /// Whether the program says it has started soon after launch, so that
    /// silence past a grace means it is asking something first, such as
    /// whether to trust the folder. Copilot CLI says so only with its first
    /// prompt.
    pub(crate) fn reports_start(self) -> bool {
        matches!(
            self.kind,
            Kind::Claude | Kind::Review | Kind::Codex | Kind::Gemini | Kind::OpenCode
        )
    }

    /// Whether it can take a first prompt on its command line.
    pub(crate) fn takes_prompt(self) -> bool {
        self.is_agent()
    }

    /// Whether a session can be resumed after the daemon restarts.
    pub(crate) fn can_resume(self) -> bool {
        self.is_agent()
    }

    /// Whether the program suspends itself on Ctrl+Z and waits to be
    /// continued (see `PtyHandle::resume_if_suspended`).
    pub(crate) fn suspends_itself(self) -> bool {
        self.is_agent()
    }

    /// The hook socket's address for its reports: `/hooks/<route>`.
    pub(crate) fn hook_route(self) -> Option<&'static str> {
        match self.kind {
            Kind::Claude | Kind::Review => Some("claude"),
            Kind::Codex => Some("codex"),
            Kind::Gemini => Some("gemini"),
            Kind::OpenCode => Some("opencode"),
            Kind::Copilot => Some("copilot"),
            Kind::Shell | Kind::Checks => None,
        }
    }

    /// What a report posted to its route (and `event`, for a harness that
    /// names it in the address) means to the session machine, with the
    /// agent session it came from.
    pub(crate) fn translate(
        self,
        event: Option<&str>,
        body: &[u8],
    ) -> Option<(Option<String>, HookEvent)> {
        match self.kind {
            Kind::Claude | Kind::Review => claude::translate(&claude::HookPayload::parse(body)?),
            Kind::Codex => codex::translate(body),
            Kind::Gemini => gemini::translate(body),
            Kind::OpenCode => opencode::translate(body),
            Kind::Copilot => copilot::translate(event?, body),
            Kind::Shell | Kind::Checks => None,
        }
    }

    /// What a report says the session has used, for a harness that reports
    /// it: `OpenCode`'s plugin, at each turn's end.
    pub(crate) fn spend(self, body: &[u8]) -> Option<muster_core::Spend> {
        match self.kind {
            Kind::OpenCode => opencode::spend(body),
            _ => None,
        }
    }

    /// What its window title says, for a harness whose title says how it
    /// is doing.
    pub(crate) fn read_title(self, title: &str) -> Option<TitleState> {
        match self.kind {
            Kind::Codex => codex::read_title(title),
            Kind::Gemini => gemini::read_title(title),
            _ => None,
        }
    }

    /// Whether its title is its only word of waiting on the person, and of
    /// its start: Codex's, which runs no hooks Muster could pass it.
    pub(crate) fn title_reports_waits(self) -> bool {
        self.kind == Kind::Codex
    }

    /// Whether the agent still has `agent_session`'s conversation to resume:
    /// `None` if it cannot tell, and the resume is tried.
    pub(crate) fn has_conversation(self, agent_session: &str) -> Option<bool> {
        match self.kind {
            Kind::Claude | Kind::Review => transcript::projects_dir()
                .and_then(|projects| transcript::exists(&projects, agent_session)),
            _ => None,
        }
    }

    /// Writes the files of Muster's own it reads its hooks from into `dir`.
    pub(crate) fn prepare_support(self, dir: &std::path::Path) -> std::io::Result<()> {
        support::prepare(self.kind, dir)
    }
}

/// Where a session's hooks report to, and the token that names the session.
#[derive(Clone, Debug)]
pub(crate) struct HookEndpoint {
    pub(crate) socket: PathBuf,
    pub(crate) token: crate::secrets::Token,
}

/// Where a session's program runs.
#[derive(Clone, Debug)]
pub(crate) enum Place {
    /// On the host, in this directory, as the person, with their sign-ins.
    Host(PathBuf),
    /// In a run's container, through `docker exec` with a terminal: the
    /// daemon's terminal drives docker's, which drives the program's
    /// (`ARCHITECTURE.md` §5). Claude Code there gets `secret`, through
    /// docker's environment, never its arguments (`PRD-06`).
    Container {
        docker: String,
        container: String,
        secret: Option<Secret>,
    },
}

impl Place {
    /// Whether hooks can reach the daemon from there: its socket is on the
    /// host, out of a container's reach.
    pub(crate) fn hears_hooks(&self) -> bool {
        matches!(self, Self::Host(_))
    }
}

/// Everything needed to start a session's program.
#[derive(Clone, Debug)]
pub(crate) struct Launch {
    pub(crate) session: SessionId,
    /// The harness's own session id: chosen by Muster for a new session of
    /// a harness that lets it, the saved one for a resumed session.
    pub(crate) agent_session: Option<String>,
    pub(crate) resume: bool,
    pub(crate) prompt: Option<String>,
    pub(crate) hooks: Option<HookEndpoint>,
    pub(crate) place: Place,
    pub(crate) size: TermSize,
    /// The checks, for the harness that runs them.
    pub(crate) checks: Option<CheckCommands>,
    /// The folder holding the files of Muster's own a harness reads its
    /// hooks from ([`support`]), once written.
    pub(crate) support: Option<PathBuf>,
}

/// Runs a session's program in a container: `$1` the session, `$2` the
/// harness, `$3` its agent session. It records its process, which the
/// program then becomes, so Muster can stop it there: stopping docker's
/// client stops nothing in the container. A program the session ran
/// before, still there after a daemon restart, is stopped first; a pid is
/// trusted only if that process carries the session in its environment,
/// as a pid can be reused. Stopping is what a closed terminal does, a
/// hangup (an interactive shell ignores a plain terminate), and a kill if
/// the program is still there after two seconds. Claude Code resumes its
/// conversation if the container has the transcript, and starts it under
/// that id otherwise; the container is the sandbox, so it asks no
/// permissions, as the building agent does.
pub(crate) const IN_CONTAINER: &str = r#"stop_session() {
  f="/tmp/muster/sessions/$1.pid"
  [ -s "$f" ] || return 0
  pid="$(cat "$f")"
  if tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep -qx "MUSTER_SESSION=$1"; then
    kill -HUP -- "-$pid" 2>/dev/null || kill -HUP "$pid" 2>/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.2 2>/dev/null || sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -KILL -- "-$pid" 2>/dev/null || kill -KILL "$pid" 2>/dev/null
    fi
  fi
  rm -f "$f"
}
"#;

/// Starts a session's program: [`IN_CONTAINER`], then this.
const START_IN_CONTAINER: &str = r#"stop_session "$1"
mkdir -p /tmp/muster/sessions && echo $$ > "/tmp/muster/sessions/$1.pid"
cd /work 2>/dev/null || cd
case "$2" in
  claude)
    if ls "$HOME"/.claude/projects/*/"$3".jsonl >/dev/null 2>&1; then
      exec claude --dangerously-skip-permissions --resume "$3"
    fi
    exec claude --dangerously-skip-permissions --session-id "$3" ;;
  *)
    command -v bash >/dev/null 2>&1 && exec bash -l
    exec sh -l ;;
esac
"#;

/// Stops a session's program in a container: `$1` the session.
pub(crate) const STOP_IN_CONTAINER: &str = r#"stop_session "$1""#;

/// How long a hook waits for the daemon. Hooks run inside the agent's turn,
/// so a hung daemon must cost seconds, not a stuck session.
const HOOK_TIMEOUT_SECONDS: u32 = 3;

/// The `curl` that posts its standard input to the hook socket at
/// `/hooks/<route>[/<event>]` with the session's token. It prints nothing
/// (a hook's output can reach the model) and never fails.
pub(crate) fn hook_curl(route: &str, event: Option<&str>) -> String {
    let address = match event {
        Some(event) => format!("http://muster/hooks/{route}/{event}"),
        None => format!("http://muster/hooks/{route}"),
    };
    format!(
        "curl -sS -m {HOOK_TIMEOUT_SECONDS} --unix-socket \"$MUSTER_HOOK_SOCKET\" \
         -H \"Authorization: Bearer $MUSTER_HOOK_TOKEN\" -H \"Content-Type: application/json\" \
         --data-binary @- {address} >/dev/null 2>&1 || true"
    )
}

/// The command a hook runs with its payload on standard input: inert
/// outside a Muster session, and it never fails the agent's turn.
pub(crate) fn hook_command(route: &str, event: Option<&str>) -> String {
    format!(
        "[ -n \"$MUSTER_HOOK_TOKEN\" ] && [ -S \"$MUSTER_HOOK_SOCKET\" ] || exit 0; {}",
        hook_curl(route, event)
    )
}

/// How to start `harness` for `launch`.
pub(crate) fn spawn_spec(harness: Harness, launch: &Launch) -> SpawnSpec {
    let env = vec![
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ("MUSTER_SESSION".to_owned(), launch.session.to_string()),
    ];
    match &launch.place {
        Place::Host(cwd) => on_host(harness, launch, cwd.clone(), env),
        Place::Container {
            docker,
            container,
            secret,
        } => in_container(harness, launch, (docker, container), secret.as_ref(), env),
    }
}

fn on_host(
    harness: Harness,
    launch: &Launch,
    cwd: PathBuf,
    mut env: Vec<(String, String)>,
) -> SpawnSpec {
    if let Some(hooks) = &launch.hooks {
        env.push((
            "MUSTER_HOOK_SOCKET".to_owned(),
            hooks.socket.to_string_lossy().into_owned(),
        ));
        env.push((
            "MUSTER_HOOK_TOKEN".to_owned(),
            hooks.token.value().to_owned(),
        ));
    }
    let place = cwd.to_string_lossy().into_owned();
    // The files of Muster's own, where hooks are heard.
    let support = launch.support.as_deref().filter(|_| launch.hooks.is_some());
    let own = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
    let (program, args) = match harness.kind {
        Kind::Shell => (
            own("SHELL").unwrap_or_else(|| "/bin/sh".to_owned()),
            Vec::new(),
        ),
        Kind::Claude => ("claude".to_owned(), claude::arguments(launch)),
        Kind::Review => {
            // Plan mode, resumed or not: a review changes nothing.
            let mut args = vec!["--permission-mode".to_owned(), "plan".to_owned()];
            args.extend(claude::arguments(launch));
            ("claude".to_owned(), args)
        }
        Kind::Codex => ("codex".to_owned(), codex::arguments(launch, &place)),
        Kind::Gemini => {
            // A defaults file the person named stays theirs.
            if let Some(dir) = support.filter(|_| own(gemini::DEFAULTS_VARIABLE).is_none()) {
                let defaults = support::gemini_defaults(dir);
                env.push((
                    gemini::DEFAULTS_VARIABLE.to_owned(),
                    defaults.to_string_lossy().into_owned(),
                ));
            }
            ("gemini".to_owned(), gemini::arguments(launch))
        }
        Kind::OpenCode => {
            if let Some(dir) = support {
                let plugin = support::opencode_plugin(dir);
                let content = opencode::config_content(
                    own(opencode::CONFIG_VARIABLE).as_deref(),
                    &plugin.to_string_lossy(),
                );
                if let Some(content) = content {
                    env.push((opencode::CONFIG_VARIABLE.to_owned(), content));
                }
            }
            ("opencode".to_owned(), opencode::arguments(launch, &place))
        }
        Kind::Copilot => {
            let plugin =
                support.map(|dir| support::copilot_plugin(dir).to_string_lossy().into_owned());
            (
                "copilot".to_owned(),
                copilot::arguments(launch, plugin.as_deref()),
            )
        }
        Kind::Checks => {
            let checks = launch.checks.clone().unwrap_or(CheckCommands {
                commands: Vec::new(),
                limit: std::time::Duration::from_secs(60),
            });
            let mut args = vec![
                "-c".to_owned(),
                CHECKS_BY_HAND.to_owned(),
                "sh".to_owned(),
                checks.limit.as_secs().max(1).to_string(),
            ];
            args.extend(checks.commands);
            ("/bin/sh".to_owned(), args)
        }
    };
    SpawnSpec {
        program,
        args,
        cwd,
        env,
        env_remove: scrub::scrubbed(std::env::vars().map(|(key, _)| key)),
        size: launch.size,
    }
}

/// `docker exec` with a terminal, running [`START_IN_CONTAINER`].
fn in_container(
    harness: Harness,
    launch: &Launch,
    (docker, container): (&str, &str),
    secret: Option<&Secret>,
    mut env: Vec<(String, String)>,
) -> SpawnSpec {
    if let Some(secret) = secret.filter(|_| harness.kind == Kind::Claude) {
        env.push((secret.variable.to_owned(), secret.value().to_owned()));
    }
    let mut args: Vec<String> = ["exec", "--interactive", "--tty"]
        .map(str::to_owned)
        .to_vec();
    // Values reach the program through docker's own environment.
    for (name, _) in &env {
        args.extend(["--env".to_owned(), name.clone()]);
    }
    let kind = match harness.kind {
        // A review never runs in a container (`place_for` refuses agents).
        Kind::Claude | Kind::Review => "claude",
        // Other agents never run in a container either (`place_for`), and
        // checks never by hand: a run checks its own work.
        Kind::Shell
        | Kind::Checks
        | Kind::Codex
        | Kind::Gemini
        | Kind::OpenCode
        | Kind::Copilot => "shell",
    };
    args.extend([
        container.to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        format!("{IN_CONTAINER}{START_IN_CONTAINER}"),
        "sh".to_owned(),
        launch.session.to_string(),
        kind.to_owned(),
        launch.agent_session.clone().unwrap_or_default(),
    ]);
    SpawnSpec {
        program: docker.to_owned(),
        args,
        // Docker's client needs no particular directory.
        cwd: PathBuf::from("/"),
        env,
        env_remove: scrub::scrubbed(std::env::vars().map(|(key, _)| key)),
        size: launch.size,
    }
}

/// A secret that names one session to the hook receiver (`ARC-11`).
pub(crate) fn new_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(muster_core::hex::encode(&bytes))
}

#[cfg(test)]
mod in_container_tests;

#[cfg(test)]
mod tests;
