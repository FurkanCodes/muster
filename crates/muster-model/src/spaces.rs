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

    /// The spaces `member` is in, in the order they were made.
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

    /// The names of the spaces `member` is in, as a line's note says them:
    /// `in Work, Home`; empty where it is in none.
    pub fn in_spaces(spaces: &[&Space]) -> String {
        if spaces.is_empty() {
            return String::new();
        }
        let names: Vec<String> = spaces
            .iter()
            .map(|space| muster_core::text::one_line(&space.name).trim().to_owned())
            .collect();
        format!("in {}", names.join(", "))
    }

    /// The free terminals, those in no project (`ARC-33`), oldest first.
    pub fn free_terminals(&self) -> impl Iterator<Item = &Session> {
        self.sessions.iter().filter(|session| session.is_free())
    }

    /// The lines `view` lists: its projects in the person's order, each
    /// with its worktrees and their sessions, then its free terminals under
    /// their heading. A space no longer there lists nothing.
    pub fn rows_in(&self, view: View) -> Vec<Row> {
        let space = match view {
            View::All => None,
            View::Space(id) => match self.space(id) {
                Some(space) => Some(space),
                None => return Vec::new(),
            },
        };
        let shown = |member| space.is_none_or(|space| space.holds(member));
        let mut rows = Vec::new();
        for project in &self.projects {
            if shown(Member::Project(project.id)) {
                self.project_rows(project.id, &mut rows);
            }
        }
        let terminals: Vec<Row> = self
            .free_terminals()
            .filter(|session| shown(Member::Session(session.id)))
            .map(|session| Row::Session(session.id))
            .collect();
        if !terminals.is_empty() {
            rows.push(Row::Terminals);
            rows.extend(terminals);
        }
        rows
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
    fn a_space_lists_its_projects_in_the_persons_order_then_its_free_terminals_arc_33() {
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
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone(), lib.clone(), other.clone()],
            workspaces: vec![tree.clone()],
            sessions: vec![chat.clone(), vm.clone(), notes.clone()],
            spaces: vec![work.clone()],
            ..Snapshot::default()
        });
        assert_eq!(
            model.rows_in(View::Space(work.id)),
            [
                Row::Project(app.id),
                Row::Worktree(tree.id),
                Row::Session(chat.id),
                Row::Project(lib.id),
                Row::Terminals,
                Row::Session(vm.id),
            ]
        );
        assert_eq!(
            model.rows_in(View::All),
            [
                Row::Project(app.id),
                Row::Worktree(tree.id),
                Row::Session(chat.id),
                Row::Project(lib.id),
                Row::Project(other.id),
                Row::Terminals,
                Row::Session(vm.id),
                Row::Session(notes.id),
            ]
        );
        assert_eq!(model.rows(), model.rows_in(View::All));
        assert_eq!(model.project_of(Row::Session(vm.id)), None);
        let named = |session: &Session| -> Vec<String> {
            model
                .spaces_of_session(session)
                .iter()
                .map(|space| space.name.clone())
                .collect()
        };
        assert_eq!(named(&vm), ["Work"]);
        assert_eq!(
            named(&chat),
            ["Work"],
            "a chat shows in its project's spaces"
        );
        assert!(named(&notes).is_empty());
        assert_eq!(model.worktree_of(Row::Session(vm.id)), None);
        assert_eq!(
            model.sessions_on(Row::Terminals).count(),
            2,
            "the heading stands for every free terminal"
        );
    }

    #[test]
    fn a_project_in_two_spaces_is_listed_in_both_and_noted_with_both() {
        let app = project("app");
        let work = space("Work", vec![Member::Project(app.id)]);
        let home = space("Home", vec![Member::Project(app.id)]);
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone()],
            spaces: vec![work.clone(), home.clone()],
            ..Snapshot::default()
        });
        assert_eq!(model.rows_in(View::Space(work.id)), [Row::Project(app.id)]);
        assert_eq!(model.rows_in(View::Space(home.id)), [Row::Project(app.id)]);
        let names: Vec<&str> = model
            .spaces_of(Member::Project(app.id))
            .map(|space| space.name.as_str())
            .collect();
        assert_eq!(names, ["Work", "Home"]);
        assert_eq!(
            Model::in_spaces(&model.spaces_of(Member::Project(app.id)).collect::<Vec<_>>()),
            "in Work, Home"
        );
        assert_eq!(Model::in_spaces(&[]), "");
    }

    #[test]
    fn spaces_follow_their_deltas_and_a_space_gone_lists_nothing() {
        let app = project("app");
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone()],
            ..Snapshot::default()
        });
        let mut work = space("Work", Vec::new());
        model.apply(Delta::SpaceUpserted(work.clone()));
        assert!(model.rows_in(View::Space(work.id)).is_empty());
        work.members.push(Member::Project(app.id));
        model.apply(Delta::SpaceUpserted(work.clone()));
        assert_eq!(model.spaces().len(), 1);
        assert_eq!(model.rows_in(View::Space(work.id)), [Row::Project(app.id)]);
        model.apply(Delta::SpaceRemoved { id: work.id });
        assert!(model.spaces().is_empty());
        assert!(model.rows_in(View::Space(work.id)).is_empty());
        assert_eq!(model.rows_in(View::All), [Row::Project(app.id)]);
    }

    #[test]
    fn no_heading_is_listed_without_a_free_terminal() {
        let app = project("app");
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone()],
            ..Snapshot::default()
        });
        assert_eq!(model.rows(), [Row::Project(app.id)]);
    }
}
