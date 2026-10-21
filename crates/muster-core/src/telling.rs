//! What the person chooses to be told of, and how it sounds (`PRODUCT.md`
//! §6): questions, finishes and errors, each on or off with a sound of its
//! own (`[notifications]` in Muster's own settings). From protocol 40.
//! Pure (`ARC-01`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Session, SessionStatus};

/// What a notification is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A session asks something of the person: a question, a permission,
    /// a form.
    Question,
    /// An agent's turn finished, or its program ended well.
    Finish,
    /// Its program ended with an error.
    Error,
}

impl Kind {
    /// Each kind, in the order the Settings screen lists them.
    pub const ALL: [Self; 3] = [Self::Question, Self::Finish, Self::Error];

    /// What `session`, which needs the person, would be told as.
    pub fn of(session: &Session) -> Self {
        match session.status {
            SessionStatus::NeedsYou { .. } => Self::Question,
            SessionStatus::Exited { success: false } => Self::Error,
            _ => Self::Finish,
        }
    }

    /// Its key in `[notifications]`; its sound's is this and `_sound`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Question => "questions",
            Self::Finish => "finishes",
            Self::Error => "errors",
        }
    }

    /// What the Settings screen calls it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Question => "Questions",
            Self::Finish => "Finishes",
            Self::Error => "Errors",
        }
    }
}

/// The sounds every Mac has (`/System/Library/Sounds`), which the Settings
/// screen offers; Muster's app carries a copy, as a notification's sound
/// is looked for in the app and the person's own sounds only.
pub const MAC_SOUNDS: [&str; 14] = [
    "Basso",
    "Blow",
    "Bottle",
    "Frog",
    "Funk",
    "Glass",
    "Hero",
    "Morse",
    "Ping",
    "Pop",
    "Purr",
    "Sosumi",
    "Submarine",
    "Tink",
];

/// The longest sound name read.
const NAME_MOST: usize = 64;

/// How a notification sounds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
#[serde(try_from = "String", into = "String")]
pub enum Sound {
    /// The Mac's own notification sound.
    #[default]
    Default,
    /// None.
    Silent,
    /// One by name: one of [`MAC_SOUNDS`], or a file in the person's
    /// `~/Library/Sounds` (`notification.wav`).
    Named(String),
}

impl Sound {
    /// The sound `text` names, as written in `[notifications]`: `default`,
    /// `none`, or a name with no folder in it.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "default" => Ok(Self::Default),
            "none" => Ok(Self::Silent),
            name if name.is_empty()
                || name.chars().count() > NAME_MOST
                || name.contains(['/', '\\'])
                || name.starts_with('.')
                || name.chars().any(char::is_control) =>
            {
                Err(format!(
                    "{text:?} is not a sound: \"default\", \"none\", or a sound's name \
                     (Glass, or a file in ~/Library/Sounds)"
                ))
            }
            name => Ok(Self::Named(name.to_owned())),
        }
    }

    /// As written in `[notifications]`.
    pub fn key(&self) -> &str {
        match self {
            Self::Default => "default",
            Self::Silent => "none",
            Self::Named(name) => name,
        }
    }

    /// The sound `by` steps on from this one, round the Mac's default,
    /// none, and the Mac's own sounds; one of the person's own goes to the
    /// first.
    #[must_use]
    pub fn step(&self, by: isize) -> Self {
        let mut all = vec![Self::Default, Self::Silent];
        all.extend(
            MAC_SOUNDS
                .iter()
                .map(|name| Self::Named((*name).to_owned())),
        );
        let Some(at) = all.iter().position(|sound| sound == self) else {
            return Self::Default;
        };
        let count = isize::try_from(all.len()).unwrap_or(1);
        let at = isize::try_from(at).unwrap_or(0);
        let next = usize::try_from((at + by).rem_euclid(count)).unwrap_or(0);
        all.swap_remove(next)
    }
}

impl fmt::Display for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("the Mac's sound"),
            Self::Silent => f.write_str("silent"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

impl TryFrom<String> for Sound {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<Sound> for String {
    fn from(sound: Sound) -> Self {
        sound.key().to_owned()
    }
}

/// Whether one kind is told, and how it sounds.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Tell {
    pub on: bool,
    #[serde(default)]
    pub sound: Sound,
}

impl Default for Tell {
    fn default() -> Self {
        Self {
            on: true,
            sound: Sound::Default,
        }
    }
}

/// What the person is told of, each kind on unless they turn it off, with
/// the Mac's own sound unless they pick another.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Telling {
    #[serde(default)]
    pub questions: Tell,
    #[serde(default)]
    pub finishes: Tell,
    #[serde(default)]
    pub errors: Tell,
}

impl Telling {
    /// How `kind` is told.
    pub const fn of(&self, kind: Kind) -> &Tell {
        match kind {
            Kind::Question => &self.questions,
            Kind::Finish => &self.finishes,
            Kind::Error => &self.errors,
        }
    }

    /// `kind` told as `tell`, the rest as they are.
    #[must_use]
    pub fn with(mut self, kind: Kind, tell: Tell) -> Self {
        match kind {
            Kind::Question => self.questions = tell,
            Kind::Finish => self.finishes = tell,
            Kind::Error => self.errors = tell,
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_reads_as_written_and_a_path_never() {
        assert_eq!(Sound::parse("default"), Ok(Sound::Default));
        assert_eq!(Sound::parse("none"), Ok(Sound::Silent));
        assert_eq!(Sound::parse("Glass"), Ok(Sound::Named("Glass".to_owned())));
        assert_eq!(
            Sound::parse("notification.wav"),
            Ok(Sound::Named("notification.wav".to_owned()))
        );
        for not_one in [
            "",
            "../x.aiff",
            "/System/Library/Sounds/Glass.aiff",
            ".hidden",
            "a\nb",
        ] {
            assert!(Sound::parse(not_one).is_err(), "{not_one:?}");
        }
        assert_eq!(Sound::Named("Glass".to_owned()).key(), "Glass");
        assert_eq!(Sound::Silent.to_string(), "silent");
    }

    #[test]
    fn the_sounds_go_round_both_ways_and_one_of_the_person_s_to_the_first() {
        assert_eq!(Sound::Default.step(1), Sound::Silent);
        assert_eq!(Sound::Silent.step(1), Sound::Named("Basso".to_owned()));
        assert_eq!(Sound::Default.step(-1), Sound::Named("Tink".to_owned()));
        assert_eq!(Sound::Named("Tink".to_owned()).step(1), Sound::Default);
        assert_eq!(Sound::Named("mine.wav".to_owned()).step(1), Sound::Default);
    }

    #[test]
    fn each_kind_is_told_on_its_own() {
        let telling = Telling::default().with(
            Kind::Finish,
            Tell {
                on: false,
                sound: Sound::Silent,
            },
        );
        assert!(telling.of(Kind::Question).on);
        assert!(!telling.of(Kind::Finish).on);
        assert_eq!(telling.of(Kind::Error).sound, Sound::Default);
        assert_eq!(Kind::Error.key(), "errors");
    }
}
