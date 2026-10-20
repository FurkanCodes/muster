//! How the UI's panels are laid out (`PRODUCT.md` §6 Sessions): the list,
//! the session (or what the screen shows beside the list), a workspace's
//! changes, its shell and a plugin's panel, each where the person put it,
//! side by side or one above the other. It is a small tree of the five,
//! kept as one line of text in Muster's own settings (`[layout] panels`):
//!
//! ```text
//! list | session 60% | (changes / shell / plugin)
//! ```
//!
//! `|` puts two side by side and `/` one above the other; a run of one of
//! them nests to the right (`a | b | c` is `a | (b | c)`), and brackets
//! mix them. A panel or a bracket may say its size within the two it is
//! one of: a share (`60%`) or a width in columns (`30`). One without a
//! size shares evenly, but the list, which is as wide as its lines need.
//! A panel with nothing to show leaves its place to the other of its two.
//! A layout written before a panel was added still reads: the panel is put
//! where the default layout puts it, beside its neighbour there. Parsing
//! does no IO (`ARC-01`).

use std::fmt;

use serde::{Deserialize, Serialize};

/// The panels a window lays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Panel {
    /// The list of the screen shown: sessions, Needs you, tickets or
    /// settings.
    List,
    /// The selected session's terminal, or what the screen shows of the
    /// line selected.
    Session,
    /// A workspace's changes and its timeline.
    Changes,
    /// A shell in the session's workspace.
    Shell,
    /// A plugin's panel in the session's workspace.
    Plugin,
}

impl Panel {
    /// Every panel, in the order the default layout names them.
    pub const ALL: [Self; 5] = [
        Self::List,
        Self::Session,
        Self::Changes,
        Self::Shell,
        Self::Plugin,
    ];

    /// As the layout's text names it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Session => "session",
            Self::Changes => "changes",
            Self::Shell => "shell",
            Self::Plugin => "plugin",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|panel| panel.word() == word)
    }
}

/// How two panels share their place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Split {
    /// Side by side: `|`.
    Across,
    /// One above the other: `/`.
    Down,
}

impl Split {
    const fn sign(self) -> char {
        match self {
            Self::Across => '|',
            Self::Down => '/',
        }
    }
}

/// How much of the two it is one of a part takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    /// A share, in percent: 1 to 99.
    Share(u8),
    /// A width (or a height, one above the other) in cells.
    Cells(u16),
}

/// A panel, or two sharing a place, and the size it was given there.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub part: Part,
    pub size: Option<Size>,
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Panel(Panel),
    Split(Split, Box<Node>, Box<Node>),
}

/// Which of a split's two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Branch {
    First,
    Second,
}

/// A side of a panel or of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    const fn split(self) -> Split {
        match self {
            Self::Left | Self::Right => Split::Across,
            Self::Top | Self::Bottom => Split::Down,
        }
    }

    /// Whether what is put at this edge comes first.
    const fn first(self) -> bool {
        matches!(self, Self::Left | Self::Top)
    }
}

/// Where a panel moved to goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// Along an edge of the whole window.
    Window(Edge),
    /// At an edge of another panel, sharing its place.
    Beside(Panel, Edge),
}

/// The panels' layout. Every panel is in it once, shown or not.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Layout {
    root: Node,
}

/// What the person sees first: the list, the session beside it, and the
/// changes above the shell above a plugin's panel beside that.
pub const DEFAULT: &str = "list | session 60% | (changes / shell / plugin)";

impl Default for Layout {
    fn default() -> Self {
        Self::parse(DEFAULT).unwrap_or_else(|_| unreachable!("the default layout reads"))
    }
}

/// Why a layout does not read: said for the person, naming what is wrong.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("{0:?} is not a panel: the panels are list, session, changes, shell and plugin")]
    NotAPanel(String),
    #[error("{0} is in the layout twice")]
    Twice(&'static str),
    #[error("{0:?} is not a size: a size is a share like 60% or a width like 30")]
    NotASize(String),
    #[error("| and / are mixed without brackets around one of them")]
    Mixed,
    #[error("a bracket is not closed, or one is closed that was not opened")]
    Brackets,
    #[error("something is missing before or after a | or a /")]
    Incomplete,
}

impl Layout {
    /// Reads a layout as `[layout] panels` writes it. A panel it lacks,
    /// as one written before that panel was Muster's lacks it, is put
    /// where the default layout puts it.
    pub fn parse(text: &str) -> Result<Self, LayoutError> {
        let root = tree(text)?;
        let mut seen = Vec::new();
        collect(&root, &mut seen);
        for panel in Panel::ALL {
            if seen.iter().filter(|seen| **seen == panel).count() > 1 {
                return Err(LayoutError::Twice(panel.word()));
            }
        }
        let mut root = Node { size: None, ..root };
        for panel in Panel::ALL {
            if !contains(&root, panel) {
                root = completed(root, panel);
            }
        }
        Ok(Self { root })
    }

    /// The layout with `panel` taken out, for a client older than that
    /// panel, which would refuse a layout naming it: its place goes to the
    /// other of its two, as for a panel not shown. Read back, it is put
    /// where the default puts it.
    #[must_use]
    pub fn without(&self, panel: Panel) -> Self {
        match without(&self.root, panel) {
            Some((rest, _)) => Self {
                root: Node { size: None, ..rest },
            },
            None => self.clone(),
        }
    }

    /// The tree, from the whole window down.
    #[must_use]
    pub const fn root(&self) -> &Node {
        &self.root
    }

    /// The layout with `panel` moved to `place`. A panel moved beside
    /// itself stays where it is. It keeps a width in cells it had, as the
    /// list keeps the width the person dragged it to; a share it had was of
    /// the place it left.
    #[must_use]
    pub fn moved(&self, panel: Panel, place: Place) -> Self {
        if matches!(place, Place::Beside(other, _) if other == panel) {
            return self.clone();
        }
        let Some((rest, size)) = without(&self.root, panel) else {
            return self.clone();
        };
        let moving = Node {
            part: Part::Panel(panel),
            size: size.filter(|size| matches!(size, Size::Cells(_))),
        };
        let root = match place {
            Place::Window(edge) => joined(moving, rest, edge, None),
            Place::Beside(target, edge) => beside(rest, target, moving, edge),
        };
        Self {
            root: Node { size: None, ..root },
        }
    }

    /// The layout with the split at `path` (each step from the window
    /// down) sharing out its place by `size`, given to its `branch`; the
    /// other of the two takes what is left.
    #[must_use]
    pub fn sized(&self, path: &[Branch], branch: Branch, size: Size) -> Self {
        let mut root = self.root.clone();
        if let Some(Node {
            part: Part::Split(_, first, second),
            ..
        }) = at_path(&mut root, path)
        {
            let (given, other) = match branch {
                Branch::First => (first, second),
                Branch::Second => (second, first),
            };
            given.size = Some(size);
            other.size = None;
        }
        Self { root }
    }

    /// The path to the split that shares out a place between `first`'s
    /// panels and `second`'s, as a tree of the panels shown draws a line
    /// between them; `None` if no split does. The panels not shown may be
    /// on either side.
    #[must_use]
    pub fn split_between(&self, first: &[Panel], second: &[Panel]) -> Option<Vec<Branch>> {
        let mut path = Vec::new();
        let mut node = &self.root;
        loop {
            let Part::Split(_, one, other) = &node.part else {
                return None;
            };
            let holds =
                |node: &Node, panels: &[Panel]| panels.iter().all(|panel| contains(node, *panel));
            if holds(one, first) && holds(other, second) {
                return Some(path);
            }
            let all = || first.iter().chain(second);
            if all().all(|panel| contains(one, *panel)) {
                path.push(Branch::First);
                node = one;
            } else if all().all(|panel| contains(other, *panel)) {
                path.push(Branch::Second);
                node = other;
            } else {
                return None;
            }
        }
    }

    /// The panels that are shown, as a tree: the others left out, each
    /// leaving its place to the other of its two, which takes the size
    /// the two had. `None` when none is shown.
    #[must_use]
    pub fn shown(&self, shown: impl Fn(Panel) -> bool + Copy) -> Option<Node> {
        kept(&self.root, shown)
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_node(f, &self.root, None)
    }
}

impl From<Layout> for String {
    fn from(layout: Layout) -> Self {
        layout.to_string()
    }
}

impl TryFrom<String> for Layout {
    type Error = LayoutError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word(String),
    Size(Size),
    Sign(Split),
    Open,
    Close,
}

/// The tree `text` writes, whatever panels it has.
fn tree(text: &str) -> Result<Node, LayoutError> {
    let tokens = tokens(text)?;
    let mut at = 0;
    let root = node(&tokens, &mut at)?;
    if at != tokens.len() {
        return Err(match tokens[at] {
            Token::Close => LayoutError::Brackets,
            _ => LayoutError::Incomplete,
        });
    }
    Ok(root)
}

/// `root` with `panel`, which it lacks, where the default layout puts it:
/// beside the panel nearest it there that `root` has, on the same side of
/// it; along the window's right edge if the default says nothing.
fn completed(root: Node, panel: Panel) -> Node {
    let moving = Node {
        part: Part::Panel(panel),
        size: None,
    };
    let default = tree(DEFAULT).unwrap_or_else(|_| unreachable!("the default layout reads"));
    match place_in(&default, panel, &|other| contains(&root, other)) {
        Some((target, edge)) => beside(root, target, moving, edge),
        None => joined(moving, root, Edge::Right, None),
    }
}

/// Where `node` puts `panel`: at an edge of the panel nearest it in the
/// smallest part around it that holds one `present` says is there.
fn place_in(node: &Node, panel: Panel, present: &dyn Fn(Panel) -> bool) -> Option<(Panel, Edge)> {
    let Part::Split(split, first, second) = &node.part else {
        return None;
    };
    let (this, other, after) = if contains(first, panel) {
        (first, second, false)
    } else if contains(second, panel) {
        (second, first, true)
    } else {
        return None;
    };
    if let Some(place) = place_in(this, panel, present) {
        return Some(place);
    }
    let mut panels = Vec::new();
    collect(other, &mut panels);
    let nearest = if after {
        panels.into_iter().rev().find(|other| present(*other))
    } else {
        panels.into_iter().find(|other| present(*other))
    }?;
    let edge = match (split, after) {
        (Split::Across, true) => Edge::Right,
        (Split::Across, false) => Edge::Left,
        (Split::Down, true) => Edge::Bottom,
        (Split::Down, false) => Edge::Top,
    };
    Some((nearest, edge))
}

fn tokens(text: &str) -> Result<Vec<Token>, LayoutError> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {}
            '|' => tokens.push(Token::Sign(Split::Across)),
            '/' => tokens.push(Token::Sign(Split::Down)),
            '(' => tokens.push(Token::Open),
            ')' => tokens.push(Token::Close),
            _ => {
                let mut word = String::from(c);
                while let Some(next) = chars.peek()
                    && !matches!(next, ' ' | '\t' | '|' | '/' | '(' | ')')
                {
                    word.push(*next);
                    chars.next();
                }
                tokens.push(if word.starts_with(|c: char| c.is_ascii_digit()) {
                    Token::Size(size(&word)?)
                } else {
                    Token::Word(word.to_ascii_lowercase())
                });
            }
        }
    }
    Ok(tokens)
}

fn size(word: &str) -> Result<Size, LayoutError> {
    let not = || LayoutError::NotASize(word.to_owned());
    if let Some(share) = word.strip_suffix('%') {
        let share: u8 = share.parse().map_err(|_| not())?;
        return if (1..=99).contains(&share) {
            Ok(Size::Share(share))
        } else {
            Err(not())
        };
    }
    match word.parse::<u16>() {
        Ok(cells) if cells > 0 => Ok(Size::Cells(cells)),
        _ => Err(not()),
    }
}

/// A run of terms joined by one sign, nested to the right.
fn node(tokens: &[Token], at: &mut usize) -> Result<Node, LayoutError> {
    let first = term(tokens, at)?;
    let Some(Token::Sign(split)) = tokens.get(*at) else {
        return Ok(first);
    };
    let split = *split;
    let mut terms = vec![first];
    while let Some(Token::Sign(sign)) = tokens.get(*at) {
        if *sign != split {
            return Err(LayoutError::Mixed);
        }
        *at += 1;
        terms.push(term(tokens, at)?);
    }
    let mut nested = terms
        .pop()
        .unwrap_or_else(|| unreachable!("two terms at least"));
    while let Some(term) = terms.pop() {
        nested = Node {
            part: Part::Split(split, Box::new(term), Box::new(nested)),
            size: None,
        };
    }
    Ok(nested)
}

fn term(tokens: &[Token], at: &mut usize) -> Result<Node, LayoutError> {
    let part = match tokens.get(*at) {
        Some(Token::Word(word)) => {
            *at += 1;
            Panel::from_word(word)
                .map(Part::Panel)
                .ok_or_else(|| LayoutError::NotAPanel(word.clone()))?
        }
        Some(Token::Open) => {
            *at += 1;
            let inner = node(tokens, at)?;
            if tokens.get(*at) != Some(&Token::Close) {
                return Err(LayoutError::Brackets);
            }
            *at += 1;
            inner.part
        }
        Some(Token::Close) => return Err(LayoutError::Brackets),
        _ => return Err(LayoutError::Incomplete),
    };
    let size = match tokens.get(*at) {
        Some(Token::Size(size)) => {
            *at += 1;
            Some(*size)
        }
        _ => None,
    };
    Ok(Node { part, size })
}

fn collect(node: &Node, into: &mut Vec<Panel>) {
    match &node.part {
        Part::Panel(panel) => into.push(*panel),
        Part::Split(_, first, second) => {
            collect(first, into);
            collect(second, into);
        }
    }
}

/// `node` without `panel`, and the size `panel` had; `None` when `node`
/// is `panel` alone, or `panel` is not in it.
fn without(node: &Node, panel: Panel) -> Option<(Node, Option<Size>)> {
    let Part::Split(split, first, second) = &node.part else {
        return None;
    };
    for (this, other, first_is_this) in [(first, second, true), (second, first, false)] {
        if this.part == Part::Panel(panel) {
            return Some((
                Node {
                    part: other.part.clone(),
                    size: node.size,
                },
                this.size,
            ));
        }
        if let Some((rest, size)) = without(this, panel) {
            let (first, second) = if first_is_this {
                (rest, (**other).clone())
            } else {
                ((**other).clone(), rest)
            };
            return Some((
                Node {
                    part: Part::Split(*split, Box::new(first), Box::new(second)),
                    size: node.size,
                },
                size,
            ));
        }
    }
    None
}

/// `moving` and `staying` sharing `staying`'s place, `moving` at `edge`
/// of it, the two taking `size` where they are.
fn joined(moving: Node, staying: Node, edge: Edge, size: Option<Size>) -> Node {
    let staying = Node {
        size: None,
        ..staying
    };
    let (first, second) = if edge.first() {
        (moving, staying)
    } else {
        (staying, moving)
    };
    Node {
        part: Part::Split(edge.split(), Box::new(first), Box::new(second)),
        size,
    }
}

/// `node` with `moving` put at `edge` of `target`.
fn beside(node: Node, target: Panel, moving: Node, edge: Edge) -> Node {
    match node.part {
        Part::Panel(panel) if panel == target => {
            let size = node.size;
            joined(moving, node, edge, size)
        }
        Part::Panel(_) => node,
        Part::Split(split, first, second) => {
            let (first, second) = if contains(&first, target) {
                (beside(*first, target, moving, edge), *second)
            } else {
                (*first, beside(*second, target, moving, edge))
            };
            Node {
                part: Part::Split(split, Box::new(first), Box::new(second)),
                size: node.size,
            }
        }
    }
}

fn contains(node: &Node, panel: Panel) -> bool {
    match &node.part {
        Part::Panel(this) => *this == panel,
        Part::Split(_, first, second) => contains(first, panel) || contains(second, panel),
    }
}

fn at_path<'a>(node: &'a mut Node, path: &[Branch]) -> Option<&'a mut Node> {
    let Some((step, rest)) = path.split_first() else {
        return Some(node);
    };
    match &mut node.part {
        Part::Split(_, first, second) => at_path(
            match step {
                Branch::First => first,
                Branch::Second => second,
            },
            rest,
        ),
        Part::Panel(_) => None,
    }
}

fn kept(node: &Node, shown: impl Fn(Panel) -> bool + Copy) -> Option<Node> {
    match &node.part {
        Part::Panel(panel) => shown(*panel).then(|| node.clone()),
        Part::Split(split, first, second) => match (kept(first, shown), kept(second, shown)) {
            (Some(first), Some(second)) => Some(Node {
                part: Part::Split(*split, Box::new(first), Box::new(second)),
                size: node.size,
            }),
            (Some(only), None) | (None, Some(only)) => Some(Node {
                size: node.size,
                ..only
            }),
            (None, None) => None,
        },
    }
}

/// Writes `node`, inside a run of `within`'s sign if it is one.
fn write_node(f: &mut fmt::Formatter<'_>, node: &Node, within: Option<Split>) -> fmt::Result {
    match &node.part {
        Part::Panel(panel) => f.write_str(panel.word())?,
        Part::Split(split, first, second) => {
            // A run of one sign, nesting to the right, needs no brackets.
            let bracketed = within.is_some();
            if bracketed {
                f.write_str("(")?;
            }
            write_node(f, first, Some(*split))?;
            write!(f, " {} ", split.sign())?;
            let runs_on = matches!(second.part, Part::Split(next, ..) if next == *split)
                && second.size.is_none();
            if runs_on {
                write_run(f, second, *split)?;
            } else {
                write_node(f, second, Some(*split))?;
            }
            if bracketed {
                f.write_str(")")?;
            }
        }
    }
    match node.size {
        Some(Size::Share(share)) => write!(f, " {share}%"),
        Some(Size::Cells(cells)) => write!(f, " {cells}"),
        None => Ok(()),
    }
}

/// Writes the rest of a run of `split`'s sign, without brackets.
fn write_run(f: &mut fmt::Formatter<'_>, node: &Node, split: Split) -> fmt::Result {
    let Part::Split(_, first, second) = &node.part else {
        return write_node(f, node, Some(split));
    };
    write_node(f, first, Some(split))?;
    write!(f, " {} ", split.sign())?;
    let runs_on =
        matches!(second.part, Part::Split(next, ..) if next == split) && second.size.is_none();
    if runs_on {
        write_run(f, second, split)
    } else {
        write_node(f, second, Some(split))
    }
}

#[cfg(test)]
mod tests;
