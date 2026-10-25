//! What a space shows (`ARCHITECTURE.md` §5a, §5c): the one place the
//! terminal UI and the desktop app learn which lines a view lists, so they
//! list alike (`ARC-31`). A space only groups: what is in it is listed as
//! it is everywhere else.

use muster_core::spaces::Member;
use muster_core::{Session, Space, SpaceId};

use super::{Model, Row};

/// What the sessions list shows: everything, or one space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    #[default]
    All,
    Space(SpaceId),
}

impl Model {
    /// The person's spaces, in the order they were made.
    pub fn spaces(&self) -> &[Space] {
        &self.spaces
    }

    pub fn space(&self, id: SpaceId) -> Option<&Space> {
        self.spaces.iter().find(|space| space.id == id)
    }

    /// The spaces `member` is in, in the order they were made: one, as
    /// the daemon keeps it (`ARC-33`), or none for a moment between deltas.
    pub fn spaces_of(&self, member: Member) -> impl Iterator<Item = &Space> {
        self.spaces.iter().filter(move |space| space.holds(member))
    }

    /// The spaces `session` shows in: its project's, or its own where it
    /// is a free terminal; for the note on a line in *Needs you*.
    pub fn spaces_of_session(&self, session: &Session) -> Vec<&Space> {
        let member = match session.workspace {
            None => Member::Session(session.id),
            Some(workspace) => match self.workspace(workspace) {
                Some(found) => Member::Project(found.project),
                None => return Vec::new(),
            },
        };
        self.spaces_of(member).collect()
    }

    /// The space a line is in: a heading's own; a project's, and so that of
    /// everything under it; a free terminal's.
    pub fn space_of(&self, row: Row) -> Option<SpaceId> {
        let member = match row {
            Row::Space(id) => return self.space(id).map(|space| space.id),
            Row::Session(id) if self.session(id).is_some_and(Session::is_free) => {
                Member::Session(id)
            }
            other => Member::Project(self.project_of(other)?),
        };
        self.spaces_of(member).next().map(|space| space.id)
    }

    /// The free terminals, those in no project (`ARC-33`), oldest first.
    pub fn free_terminals(&self) -> impl Iterator<Item = &Session> {
        self.sessions.iter().filter(|session| session.is_free())
    }

    /// The lines `view` lists. Everything: each space's heading, in the
    /// order made, then what it holds; a space alone: what it holds, with
    /// no heading. What it holds is its projects in the person's order,
    /// each with its worktrees and their sessions, then its free
    /// terminals. A space no longer there lists nothing. Everything lists
    /// what is in no space last, as it can be for a moment between deltas,
    /// so nothing drops out of sight.
    pub fn rows_in(&self, view: View) -> Vec<Row> {
        let mut rows = Vec::new();
        match view {
            View::Space(id) => {
                if let Some(space) = self.space(id) {
                    self.held_rows(|member| space.holds(member), &mut rows);
                }
            }
            View::All => {
                for space in &self.spaces {
                    rows.push(Row::Space(space.id));
                    self.held_rows(|member| space.holds(member), &mut rows);
                }
                let homeless = |member| !self.spaces.iter().any(|space| space.holds(member));
                self.held_rows(homeless, &mut rows);
            }
        }
        rows
    }

    /// The lines of what `holds` says: projects, then free terminals.
    fn held_rows(&self, holds: impl Fn(Member) -> bool, rows: &mut Vec<Row>) {
        for project in &self.projects {
            if holds(Member::Project(project.id)) {
                self.project_rows(project.id, rows);
            }
        }
        rows.extend(
            self.free_terminals()
                .filter(|session| holds(Member::Session(session.id)))
                .map(|session| Row::Session(session.id)),
        );
    }
}

#[cfg(test)]
mod tests {
    use muster_core::protocol::{Delta, Snapshot};
    use muster_core::{SpaceId, Timestamp};

    use super::*;
    use crate::fixtures::{free_terminal, project, session, worktree};

    fn space(name: &str, members: Vec<Member>) -> Space {
        Space {
            id: SpaceId::new(),
            name: name.to_owned(),
            members,
            created_at: Timestamp::from_millis(1),
        }
    }

    #[test]
    fn everything_lists_each_space_under_its_heading_and_a_space_alone_without_arc_33() {
        let (app, lib, other) = (project("app"), project("lib"), project("other"));
        let tree = worktree(app.id, "work");
        let chat = session(tree.id, "Fix", 2);
        let (vm, notes) = (free_terminal("vm", 3), free_terminal("notes", 4));
        let work = space(
            "Work",
            vec![
                Member::Session(vm.id),
                Member::Project(lib.id),
                Member::Project(app.id),
            ],
        );
        let home = space(
            "Home",
            vec![Member::Project(other.id), Member::Session(notes.id)],
        );
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone(), lib.clone(), other.clone()],
            workspaces: vec![tree.clone()],
            sessions: vec![chat.clone(), vm.clone(), notes.clone()],
            spaces: vec![work.clone(), home.clone()],
            ..Snapshot::default()
        });
        let work_rows = [
            Row::Project(app.id),
            Row::Worktree(tree.id),
            Row::Session(chat.id),
            Row::Project(lib.id),
            Row::Session(vm.id),
        ];
        assert_eq!(model.rows_in(View::Space(work.id)), work_rows);
        let mut all = vec![Row::Space(work.id)];
        all.extend(work_rows);
        all.extend([
            Row::Space(home.id),
            Row::Project(other.id),
            Row::Session(notes.id),
        ]);
        assert_eq!(model.rows_in(View::All), all);
        assert_eq!(model.rows(), model.rows_in(View::All));

        // The space of each line: the heading's, the project's, the terminal's.
        assert_eq!(model.space_of(Row::Space(home.id)), Some(home.id));
        assert_eq!(model.space_of(Row::Session(chat.id)), Some(work.id));
        assert_eq!(model.space_of(Row::Worktree(tree.id)), Some(work.id));
        assert_eq!(model.space_of(Row::Session(notes.id)), Some(home.id));
        assert_eq!(model.project_of(Row::Space(work.id)), None);
        assert_eq!(model.worktree_of(Row::Session(vm.id)), None);
        assert_eq!(
            model.sessions_on(Row::Space(work.id)).count(),
            2,
            "a space stands for its projects' sessions and its terminals"
        );
        let named: Vec<&str> = model
            .spaces_of_session(&chat)
            .iter()
            .map(|space| space.name.as_str())
            .collect();
        assert_eq!(named, ["Work"], "a chat shows in its project's space");
    }

    #[test]
    fn spaces_follow_their_deltas_and_what_is_in_none_still_shows_last() {
        let app = project("app");
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone()],
            ..Snapshot::default()
        });
        // In no space for a moment, as between deltas: still listed.
        assert_eq!(model.rows(), [Row::Project(app.id)]);
        let mut work = space("Work", Vec::new());
        model.apply(Delta::SpaceUpserted(work.clone()));
        assert!(model.rows_in(View::Space(work.id)).is_empty());
        assert_eq!(
            model.rows(),
            [Row::Space(work.id), Row::Project(app.id)],
            "an empty space keeps its heading"
        );
        work.members.push(Member::Project(app.id));
        model.apply(Delta::SpaceUpserted(work.clone()));
        assert_eq!(model.rows_in(View::Space(work.id)), [Row::Project(app.id)]);
        assert_eq!(model.space_of(Row::Project(app.id)), Some(work.id));
        model.apply(Delta::SpaceRemoved { id: work.id });
        assert!(model.spaces().is_empty());
        assert!(model.rows_in(View::Space(work.id)).is_empty());
        assert_eq!(model.rows_in(View::All), [Row::Project(app.id)]);
    }
}
