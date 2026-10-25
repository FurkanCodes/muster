//! Spaces and free terminals in the terminal UI (`PRODUCT.md` §6,
//! `ARCHITECTURE.md` §5a). Everything lives in one space (`ARC-33`). The
//! list shows every space under its heading, or one: which is this UI's
//! own, never the daemon's or kept, and what a space lists is the model's
//! (`muster_model::Model::rows_in`). `[` `]` go round the spaces; `W` opens
//! a menu of them, and of what the selected line can do with them; `+`
//! starts a terminal in no project; `F` puts one under a project, where its
//! shell is or started again there, never typing into it. Something new
//! goes in the selected line's space; with no space yet, one is named first.

use crossterm::event::KeyEvent;
use muster_core::protocol::{Command, ErrorBody, Reply};
use muster_core::spaces::Member;
use muster_core::{SessionId, Space, SpaceId, WorkspaceId};

use super::menu::Menu;
use super::{Action, App, Confirmed, Dialog, Here, Mode, Then};
use crate::input::{Edit, LineInput};
use crate::model::{Row, View as ListView};

/// What a space menu's item does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    /// Show this space, or all of the list.
    GoTo(Option<SpaceId>),
    New,
    Rename(SpaceId),
    Delete(SpaceId),
    /// Move a project or a free terminal to the space.
    Move(SpaceId, Member),
    /// Make a space, then do this in it.
    NewThen(Later),
    /// Do this in the selected line's space.
    Here(Later),
    /// File the free terminal under the workspace its shell is in.
    FileUnder(SessionId, WorkspaceId),
    /// Add the folder its shell is in as a project, the terminal under it.
    AddHere(SessionId),
    /// Start it again in the workspace, ending what runs in it (named).
    RestartIn(SessionId, WorkspaceId, Option<String>),
}

/// What waits on a space to go in: asked for while there was none, or
/// for a space made for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Later {
    Terminal,
    AddProject,
    CloneProject,
    Move(Member),
}

/// What a reply about spaces is waited for to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Waiting {
    /// Show the space just made, or do this in it.
    Made(Option<Later>),
    /// Offer what the free terminal can be filed under.
    Place(SessionId),
    /// Say this once it is done.
    Done(&'static str),
}

impl App {
    /// The space shown, if it is still there.
    pub(crate) fn space_shown(&self) -> Option<&Space> {
        self.space.and_then(|id| self.model.space(id))
    }

    /// The space shown, by its id.
    pub(crate) fn space_id(&self) -> Option<SpaceId> {
        self.space_shown().map(|space| space.id)
    }

    /// The space something new goes in: the selected line's, else the one
    /// shown, else the first; none while there is no space.
    pub(crate) fn space_to_join(&self) -> Option<SpaceId> {
        self.selected
            .and_then(|row| self.model.space_of(row))
            .or_else(|| self.space_id())
            .or_else(|| self.model.spaces().first().map(|space| space.id))
    }

    /// With no space yet, asks for one's name, `later` waiting on it; then
    /// true, and what was asked waits.
    pub(super) fn space_first(&mut self, later: Later) -> bool {
        if !self.model.spaces().is_empty() {
            return false;
        }
        self.mode = Mode::Dialog(Dialog::SpaceName {
            space: None,
            name: LineInput::default(),
            then: Some(later),
        });
        true
    }

    /// Does `later`, a space there to go in, though its delta may come
    /// after the reply that made it.
    fn do_later(&mut self, later: Later, space: SpaceId) -> Vec<Action> {
        match later {
            Later::Terminal => self.free_terminal_in(space),
            Later::AddProject => self.add_project_dialog(),
            Later::CloneProject => self.clone_project_dialog(),
            Later::Move(member) => vec![self.command(
                Command::MoveToSpace { space, member },
                Then::Space(Waiting::Done("Moved to the space.")),
            )],
        }
    }

    /// What the sessions list shows.
    pub(crate) fn list_view(&self) -> ListView {
        self.space_id().map_or(ListView::All, ListView::Space)
    }

    /// Shows `space`, or all of the list.
    pub(super) fn go_to_space(&mut self, space: Option<SpaceId>) -> Vec<Action> {
        self.space = space;
        let name = self
            .space_shown()
            .map_or("everything".to_owned(), |space| one_line(&space.name));
        self.say(format!("Showing {name}."));
        if self.selected.is_some_and(|row| !self.rows().contains(&row)) {
            self.selected = self.rows().first().copied();
        }
        self.settle()
    }

    /// `]` and `[`: the next space, or the one before; all of the list
    /// comes after the last.
    pub(super) fn cycle_space(&mut self, by: isize) -> Vec<Action> {
        let mut round: Vec<Option<SpaceId>> = vec![None];
        round.extend(self.model.spaces().iter().map(|space| Some(space.id)));
        if round.len() == 1 {
            self.complain(format!(
                "There is no space yet: press {} to make one.",
                self.key(muster_core::keys::Action::Spaces)
            ));
            return Vec::new();
        }
        let at = round
            .iter()
            .position(|space| *space == self.space_id())
            .unwrap_or(0);
        let next = if by > 0 {
            (at + 1) % round.len()
        } else {
            (at + round.len() - 1) % round.len()
        };
        self.go_to_space(round[next])
    }

    /// What the selected line is, as something a space can hold: a
    /// project, or a terminal in no project.
    fn member_here(&self) -> Option<(Member, String)> {
        match self.selected? {
            Row::Project(id) => {
                let project = self.model.project(id)?;
                Some((Member::Project(id), one_line(&project.name)))
            }
            Row::Session(id) => {
                let session = self.model.session(id).filter(|session| session.is_free())?;
                Some((Member::Session(id), one_line(&session.title)))
            }
            Row::Worktree(_) | Row::Space(_) => None,
        }
    }

    /// The space a heading selected stands for, else the one shown: what
    /// the menu renames or removes.
    fn space_here(&self) -> Option<&Space> {
        match self.selected {
            Some(Row::Space(id)) => self.model.space(id),
            _ => self.space_shown(),
        }
    }

    /// `S`: the spaces, and what can be done with them here.
    pub(super) fn open_spaces_menu(&mut self) -> Vec<Action> {
        let shown = self.space_id();
        let mark = |on: bool| if on { "•" } else { " " }.to_owned();
        let mut items = vec![choice(
            mark(shown.is_none()),
            "everything".to_owned(),
            Choice::GoTo(None),
        )];
        for space in self.model.spaces() {
            items.push(choice(
                mark(shown == Some(space.id)),
                one_line(&space.name),
                Choice::GoTo(Some(space.id)),
            ));
        }
        items.push(choice(" ".to_owned(), "new space…".to_owned(), Choice::New));
        if let Some(space) = self.space_here() {
            let (id, name) = (space.id, one_line(&space.name));
            if matches!(self.selected, Some(Row::Space(_))) {
                for (later, what) in [
                    (Later::AddProject, "add a project"),
                    (Later::CloneProject, "clone from GitHub"),
                    (Later::Terminal, "a terminal"),
                ] {
                    items.push(choice(
                        "+".to_owned(),
                        format!("{what} in {name}…"),
                        Choice::Here(later),
                    ));
                }
            }
            items.push(choice(
                " ".to_owned(),
                format!("rename {name}…"),
                Choice::Rename(id),
            ));
            items.push(choice(
                " ".to_owned(),
                format!("remove {name}…"),
                Choice::Delete(id),
            ));
        }
        if let Some((member, what)) = self.member_here() {
            for space in self.model.spaces() {
                if !space.holds(member) {
                    items.push(choice(
                        "→".to_owned(),
                        format!("move {what} to {}", one_line(&space.name)),
                        Choice::Move(space.id, member),
                    ));
                }
            }
            items.push(choice(
                "→".to_owned(),
                format!("move {what} to a new space…"),
                Choice::NewThen(Later::Move(member)),
            ));
        }
        self.open_choices(items);
        Vec::new()
    }

    fn open_choices(&mut self, items: Vec<Here>) {
        self.note = None;
        self.mode = Mode::Menu(Menu {
            at: (2, 2),
            items,
            chosen: 0,
            on: None,
        });
    }

    /// `+`: a terminal in no project, in the home folder, in the selected
    /// line's space; with no space yet, one is named first.
    pub(super) fn new_free_terminal(&mut self) -> Vec<Action> {
        if self.space_first(Later::Terminal) {
            return Vec::new();
        }
        match self.space_to_join() {
            Some(space) => self.free_terminal_in(space),
            None => Vec::new(),
        }
    }

    fn free_terminal_in(&mut self, space: SpaceId) -> Vec<Action> {
        let command = Command::StartSession {
            workspace: None,
            harness: muster_core::harness::SHELL.to_owned(),
            title: None,
            size: self.session_size(),
            prompt: None,
            started_by: None,
            space: Some(space),
        };
        vec![self.command(command, Then::OpenSession)]
    }

    /// `F`: asks where the selected free terminal's shell is, to offer
    /// what it can be put under.
    pub(super) fn ask_place(&mut self) -> Vec<Action> {
        let free = match self.selected {
            Some(Row::Session(id)) => self.model.session(id).filter(|s| s.is_free()),
            _ => None,
        };
        let Some(session) = free.map(|session| session.id) else {
            self.complain("Select a terminal in no project, under its space, to put it under one.");
            return Vec::new();
        };
        vec![self.command(
            Command::ReadPlace { session },
            Then::Space(Waiting::Place(session)),
        )]
    }

    /// Where a free terminal's shell is: what it can be put under.
    fn offer_places(
        &mut self,
        session: SessionId,
        (folder, workspace, front): (
            Option<std::path::PathBuf>,
            Option<WorkspaceId>,
            Option<String>,
        ),
    ) {
        let mut items = Vec::new();
        if let Some(workspace) = workspace {
            let name = self.workspace_name(workspace);
            items.push(choice(
                "⏎".to_owned(),
                format!("file under {name}, as it is"),
                Choice::FileUnder(session, workspace),
            ));
        } else if let Some(folder) = &folder {
            items.push(choice(
                "+".to_owned(),
                format!(
                    "add {} as a project, the terminal under it",
                    folder.display()
                ),
                Choice::AddHere(session),
            ));
        }
        for project in self.model.projects() {
            // Its own checkout, else its first worktree on this machine.
            let first = self
                .model
                .workspaces_of(project.id)
                .find(|found| found.kind.host_path().is_some())
                .map(|found| found.id);
            if let Some(checkout) = first
                && Some(checkout) != workspace
            {
                items.push(choice(
                    " ".to_owned(),
                    format!("start again in {}…", one_line(&project.name)),
                    Choice::RestartIn(session, checkout, front.clone()),
                ));
            }
        }
        if items.is_empty() {
            self.complain("There is no project to put it under: add one first.");
            return;
        }
        self.open_choices(items);
    }

    fn workspace_name(&self, workspace: WorkspaceId) -> String {
        let project = self
            .model
            .workspace(workspace)
            .and_then(|found| self.model.project(found.project))
            .map_or("?".to_owned(), |project| one_line(&project.name));
        match self.model.branch(workspace) {
            Some(branch) if !branch.is_empty() && !self.is_checkout(workspace) => {
                format!("{project} ({branch})")
            }
            _ => project,
        }
    }

    fn is_checkout(&self, workspace: WorkspaceId) -> bool {
        self.model
            .workspace(workspace)
            .is_some_and(|found| found.kind.is_checkout())
    }

    /// What choosing `choice` in a space menu does.
    pub(super) fn choose_space(&mut self, choice: Choice) -> Vec<Action> {
        self.mode = Mode::Browse;
        let (command, then) = match choice {
            Choice::GoTo(space) => return self.go_to_space(space),
            Choice::New | Choice::NewThen(_) => {
                self.mode = Mode::Dialog(Dialog::SpaceName {
                    space: None,
                    name: LineInput::default(),
                    then: match choice {
                        Choice::NewThen(later) => Some(later),
                        _ => None,
                    },
                });
                return Vec::new();
            }
            Choice::Here(later) => {
                return match self.space_to_join() {
                    Some(space) => self.do_later(later, space),
                    None => Vec::new(),
                };
            }
            Choice::Rename(space) => {
                let name = self.model.space(space).map_or("", |found| &found.name);
                self.mode = Mode::Dialog(Dialog::SpaceName {
                    space: Some(space),
                    name: LineInput::with(name),
                    then: None,
                });
                return Vec::new();
            }
            Choice::Delete(space) => {
                let name = self
                    .model
                    .space(space)
                    .map_or("the space".to_owned(), |found| one_line(&found.name));
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: format!("Remove {name}? Only an empty space goes."),
                    then: Confirmed::DeleteSpace(space),
                });
                return Vec::new();
            }
            Choice::RestartIn(session, workspace, front) => {
                let ends =
                    front.map_or("Its shell ends".to_owned(), |front| format!("{front} ends"));
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: format!(
                        "Start it again in {}? {ends} (background jobs aren't listed).",
                        self.workspace_name(workspace)
                    ),
                    then: Confirmed::RestartIn { session, workspace },
                });
                return Vec::new();
            }
            Choice::Move(space, member) => (
                Command::MoveToSpace { space, member },
                Then::Space(Waiting::Done("Moved to the space.")),
            ),
            Choice::FileUnder(session, workspace) => (
                Command::FileSession { session, workspace },
                Then::Space(Waiting::Done("Filed under the project, still running.")),
            ),
            // The project goes in the terminal's space.
            Choice::AddHere(session) => (
                Command::AddProjectHere {
                    session,
                    space: None,
                },
                Then::SelectProject,
            ),
        };
        vec![self.command(command, then)]
    }

    /// A key in the dialog naming a space.
    pub(super) fn space_name_key(
        &mut self,
        space: Option<SpaceId>,
        mut name: LineInput,
        later: Option<Later>,
        key: &KeyEvent,
    ) -> Vec<Action> {
        match name.handle(key) {
            Edit::Submit if name.text().trim().is_empty() => {
                self.mode = Mode::Dialog(Dialog::SpaceName {
                    space,
                    name,
                    then: later,
                });
                Vec::new()
            }
            Edit::Submit => {
                self.mode = Mode::Browse;
                let name = name.text().trim().to_owned();
                let (command, then) = match space {
                    None => (Command::CreateSpace { name }, Waiting::Made(later)),
                    Some(space) => (
                        Command::RenameSpace { space, name },
                        Waiting::Done("Renamed."),
                    ),
                };
                vec![self.command(command, Then::Space(then))]
            }
            edit => self.keep_editing(
                edit,
                Dialog::SpaceName {
                    space,
                    name,
                    then: later,
                },
            ),
        }
    }

    /// What a confirmed question about spaces does.
    pub(super) fn confirmed_space(&mut self, then: &Confirmed) -> Option<Action> {
        Some(match *then {
            Confirmed::DeleteSpace(space) => self.command(
                Command::DeleteSpace { space },
                Then::Space(Waiting::Done("Removed the space.")),
            ),
            Confirmed::RestartIn { session, workspace } => {
                self.command(Command::RestartIn { session, workspace }, Then::OpenSession)
            }
            _ => return None,
        })
    }

    /// A reply about spaces.
    pub(super) fn space_reply(
        &mut self,
        waiting: Waiting,
        outcome: Result<Reply, ErrorBody>,
    ) -> Vec<Action> {
        match (waiting, outcome) {
            (_, Err(error)) => self.complain(error.message),
            (Waiting::Made(None), Ok(Reply::SpaceCreated { id })) => {
                return self.go_to_space(Some(id));
            }
            (Waiting::Made(Some(later)), Ok(Reply::SpaceCreated { id })) => {
                return self.do_later(later, id);
            }
            (
                Waiting::Place(session),
                Ok(Reply::Place {
                    folder,
                    workspace,
                    front,
                }),
            ) => self.offer_places(session, (folder, workspace, front)),
            (Waiting::Done(text), Ok(_)) => self.say(text),
            (_, Ok(_)) => {}
        }
        Vec::new()
    }

    /// The menu items about spaces on a free terminal's line: a project's
    /// are in the spaces menu (`W`), which the list's empty part offers.
    pub(super) fn spaces_here(&self, here: &mut Vec<Here>) {
        use muster_core::keys::Action as Does;
        if matches!(self.member_here(), Some((Member::Session(_), _))) {
            here.push(self.here_does(Does::FileTerminal, "under a project"));
            here.push(self.here_does(Does::Spaces, "spaces"));
        }
    }

    /// What a space's heading offers: what can go in it, and its menu.
    pub(super) fn space_heading_here(&self) -> Vec<Here> {
        use muster_core::keys::Action as Does;
        vec![
            self.here_does(Does::AddProject, "add"),
            self.here_does(Does::CloneProject, "clone"),
            self.here_does(Does::NewTerminal, "terminal"),
            self.here_does(Does::Spaces, "space"),
        ]
    }

    fn here_does(&self, does: muster_core::keys::Action, what: &str) -> Here {
        Here {
            does: Some(does),
            panel: None,
            key: self.keymap.label(does),
            what: what.to_owned(),
            choice: None,
        }
    }

    /// The Sessions tab's name: with the space it shows.
    pub(crate) fn sessions_tab_name(&self) -> String {
        match self.space_shown() {
            Some(space) => format!("Sessions · {}", one_line(&space.name)),
            None => "Sessions".to_owned(),
        }
    }
}

fn choice(key: String, what: String, choice: Choice) -> Here {
    Here {
        does: None,
        panel: None,
        key,
        what,
        choice: Some(choice),
    }
}

fn one_line(text: &str) -> String {
    muster_core::text::one_line(text).trim().to_owned()
}

#[cfg(test)]
#[path = "spaces_tests.rs"]
mod tests;
