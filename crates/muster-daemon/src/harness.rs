//! What to run for a session (`ARCHITECTURE.md` §7).
//!
//! The harnesses: `shell`, the person's own shell in the workspace; the
//! coding agents `claude` (Claude Code), `codex` (Codex), `gemini` (Gemini
//! CLI), `opencode` (`OpenCode`) and `copilot` (GitHub Copilot CLI); and two
//! Muster runs by hand, `checks` and `review`; and any coding agent the
//! person added in Muster's own settings (`[agents.<name>]`,
//! `muster_core::config::agents`); and a plugin's panels
//! (`plugin:<plugin>/<panel>`, [`panel`]). A harness says how to start, resume and
//! hear from its program; the daemon does the rest. Each agent is heard the
//! way it allows without editing the person's config (`PRD-13`): hooks
//! passed at launch, a plugin or settings file of Muster's own that it is
//! pointed at, or what it shows in its window title. An agent the person
//! added is heard without hooks: its terminal's progress sequence, the
//! words of its window title its definition names, and its exit.

pub(crate) mod activity;
pub(crate) mod claude;
pub(crate) mod claude_stream;
pub(crate) mod codex;
pub(crate) mod copilot;
pub(crate) mod gemini;
pub(crate) mod opencode;
pub(crate) mod panel;
pub(crate) mod scrub;
pub(crate) mod support;
pub(crate) mod transcript;

use std::path::PathBuf;
use std::sync::Arc;

use muster_core::SessionId;
use muster_core::config::agents::{AgentDefinition, TitleSays};
use muster_core::protocol::TermSize;
use muster_state::session::HookEvent;

pub(crate) use self::panel::PanelLaunch;
use crate::pty::SpawnSpec;
use crate::runs::credentials::Secret;

/// A harness Muster can start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Harness {
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// A coding agent the person added in Muster's own settings.
    Added(Arc<AgentDefinition>),
    /// A plugin's panel: no agent, but a program shown beside one.
    Panel(Arc<PanelLaunch>),
}

/// Muster's own harnesses.
const BUILT_IN: [Kind; 8] = [
    Kind::Shell,
    Kind::Claude,
    Kind::Codex,
    Kind::Gemini,
    Kind::OpenCode,
    Kind::Copilot,
    Kind::Checks,
    Kind::Review,
];

/// Muster's own harness called `id`, if there is one.
pub(crate) fn find(id: &str) -> Option<Harness> {
    BUILT_IN
        .into_iter()
        .map(|kind| Harness { kind })
        .find(|harness| harness.id() == id)
}

/// The harness called `id`: one of Muster's own, or else one of the agents
/// the person `added`.
pub(crate) fn find_with(id: &str, added: &[Arc<AgentDefinition>]) -> Option<Harness> {
    find(id).or_else(|| {
        added
            .iter()
            .find(|agent| agent.name == id)
            .map(|agent| Harness::added(agent.clone()))
    })
}

/// The coding agents Muster can start, its own then those the person
/// `added`: those with a program of their own ([`Harness::program`]).
pub(crate) fn coding_agents(added: &[Arc<AgentDefinition>]) -> Vec<Harness> {
    BUILT_IN
        .into_iter()
        .map(|kind| Harness { kind })
        .chain(added.iter().cloned().map(Harness::added))
        .filter(|harness| harness.program().is_some())
        .collect()
}

/// The harness of a session that ran as `id`, to keep it running: as
/// [`find_with`] finds it, a plugin's panel by its harness alone, and an
/// agent the settings no longer define by its name alone, which cannot be
/// started again.
pub(crate) fn carried(id: &str, added: &[Arc<AgentDefinition>]) -> Harness {
    find_with(id, added)
        .or_else(|| PanelLaunch::carried(id).map(Harness::panel))
        .unwrap_or_else(|| Harness::added(Arc::new(AgentDefinition::bare(id))))
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
    /// The agent the person added as `agent`.
    pub(crate) fn added(agent: Arc<AgentDefinition>) -> Self {
        Self {
            kind: Kind::Added(agent),
        }
    }

    /// A plugin's panel, run as `launch` says.
    pub(crate) fn panel(launch: PanelLaunch) -> Self {
        Self {
            kind: Kind::Panel(Arc::new(launch)),
        }
    }

    /// What sessions name it by.
    pub(crate) fn id(&self) -> &str {
        match &self.kind {
            Kind::Shell => "shell",
            Kind::Claude => "claude",
            Kind::Codex => "codex",
            Kind::Gemini => "gemini",
            Kind::OpenCode => "opencode",
            Kind::Copilot => "copilot",
            Kind::Checks => "checks",
            Kind::Review => "review",
            Kind::Added(agent) => &agent.name,
            Kind::Panel(launch) => launch.harness(),
        }
    }

    /// The default title of its sessions.
    pub(crate) fn label(&self) -> &str {
        match &self.kind {
            Kind::Shell => "Shell",
            Kind::Claude => "Claude Code",
            Kind::Codex => "Codex",
            Kind::Gemini => "Gemini CLI",
            Kind::OpenCode => "OpenCode",
            Kind::Copilot => "Copilot CLI",
            Kind::Checks => "Checks",
            Kind::Review => "Review",
            Kind::Added(agent) => &agent.label,
            Kind::Panel(launch) => &launch.title,
        }
    }

    /// The agent the person added, if it is one.
    fn added_agent(&self) -> Option<&AgentDefinition> {
        match &self.kind {
            Kind::Added(agent) => Some(agent),
            _ => None,
        }
    }

    /// Whether it is a coding agent, rather than a shell.
    pub(crate) fn is_agent(&self) -> bool {
        matches!(
            self.kind,
            Kind::Claude
                | Kind::Review
                | Kind::Codex
                | Kind::Gemini
                | Kind::OpenCode
                | Kind::Copilot
                | Kind::Added(_)
        )
    }

    /// The program it starts, as it is named before `PATH` is searched:
    /// for Muster's own coding agents and those the person added; `None`
    /// for a shell, the checks, a review (Claude Code's own) and a panel.
    pub(crate) fn program(&self) -> Option<&str> {
        match &self.kind {
            Kind::Claude => Some("claude"),
            Kind::Codex => Some("codex"),
            Kind::Gemini => Some("gemini"),
            Kind::OpenCode => Some("opencode"),
            Kind::Copilot => Some("copilot"),
            Kind::Added(agent) => Some(&agent.program),
            Kind::Shell | Kind::Review | Kind::Checks | Kind::Panel(_) => None,
        }
    }

    /// Whether it runs the project's checks.
    pub(crate) fn is_checks(&self) -> bool {
        self.kind == Kind::Checks
    }

    /// Whether the program reports its state to the hook socket, and so
    /// needs a hook token at launch. An agent the person added has no
    /// hooks Muster could pass it without editing its config.
    pub(crate) fn has_hooks(&self) -> bool {
        self.is_agent() && self.added_agent().is_none()
    }

    /// Whether Muster chooses the agent's session id at launch, as Claude
    /// Code, Gemini CLI and Copilot CLI let it. Codex and `OpenCode` choose
    /// their own, which their reports then carry.
    pub(crate) fn chooses_session_id(&self) -> bool {
        matches!(
            self.kind,
            Kind::Claude | Kind::Review | Kind::Gemini | Kind::Copilot
        )
    }

    /// Whether the program says it has started soon after launch, so that
    /// silence past a grace means it is asking something first, such as
    /// whether to trust the folder. Copilot CLI says so only with its first
    /// prompt.
    pub(crate) fn reports_start(&self) -> bool {
        matches!(
            self.kind,
            Kind::Claude | Kind::Review | Kind::Codex | Kind::Gemini | Kind::OpenCode
        )
    }

    /// Whether it can take a first prompt on its command line: an agent
    /// the person added, where its definition says how.
    pub(crate) fn takes_prompt(&self) -> bool {
        match self.added_agent() {
            Some(agent) => agent.takes_prompt(),
            None => self.is_agent(),
        }
    }

    /// Whether a session can be resumed after the daemon restarts: an
    /// agent the person added, where its definition says how.
    pub(crate) fn can_resume(&self) -> bool {
        match self.added_agent() {
            Some(agent) => agent.can_resume(),
            None => self.is_agent(),
        }
    }

    /// Whether it carries on its last conversation with no id to resume
    /// by, as an agent the person added does: it reports none.
    pub(crate) fn resumes_without_id(&self) -> bool {
        self.added_agent().is_some_and(AgentDefinition::can_resume)
    }

    /// Whether its program is told it is in focus and never out of it:
    /// Claude Code writes its suggestion for the person's next prompt only
    /// as a turn ends in focus, and the person mostly comes to a chat after
    /// its turn ended, from another chat, the list or another app.
    pub(crate) fn stays_in_focus(&self) -> bool {
        matches!(self.kind, Kind::Claude | Kind::Review)
    }

    /// Whether the program suspends itself on Ctrl+Z and waits to be
    /// continued (see `PtyHandle::resume_if_suspended`). An agent the
    /// person added is taken to, as every coding agent Muster knows does:
    /// continuing one that has not stopped does nothing.
    pub(crate) fn suspends_itself(&self) -> bool {
        self.is_agent()
    }

    /// The hook socket's address for its reports: `/hooks/<route>`.
    pub(crate) fn hook_route(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Claude | Kind::Review => Some("claude"),
            Kind::Codex => Some("codex"),
            Kind::Gemini => Some("gemini"),
            Kind::OpenCode => Some("opencode"),
            Kind::Copilot => Some("copilot"),
            Kind::Shell | Kind::Checks | Kind::Added(_) | Kind::Panel(_) => None,
        }
    }

    /// What a report posted to its route (and `event`, for a harness that
    /// names it in the address) means to the session machine, with the
    /// agent session it came from.
    pub(crate) fn translate(
        &self,
        event: Option<&str>,
        body: &[u8],
    ) -> Option<(Option<String>, HookEvent)> {
        match self.kind {
            Kind::Claude | Kind::Review => claude::translate(&claude::HookPayload::parse(body)?),
            Kind::Codex => codex::translate(body),
            Kind::Gemini => gemini::translate(body),
            Kind::OpenCode => opencode::translate(body),
            Kind::Copilot => copilot::translate(event?, body),
            Kind::Shell | Kind::Checks | Kind::Added(_) | Kind::Panel(_) => None,
        }
    }

    /// The turn a report says began, by the id the harness's own record of
    /// the conversation knows it by: Claude Code's prompt id, which its
    /// transcript carries, so an interruption written there can be matched
    /// to the turn it ended.
    pub(crate) fn turn_begun(&self, body: &[u8]) -> Option<String> {
        match self.kind {
            Kind::Claude | Kind::Review => claude::turn_begun(&claude::HookPayload::parse(body)?),
            _ => None,
        }
    }

    /// The prompt the person just gave, where a report posted to its route
    /// (and `event`) carries it: Claude Code's `UserPromptSubmit`, Gemini
    /// CLI's `BeforeAgent`, Copilot CLI's `userPromptSubmitted`. Codex,
    /// `OpenCode` and the agents the person added report none.
    pub(crate) fn prompt(&self, event: Option<&str>, body: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(body).ok()?;
        let named = |name: &str| {
            value
                .get("hook_event_name")
                .and_then(serde_json::Value::as_str)
                == Some(name)
        };
        let carries = match &self.kind {
            Kind::Claude | Kind::Review => named("UserPromptSubmit"),
            Kind::Gemini => named("BeforeAgent"),
            Kind::Copilot => event == Some("userPromptSubmitted"),
            // An agent the person added has no hooks to report one.
            Kind::Codex
            | Kind::OpenCode
            | Kind::Shell
            | Kind::Checks
            | Kind::Added(_)
            | Kind::Panel(_) => false,
        };
        carries
            .then(|| value.get("prompt").and_then(serde_json::Value::as_str))
            .flatten()
            .map(str::to_owned)
    }

    /// The start of the answer a turn ended with, where a report posted to
    /// its route carries it: Claude Code's `Stop` (`last_assistant_message`)
    /// and Gemini CLI's `AfterAgent` (`prompt_response`); a chat whose
    /// first prompt said little is named by it too.
    pub(crate) fn answer(&self, body: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(body).ok()?;
        let event = value.get("hook_event_name")?.as_str()?;
        let field = match (&self.kind, event) {
            (Kind::Claude, "Stop") => "last_assistant_message",
            (Kind::Gemini, "AfterAgent") => "prompt_response",
            _ => return None,
        };
        let answer = value.get(field)?.as_str()?;
        (!answer.trim().is_empty()).then(|| answer.to_owned())
    }

    /// How it is asked one question, to name chats: Muster's own as checked
    /// (`muster_core::ask::built_in`), one the person added as its settings
    /// say; `None` for one that cannot be.
    pub(crate) fn asking(&self) -> Option<muster_core::ask::Ask> {
        match &self.kind {
            Kind::Added(agent) => agent.asking(),
            Kind::Shell | Kind::Checks | Kind::Review | Kind::Panel(_) => None,
            _ => muster_core::ask::built_in(self.id()),
        }
    }

    /// What a report says the session has used, for a harness that reports
    /// it: `OpenCode`'s plugin, at each turn's end.
    pub(crate) fn spend(&self, body: &[u8]) -> Option<muster_core::Spend> {
        match self.kind {
            Kind::OpenCode => opencode::spend(body),
            _ => None,
        }
    }

    /// What its window title says, for a harness whose title says how it
    /// is doing: an agent the person added, by the words its definition
    /// names.
    pub(crate) fn read_title(&self, title: &str) -> Option<TitleState> {
        match &self.kind {
            Kind::Codex => codex::read_title(title),
            Kind::Gemini => gemini::read_title(title),
            Kind::Added(agent) => agent.title.read(title).map(|says| match says {
                TitleSays::Working => TitleState::Working,
                TitleSays::Ready => TitleState::Ready,
                TitleSays::Waiting => TitleState::Waiting,
            }),
            _ => None,
        }
    }

    /// Whether its title is its only word of waiting on the person, and of
    /// its start: Codex's, which runs no hooks Muster could pass it, and
    /// that of an agent the person added whose definition names the words
    /// it waits with.
    pub(crate) fn title_reports_waits(&self) -> bool {
        match &self.kind {
            Kind::Codex => true,
            Kind::Added(agent) => agent.title.waiting.is_some(),
            _ => false,
        }
    }

    /// Whether the agent still has `agent_session`'s conversation to resume:
    /// `None` if it cannot tell, and the resume is tried.
    pub(crate) fn has_conversation(&self, agent_session: &str) -> Option<bool> {
        match self.kind {
            Kind::Claude | Kind::Review => transcript::projects_dir()
                .and_then(|projects| transcript::exists(&projects, agent_session)),
            _ => None,
        }
    }

    /// Writes the files of Muster's own it reads its hooks from into `dir`.
    pub(crate) fn prepare_support(&self, dir: &std::path::Path) -> std::io::Result<()> {
        support::prepare(&self.kind, dir)
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
    /// Muster's link to the ssh agent the person's ssh forwarded to this
    /// machine (`muster bridge`), named to the program as `SSH_AUTH_SOCK`
    /// where it leads somewhere.
    pub(crate) ssh_agent: Option<PathBuf>,
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

/// The terminal every session's program is told it runs in: Muster's own,
/// whatever terminal the daemon was started from (`scrub` takes that
/// one's variables away).
const TERMINAL_VARIABLES: [(&str, &str); 4] = [
    ("TERM", "xterm-256color"),
    ("COLORTERM", "truecolor"),
    ("TERM_PROGRAM", "muster"),
    ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION")),
];

/// How to start `harness` for `launch`.
pub(crate) fn spawn_spec(harness: &Harness, launch: &Launch) -> SpawnSpec {
    let mut env: Vec<(String, String)> = TERMINAL_VARIABLES
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    env.push(("MUSTER_SESSION".to_owned(), launch.session.to_string()));
    if matches!(harness.kind, Kind::Shell) {
        // A workspace's terminal: what the person starts from it belongs to
        // the workspace (`muster init`'s functions ask, with no daemon).
        env.push(("MUSTER_TERMINAL".to_owned(), "1".to_owned()));
    }
    if let Some(agent) = &launch.ssh_agent {
        env.push((
            "SSH_AUTH_SOCK".to_owned(),
            agent.to_string_lossy().into_owned(),
        ));
    }
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
    harness: &Harness,
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
    let (program, args) = match &harness.kind {
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
        // As the person would start it, with what its definition adds;
        // after Muster's own variables, so a value it names wins.
        Kind::Added(agent) => {
            env.extend(agent.env.iter().cloned());
            (
                agent.program.clone(),
                agent.arguments(launch.prompt.as_deref(), launch.resume),
            )
        }
        // As its plugin says, told where it is, with its secrets.
        Kind::Panel(panel) => {
            env.extend(panel.variables());
            panel.program()
        }
    };
    let daemon_s = std::env::vars().map(|(key, _)| key);
    let env_remove = match &harness.kind {
        // The basics of the daemon's environment, and what its plugin may
        // see (`ARC-23`).
        Kind::Panel(panel) => scrub::kept_from_plugin(daemon_s, &panel.seen),
        _ => scrub::scrubbed(daemon_s),
    };
    SpawnSpec {
        program,
        args,
        cwd,
        env,
        env_remove,
        size: launch.size,
    }
}

/// `docker exec` with a terminal, running [`START_IN_CONTAINER`].
fn in_container(
    harness: &Harness,
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
        // Other agents never run in a container either (`place_for`), nor
        // a plugin's panel (`start_panel`), and checks never by hand: a run
        // checks its own work.
        Kind::Shell
        | Kind::Checks
        | Kind::Codex
        | Kind::Gemini
        | Kind::OpenCode
        | Kind::Copilot
        | Kind::Added(_)
        | Kind::Panel(_) => "shell",
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
