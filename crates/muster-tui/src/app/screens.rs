//! What the window keeps for each of its screens and parts, apart from the
//! daemon's state: what is selected and in view on *Needs you*, *Tickets*
//! and *Settings*, the panels as the person moves them, and the news held
//! to tell.

use std::collections::HashMap;

use muster_core::layout::Layout;
use muster_core::{ProjectId, SessionId, TicketKey};

use super::{
    HarnessChoice, PluginKey, SettingRow, Settings, TicketRow, TicketText, Tickets, drag, grab,
};

/// *Needs you*.
#[derive(Debug, Default)]
pub(crate) struct NeedsScreen {
    /// The first line shown: the view moves only when the selection would
    /// leave it.
    pub(crate) offset: usize,
    /// The queue as shown: a session stays in place while it is selected,
    /// even once looking at it has cleared it, so the list does not move
    /// under the person.
    pub(crate) list: Vec<SessionId>,
    /// The selected session.
    pub(crate) selected: Option<SessionId>,
}

/// *Tickets*.
#[derive(Debug)]
pub(crate) struct TicketsScreen {
    /// Each project's tickets, as last fetched; fetched again on `r`.
    pub(crate) by_project: HashMap<ProjectId, Tickets>,
    /// The selected line, and the first line shown.
    pub(crate) selected: Option<TicketRow>,
    pub(crate) offset: usize,
    /// How far the selected ticket's details are paged down, and for which
    /// line: another line selected shows its own from the top.
    pub(crate) scroll: Option<(TicketRow, usize)>,
    /// The part of the selected ticket the keys are on (`]`, `[`), for
    /// which line, as `scroll`.
    pub(crate) part: Option<(TicketRow, usize)>,
    /// What of each ticket an agent is given, as the person chose (`space`).
    pub(crate) briefs: HashMap<(ProjectId, TicketKey), muster_core::TicketBrief>,
    /// Tickets' text, fetched as each is first shown.
    pub(crate) texts: HashMap<(ProjectId, TicketKey), TicketText>,
    /// The agent last started on a ticket, which the next ticket is
    /// offered first.
    pub(crate) agent: HarnessChoice,
    /// The project last chosen for an agent on a ticket that several
    /// projects share, by where their tickets come from: offered first
    /// next time.
    pub(crate) projects: HashMap<String, ProjectId>,
}

impl Default for TicketsScreen {
    fn default() -> Self {
        Self {
            by_project: HashMap::new(),
            selected: None,
            offset: 0,
            scroll: None,
            part: None,
            briefs: HashMap::new(),
            texts: HashMap::new(),
            agent: HarnessChoice::CLAUDE,
            projects: HashMap::new(),
        }
    }
}

/// *Settings*.
#[derive(Debug, Default)]
pub(crate) struct SettingsScreen {
    /// Each project's settings, as last read; read again each time the
    /// screen is shown, and on `r`.
    pub(crate) by_project: HashMap<ProjectId, Settings>,
    /// The selected line, and the first line shown.
    pub(crate) selected: Option<SettingRow>,
    pub(crate) offset: usize,
    /// The plugin being turned on or off: the answer is on its way.
    pub(crate) plugin_changing: Option<PluginKey>,
    /// A plugin just installed, selected once the list has it.
    pub(crate) plugin_wanted: Option<PluginKey>,
}

/// The panels as the person moves them.
#[derive(Debug, Default)]
pub(crate) struct Panels {
    /// The panels as the person moved them, until the daemon keeps them,
    /// or for as long as the window lasts where it cannot.
    pub(crate) kept: Option<Layout>,
    /// The list folded away or brought back, the same way.
    pub(crate) folded_kept: Option<bool>,
    /// A panel or the line between two being dragged.
    pub(crate) drag: Option<drag::Drag>,
    /// What the pointer is over that a drag would move, lit.
    pub(crate) hover: Option<grab::Grab>,
    /// The panels being moved with the keys.
    pub(crate) arranging: Option<drag::Arranging>,
    /// The sessions placed in panels of their own that are streamed, and
    /// the size each was given.
    pub(crate) placed: Vec<super::settle::Streamed>,
    /// The last two sessions the list showed that were not placed, the
    /// latest first: the preview keeps one while a placed one is selected
    /// (`session_beside_list`). Two, as a line dragged out of the list is
    /// selected as it is pressed, before it is placed.
    pub(crate) agents: [Option<SessionId>; 2],
    /// The session whose preview the person folded away by its `✕`: the
    /// preview shows again once the list selects another, or they type in
    /// it.
    pub(crate) preview_folded: Option<SessionId>,
}

/// The news told the person while they look elsewhere (`app::alarm`).
#[derive(Debug)]
pub(crate) struct Alarms {
    /// Whether to notify the person of news while they look elsewhere.
    pub(crate) notify: bool,
    /// Finishes not told yet, each with the step it came at: held a
    /// moment, in case the session goes straight back to work. One for
    /// each session, gone once told or dropped.
    pub(crate) held: Vec<(SessionId, u64)>,
    /// The session told of last, which `l` goes to.
    pub(crate) last: Option<SessionId>,
}

impl Default for Alarms {
    fn default() -> Self {
        Self {
            notify: true,
            held: Vec::new(),
            last: None,
        }
    }
}

/// What moves on screen (`app::motion`).
#[derive(Debug)]
pub(crate) struct Moving {
    /// Whether anything moves: running sessions' marks spin and what
    /// needs the person pulses.
    pub(crate) on: bool,
    /// The step of the motion to draw, counted out by the runtime.
    pub(crate) step: u64,
    /// Sessions whose lines flash for a change of status, with the step
    /// each flash began.
    pub(crate) flashes: HashMap<SessionId, u64>,
    /// The step a session last finished at, while Kep is glad of it.
    pub(crate) glad_at: Option<u64>,
    /// The selected line whose name, cut short, slides along.
    pub(super) slide: Option<super::motion::Slide>,
}

impl Default for Moving {
    fn default() -> Self {
        Self {
            on: true,
            step: 0,
            flashes: HashMap::new(),
            glad_at: None,
            slide: None,
        }
    }
}

/// The person's terminal, and the machine it is on, as they said as the
/// UI started and since.
#[derive(Debug)]
pub(crate) struct Person {
    /// Whether the person's terminal window has focus, once it has said;
    /// terminals that never say count as looking.
    pub(crate) focused: Option<bool>,
    /// What the person's terminal draws: the agents' logos as pictures,
    /// or their marks in text.
    pub(crate) graphics: crate::view::logos::Graphics,
    /// Whether Muster's notch can show here: a Mac, with a program to
    /// start it.
    pub(crate) notch_here: bool,
    /// The colours of the person's terminal, as it said them as the UI
    /// started, told to each daemon the UI connects to.
    pub(crate) colours: muster_core::colours::TerminalColours,
    /// One cell of the person's terminal in pixels, as its size says,
    /// read as the UI starts and as the window is resized.
    pub(crate) cell_pixels: Option<muster_core::protocol::PixelSize>,
}

impl Default for Person {
    fn default() -> Self {
        Self {
            focused: None,
            graphics: crate::view::logos::Graphics::Text,
            notch_here: false,
            colours: muster_core::colours::TerminalColours::default(),
            cell_pixels: None,
        }
    }
}

/// The commands sent and what follows each one's reply.
#[derive(Debug)]
pub(crate) struct Requests {
    pub(super) pending: HashMap<u64, super::Then>,
    /// The id the next command is sent with.
    pub(crate) next: u64,
}

impl Default for Requests {
    fn default() -> Self {
        Self {
            pending: HashMap::new(),
            next: 1,
        }
    }
}
