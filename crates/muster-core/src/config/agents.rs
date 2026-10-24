//! Coding agents the person adds in Muster's own settings (`[agents.<name>]`
//! in `settings.toml`), so an agent Muster does not know runs beside the
//! ones it does, without a Muster release (`PRODUCT.md` §7,
//! `ARCHITECTURE.md` §7). Only the person's own settings can add one,
//! never a repository's: an agent is a program Muster starts as the
//! person.
//!
//! ```toml
//! [agents.my-agent]
//! label = "My Agent"              # its name on screen; the key otherwise
//! program = "my-agent"            # on PATH, or a path
//! args = ["--no-banner"]          # always given
//! prompt = ["--message", "{prompt}"]  # how a first prompt is given; none takes none
//! resume = ["--continue"]         # given instead of a prompt to carry on after a restart
//! env = { MY_AGENT_THEME = "dark" }
//! mark = "M"                      # shown where Muster's own agents have theirs
//! colour = "green"                # the mark's: one of the terminal's colours, or "#rrggbb"
//! ask = ["--print", "{prompt}"]   # how it answers one question and ends, to name chats
//! model = ["--model", "{model}"]  # how a model is chosen for that question
//! models = ["models"]             # how it lists its models, one to a line
//!
//! [agents.my-agent.title]         # words its window title says, if it says any
//! working = "Thinking"
//! ready = "Ready"
//! waiting = "Approve?"
//! ```
//!
//! Muster hears such an agent without hooks: its terminal's progress
//! sequence, its window title where the definition says what to look for,
//! and its exit; the less status detail `PRD-13` allows. Nothing is
//! written into the agent's own config to hear more.
//!
//! Read strictly (`ARC-05`): an unknown key, a value of the wrong kind, or
//! a name one of Muster's own harnesses has is an error naming the key
//! (`ARC-19`). Parsing does no IO (`ARC-01`).

use std::fmt;

use crate::settings::AddedAgent;

/// Where a first prompt goes in [`AgentDefinition::prompt`].
pub const PROMPT: &str = "{prompt}";

/// The keys an agent's table has.
const KEYS: [&str; 12] = [
    "label", "program", "args", "prompt", "resume", "env", "mark", "colour", "title", "ask",
    "model", "models",
];

/// The keys of an agent's `title` table.
const TITLE_KEYS: [&str; 3] = ["working", "ready", "waiting"];

/// The colours a mark can have by name: the terminal's own sixteen, so they
/// follow the person's theme.
const COLOURS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "gray",
    "dark-gray",
    "light-red",
    "light-green",
    "light-yellow",
    "light-blue",
    "light-magenta",
    "light-cyan",
    "white",
];

/// The longest name and label.
const NAME_MAX: usize = 32;
const LABEL_MAX: usize = 40;

/// A coding agent the person defined.
#[derive(Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    /// The key it is defined under, which sessions name it by.
    pub name: String,
    /// How it is called on screen.
    pub label: String,
    /// The program to start: a name found on `PATH`, or a path.
    pub program: String,
    /// Arguments always given, first.
    pub args: Vec<String>,
    /// How a first prompt is given: these arguments, with [`PROMPT`] in
    /// each replaced by it; `None` for an agent that takes none.
    pub prompt: Option<Vec<String>>,
    /// The arguments that carry on its last conversation, given instead of
    /// a prompt when a session is started again after a restart; `None`
    /// for an agent that cannot.
    pub resume: Option<Vec<String>>,
    /// More environment for it.
    pub env: Vec<(String, String)>,
    /// What it is marked by on screen.
    pub mark: char,
    /// The mark's colour: a name from the terminal's sixteen, or `#rrggbb`.
    pub colour: Option<String>,
    /// What its window title says about it.
    pub title: TitleWords,
    /// How it is asked one question, [`crate::ask::QUESTION`] in one of
    /// these arguments, given instead of [`Self::args`]; `None` for an
    /// agent that cannot be, which then names no chats.
    pub ask: Option<Vec<String>>,
    /// How a model is chosen for that question, [`crate::ask::MODEL`] in
    /// one of these arguments.
    pub model: Option<Vec<String>>,
    /// The arguments that print its models, one to a line.
    pub models: Option<Vec<String>>,
}

/// Words in an agent's window title that say how it is doing, each looked
/// for anywhere in the title.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleWords {
    pub working: Option<String>,
    pub ready: Option<String>,
    /// Waiting on the person: it needs them.
    pub waiting: Option<String>,
}

/// What a title says, by the words of [`TitleWords`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleSays {
    Working,
    Ready,
    Waiting,
}

impl TitleWords {
    /// What `title` says: waiting first, as an agent waiting on the person
    /// may still say it works.
    pub fn read(&self, title: &str) -> Option<TitleSays> {
        let says = |words: &Option<String>| words.as_deref().is_some_and(|w| title.contains(w));
        if says(&self.waiting) {
            Some(TitleSays::Waiting)
        } else if says(&self.working) {
            Some(TitleSays::Working)
        } else if says(&self.ready) {
            Some(TitleSays::Ready)
        } else {
            None
        }
    }
}

impl fmt::Debug for AgentDefinition {
    /// Its environment's values are left out: the person's own, and not
    /// for a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env: Vec<&str> = self.env.iter().map(|(key, _)| key.as_str()).collect();
        f.debug_struct("AgentDefinition")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("program", &self.program)
            .field("args", &self.args)
            .field("prompt", &self.prompt)
            .field("resume", &self.resume)
            .field("env", &env)
            .field("mark", &self.mark)
            .field("colour", &self.colour)
            .field("title", &self.title)
            .field("ask", &self.ask)
            .field("model", &self.model)
            .field("models", &self.models)
            .finish()
    }
}

impl AgentDefinition {
    /// An agent known only by `name`: one a session ran that the settings
    /// no longer define, so that a session handed over in a restart in
    /// place keeps running. It cannot be started.
    pub fn bare(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            label: name.to_owned(),
            program: String::new(),
            args: Vec::new(),
            prompt: None,
            resume: None,
            env: Vec::new(),
            mark: first_mark(name),
            colour: None,
            title: TitleWords::default(),
            ask: None,
            model: None,
            models: None,
        }
    }

    /// How it is asked one question, where its settings say.
    pub fn asking(&self) -> Option<crate::ask::Ask> {
        Some(crate::ask::Ask {
            program: self.program.clone(),
            args: self.ask.clone()?,
            model: self.model.clone(),
            models: self.models.clone(),
            default_model: None,
            suggestions: Vec::new(),
        })
    }

    /// Whether it takes a first prompt on its command line.
    pub fn takes_prompt(&self) -> bool {
        self.prompt.is_some()
    }

    /// Whether it can carry on its conversation after a restart.
    pub fn can_resume(&self) -> bool {
        self.resume.is_some()
    }

    /// Its arguments: the fixed ones, then, to carry on, its resume
    /// arguments, or, given a prompt, its prompt's.
    pub fn arguments(&self, prompt: Option<&str>, resume: bool) -> Vec<String> {
        let mut args = self.args.clone();
        match (&self.resume, &self.prompt, prompt) {
            (Some(more), _, _) if resume => args.extend(more.iter().cloned()),
            (_, Some(template), Some(prompt)) if !resume => {
                args.extend(template.iter().map(|arg| arg.replace(PROMPT, prompt)));
            }
            _ => {}
        }
        args
    }

    /// What clients are told of it, to offer it.
    pub fn added(&self) -> AddedAgent {
        AddedAgent {
            name: self.name.clone(),
            label: self.label.clone(),
            mark: self.mark.to_string(),
            colour: self.colour.clone(),
            takes_prompt: self.takes_prompt(),
            asks: self.ask.is_some(),
        }
    }
}

/// Why an agent's definition does not read. Each says which key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("agents must be a table, with an [agents.<name>] for each agent")]
    NotATable,
    #[error("agents.{name} must be a table of its settings: [agents.{name}]")]
    NotAnAgent { name: String },
    #[error("agents.{name}: {name:?} is one of Muster's own; call yours something else")]
    BuiltIn { name: String },
    #[error(
        "agents.{name}: a name is lowercase letters, digits, `-` and `_`, starting with a \
         letter, at most 32"
    )]
    BadName { name: String },
    #[error("agents.{name}.{key} is not a setting of an agent: they are {known}")]
    UnknownKey {
        name: String,
        key: String,
        known: String,
    },
    #[error("agents.{name}.{key} must be {expected}")]
    Invalid {
        name: String,
        key: String,
        expected: &'static str,
    },
    #[error("agents.{name}.program is missing: the program to start, on PATH or by its path")]
    NoProgram { name: String },
    #[error("agents.{name}.prompt must hold {{prompt}} where the first prompt goes")]
    NoPromptPlace { name: String },
    #[error("agents.{name}.ask must hold {{prompt}} where the question goes")]
    NoQuestionPlace { name: String },
    #[error("agents.{name}.model must hold {{model}} where the model goes")]
    NoModelPlace { name: String },
}

/// Reads the `agents` table of Muster's own settings, ordered by name.
pub fn parse(agents: &toml::Value) -> Result<Vec<AgentDefinition>, AgentError> {
    let toml::Value::Table(agents) = agents else {
        return Err(AgentError::NotATable);
    };
    agents
        .iter()
        .map(|(name, value)| definition(name, value))
        .collect()
}

/// One agent's table, under `name`.
fn definition(name: &str, value: &toml::Value) -> Result<AgentDefinition, AgentError> {
    if crate::harness::is_built_in(name) {
        return Err(AgentError::BuiltIn {
            name: name.to_owned(),
        });
    }
    let named = name.len() <= NAME_MAX
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !named {
        return Err(AgentError::BadName {
            name: name.to_owned(),
        });
    }
    let toml::Value::Table(table) = value else {
        return Err(AgentError::NotAnAgent {
            name: name.to_owned(),
        });
    };
    let read = Read { name, table };
    read.known(table, &KEYS, "")?;
    let program = read.text("program")?.ok_or_else(|| AgentError::NoProgram {
        name: name.to_owned(),
    })?;
    let prompt = read.texts("prompt", false)?;
    if prompt
        .as_ref()
        .is_some_and(|prompt| !prompt.iter().any(|arg| arg.contains(PROMPT)))
    {
        return Err(AgentError::NoPromptPlace {
            name: name.to_owned(),
        });
    }
    let holds = |args: &Option<Vec<String>>, place: &str| {
        args.as_ref()
            .is_none_or(|args| args.iter().any(|arg| arg.contains(place)))
    };
    let ask = read.texts("ask", false)?;
    if !holds(&ask, crate::ask::QUESTION) {
        return Err(AgentError::NoQuestionPlace {
            name: name.to_owned(),
        });
    }
    let model = read.texts("model", false)?;
    if !holds(&model, crate::ask::MODEL) {
        return Err(AgentError::NoModelPlace {
            name: name.to_owned(),
        });
    }
    let label = match read.text("label")? {
        Some(label) if label.chars().count() > LABEL_MAX => {
            return Err(read.invalid("label", "at most 40 characters"));
        }
        Some(label) => label,
        None => name.to_owned(),
    };
    let mark = match read.text("mark")? {
        Some(mark) => {
            let mut chars = mark.chars();
            match (chars.next(), chars.next()) {
                (Some(mark), None) => mark,
                _ => return Err(read.invalid("mark", "one character")),
            }
        }
        None => first_mark(&label),
    };
    Ok(AgentDefinition {
        name: name.to_owned(),
        label,
        program,
        args: read.texts("args", true)?.unwrap_or_default(),
        prompt,
        resume: read.texts("resume", true)?,
        env: read.env()?,
        mark,
        colour: read.colour()?,
        title: read.title()?,
        ask,
        model,
        models: read.texts("models", false)?,
    })
}

/// The mark an agent has unless it names one: the first letter or digit
/// of its label, as a capital.
fn first_mark(label: &str) -> char {
    label
        .chars()
        .find(|c| c.is_alphanumeric())
        .and_then(|c| c.to_uppercase().next())
        .unwrap_or('?')
}

/// Reads the keys of agent `name`'s `table`.
struct Read<'a> {
    name: &'a str,
    table: &'a toml::Table,
}

impl Read<'_> {
    fn invalid(&self, key: &str, expected: &'static str) -> AgentError {
        AgentError::Invalid {
            name: self.name.to_owned(),
            key: key.to_owned(),
            expected,
        }
    }

    /// Refuses a key of `table` not in `keys`; `within` is the path to it
    /// below the agent, such as `title.`.
    fn known(&self, table: &toml::Table, keys: &[&str], within: &str) -> Result<(), AgentError> {
        match table.keys().find(|key| !keys.contains(&key.as_str())) {
            Some(key) => Err(AgentError::UnknownKey {
                name: self.name.to_owned(),
                key: format!("{within}{key}"),
                known: keys.join(", "),
            }),
            None => Ok(()),
        }
    }

    /// The text at `key`, if written: one line, not empty, and without
    /// another control character (a NUL, which no program's argument can
    /// hold, among them).
    fn text(&self, key: &str) -> Result<Option<String>, AgentError> {
        match self.table.get(key) {
            None => Ok(None),
            Some(toml::Value::String(text)) if crate::text::is_one_line(text) => {
                Ok(Some(text.clone()))
            }
            Some(toml::Value::String(_)) => Err(self.invalid(key, "one line of text, not empty")),
            Some(_) => Err(self.invalid(key, "text")),
        }
    }

    /// The list of arguments at `key`, if written; `empty` says whether
    /// it may be empty.
    fn texts(&self, key: &str, empty: bool) -> Result<Option<Vec<String>>, AgentError> {
        let Some(value) = self.table.get(key) else {
            return Ok(None);
        };
        let expected = if empty {
            "a list of arguments, each text without a NUL"
        } else {
            "a list of arguments, not empty, each text without a NUL"
        };
        let toml::Value::Array(items) = value else {
            return Err(self.invalid(key, expected));
        };
        if items.is_empty() && !empty {
            return Err(self.invalid(key, expected));
        }
        items
            .iter()
            .map(|item| match item {
                toml::Value::String(text) if !text.contains('\0') => Ok(text.clone()),
                _ => Err(self.invalid(key, expected)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// `env`: variables by their names, none of them Muster's own.
    fn env(&self) -> Result<Vec<(String, String)>, AgentError> {
        let Some(value) = self.table.get("env") else {
            return Ok(Vec::new());
        };
        let toml::Value::Table(env) = value else {
            return Err(self.invalid("env", "a table of variables: env = { NAME = \"value\" }"));
        };
        env.iter()
            .map(|(key, value)| {
                let at = format!("env.{key}");
                let named = key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !named || key.starts_with("MUSTER_") {
                    return Err(self.invalid(
                        &at,
                        "a variable's name: letters, digits and _, not starting with a digit, \
                         nor with MUSTER_, which are Muster's own",
                    ));
                }
                match value {
                    toml::Value::String(text) if !text.contains('\0') => {
                        Ok((key.clone(), text.clone()))
                    }
                    _ => Err(self.invalid(&at, "text without a NUL")),
                }
            })
            .collect()
    }

    /// `colour`, as written in lowercase.
    fn colour(&self) -> Result<Option<String>, AgentError> {
        let expected = "one of the terminal's colours (red, green, yellow, blue, magenta, \
                        cyan, gray, white, black, dark-gray, or light- before one of the first \
                        six) or #rrggbb";
        let Some(colour) = self.text("colour")? else {
            return Ok(None);
        };
        let colour = colour.trim().to_ascii_lowercase();
        let hex = colour.strip_prefix('#').is_some_and(|digits| {
            digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit())
        });
        if hex || COLOURS.contains(&colour.as_str()) {
            Ok(Some(colour))
        } else {
            Err(self.invalid("colour", expected))
        }
    }

    /// `title`: the words its window title says.
    fn title(&self) -> Result<TitleWords, AgentError> {
        let Some(value) = self.table.get("title") else {
            return Ok(TitleWords::default());
        };
        let toml::Value::Table(title) = value else {
            return Err(self.invalid(
                "title",
                "a table: working, ready and waiting, each the words its title says then",
            ));
        };
        self.known(title, &TITLE_KEYS, "title.")?;
        let words = |key: &str| match title.get(key) {
            None => Ok(None),
            Some(toml::Value::String(text)) if crate::text::is_one_line(text) => {
                Ok(Some(text.clone()))
            }
            Some(_) => Err(self.invalid(&format!("title.{key}"), "one line of text, not empty")),
        };
        Ok(TitleWords {
            working: words("working")?,
            ready: words("ready")?,
            waiting: words("waiting")?,
        })
    }
}

/// Whether `text` is one line with something on it, and no other control
/// character.
#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(text: &str) -> Result<Vec<AgentDefinition>, AgentError> {
        let table: toml::Table = toml::from_str(text).unwrap();
        parse(&table["agents"])
    }

    fn error(text: &str) -> String {
        parse_text(text).unwrap_err().to_string()
    }

    #[test]
    fn an_agent_that_says_how_to_ask_it_one_question_can_name_chats() {
        let read = parse_text(
            "[agents.pi]\nprogram = \"pi\"\nargs = [\"--tui\"]\nask = [\"-p\", \"{prompt}\"]\n\
             model = [\"--model\", \"{model}\"]\nmodels = [\"--list-models\"]\n",
        )
        .unwrap();
        let asking = read[0].asking().unwrap();
        assert_eq!(asking.program, "pi");
        // Asked without its fixed arguments, which start it in its window.
        assert_eq!(
            asking.arguments("Name it?", Some("fast")),
            ["--model", "fast", "-p", "Name it?"]
        );
        assert_eq!(asking.models, Some(vec!["--list-models".to_owned()]));
        assert!(read[0].added().asks);
        let quiet = parse_text("[agents.quiet]\nprogram = \"q\"\n").unwrap();
        assert_eq!(quiet[0].asking(), None);
        assert!(!quiet[0].added().asks);
    }

    #[test]
    fn asking_says_where_the_question_and_the_model_go() {
        assert_eq!(
            error("[agents.pi]\nprogram = \"pi\"\nask = [\"-p\"]\n"),
            "agents.pi.ask must hold {prompt} where the question goes"
        );
        assert_eq!(
            error("[agents.pi]\nprogram = \"pi\"\nask = [\"{prompt}\"]\nmodel = [\"-m\"]\n"),
            "agents.pi.model must hold {model} where the model goes"
        );
        assert!(error("[agents.pi]\nprogram = \"pi\"\nmodels = []\n").contains("agents.pi.models"));
    }

    #[test]
    fn a_whole_definition_reads() {
        let agents = parse_text(
            r#"
            [agents.my-agent]
            label = "My Agent"
            program = "/opt/my-agent/bin/my-agent"
            args = ["--no-banner"]
            prompt = ["--message", "{prompt}"]
            resume = ["--continue"]
            env = { MY_AGENT_THEME = "dark" }
            mark = "✦"
            colour = "Light-Green"

            [agents.my-agent.title]
            working = "Thinking"
            ready = "Ready"
            waiting = "Approve?"

            [agents.bare]
            program = "bare-agent"
            "#,
        )
        .unwrap();
        assert_eq!(agents.len(), 2);
        // In the order of their names.
        let (bare, mine) = (&agents[0], &agents[1]);
        assert_eq!(mine.name, "my-agent");
        assert_eq!(mine.label, "My Agent");
        assert_eq!(mine.program, "/opt/my-agent/bin/my-agent");
        assert_eq!(mine.args, ["--no-banner"]);
        assert_eq!(mine.env, [("MY_AGENT_THEME".to_owned(), "dark".to_owned())]);
        assert_eq!(mine.mark, '✦');
        assert_eq!(mine.colour.as_deref(), Some("light-green"));
        assert_eq!(mine.title.waiting.as_deref(), Some("Approve?"));
        assert!(mine.takes_prompt() && mine.can_resume());

        assert_eq!(bare.label, "bare");
        assert_eq!(bare.mark, 'B');
        assert_eq!(bare.args, Vec::<String>::new());
        assert!(!bare.takes_prompt() && !bare.can_resume());
        assert_eq!(bare.colour, None);
        assert_eq!(bare.title, TitleWords::default());
    }

    #[test]
    fn its_arguments_take_the_prompt_or_carry_on() {
        let agents = parse_text(
            r#"
            [agents.a]
            program = "a"
            args = ["--quiet"]
            prompt = ["--message={prompt}", "--then", "{prompt}"]
            resume = ["--continue"]
            "#,
        )
        .unwrap();
        let agent = &agents[0];
        assert_eq!(agent.arguments(None, false), ["--quiet"]);
        assert_eq!(
            agent.arguments(Some("fix it"), false),
            ["--quiet", "--message=fix it", "--then", "fix it"]
        );
        // Carrying on, the prompt was given already.
        assert_eq!(
            agent.arguments(Some("fix it"), true),
            ["--quiet", "--continue"]
        );
        let bare = AgentDefinition::bare("gone");
        assert_eq!(bare.arguments(Some("x"), false), Vec::<String>::new());
        assert_eq!(bare.arguments(None, true), Vec::<String>::new());
    }

    #[test]
    fn clients_are_told_what_to_offer_and_never_its_environment() {
        let agents = parse_text(
            r#"
            [agents.a]
            program = "a"
            prompt = ["{prompt}"]
            env = { A_TOKEN_FILE = "/somewhere/secret-value" }
            "#,
        )
        .unwrap();
        let added = agents[0].added();
        assert_eq!(
            added,
            AddedAgent {
                name: "a".to_owned(),
                label: "a".to_owned(),
                mark: "A".to_owned(),
                colour: None,
                takes_prompt: true,
                asks: false,
            }
        );
        let debug = format!("{:?}", agents[0]);
        assert!(debug.contains("A_TOKEN_FILE"), "{debug}");
        assert!(!debug.contains("secret-value"), "{debug}");
    }

    #[test]
    fn its_title_s_words_say_waiting_first() {
        let words = TitleWords {
            working: Some("Working".to_owned()),
            ready: Some("Ready".to_owned()),
            waiting: Some("Approve?".to_owned()),
        };
        assert_eq!(words.read("⠋ Working"), Some(TitleSays::Working));
        assert_eq!(words.read("Ready · app"), Some(TitleSays::Ready));
        assert_eq!(
            words.read("Working · Approve? (y/n)"),
            Some(TitleSays::Waiting)
        );
        assert_eq!(words.read("vim README.md"), None);
        assert_eq!(TitleWords::default().read("Working"), None);
    }

    #[test]
    fn a_built_in_s_name_is_refused() {
        let own = crate::harness::OwnAgent::ALL.map(crate::harness::OwnAgent::id);
        for name in own.into_iter().chain(["shell", "checks", "review"]) {
            let text = format!("[agents.{name}]\nprogram = \"x\"\n");
            let said = error(&text);
            assert!(
                said.contains(&format!("agents.{name}")) && said.contains("Muster's own"),
                "{said}"
            );
        }
    }

    #[test]
    fn each_mistake_names_its_key() {
        for (text, says) in [
            ("agents = 3", "agents must be a table"),
            ("[agents]\nx = 3", "agents.x must be a table"),
            (
                "[agents.My-Agent]\nprogram = \"x\"",
                "agents.My-Agent: a name",
            ),
            ("[agents.1st]\nprogram = \"x\"", "agents.1st: a name"),
            (
                "[agents.a]\nprogram = \"x\"\ncolor = \"red\"",
                "agents.a.color is not a setting of an agent",
            ),
            ("[agents.a]\nlabel = \"A\"", "agents.a.program is missing"),
            (
                "[agents.a]\nprogram = \"  \"",
                "agents.a.program must be one line",
            ),
            ("[agents.a]\nprogram = 7", "agents.a.program must be text"),
            (
                "[agents.a]\nprogram = \"x\"\nargs = \"--quiet\"",
                "agents.a.args must be a list",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nargs = [1]",
                "agents.a.args must be a list",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nargs = [\"a\\u0000b\"]",
                "agents.a.args must be a list",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nprompt = [\"--message\"]",
                "agents.a.prompt must hold {prompt}",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nprompt = []",
                "agents.a.prompt must be a list of arguments, not empty",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nmark = \"AB\"",
                "agents.a.mark must be one character",
            ),
            (
                "[agents.a]\nprogram = \"x\"\ncolour = \"teal-ish\"",
                "agents.a.colour must be one of the terminal's colours",
            ),
            (
                "[agents.a]\nprogram = \"x\"\ncolour = \"#12345g\"",
                "agents.a.colour must be",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nenv = { MUSTER_SESSION = \"x\" }",
                "agents.a.env.MUSTER_SESSION must be a variable's name",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nenv = { \"A B\" = \"x\" }",
                "agents.a.env.A B must be a variable's name",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nenv = { A = 1 }",
                "agents.a.env.A must be text",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nlabel = \"two\\nlines\"",
                "agents.a.label must be one line",
            ),
            (
                "[agents.a]\nprogram = \"x\"\nlabel = \"an agent whose label goes on and on and on\"",
                "agents.a.label must be at most 40",
            ),
            (
                "[agents.a]\nprogram = \"x\"\ntitle = \"Ready\"",
                "agents.a.title must be a table",
            ),
            (
                "[agents.a]\nprogram = \"x\"\n[agents.a.title]\nbusy = \"x\"",
                "agents.a.title.busy is not a setting of an agent: they are working, ready, waiting",
            ),
            (
                "[agents.a]\nprogram = \"x\"\n[agents.a.title]\nready = \"\"",
                "agents.a.title.ready must be one line",
            ),
        ] {
            let said = error(text);
            assert!(said.contains(says), "{text:?}: {said}");
        }
    }
}
