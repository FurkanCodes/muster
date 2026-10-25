//! The person's spaces (`ARCHITECTURE.md` §2): made, renamed, removed,
//! and what is in them. A space only groups, so nothing here starts, stops
//! or moves anything; each change is stored, then sent to every client as
//! the space now is.

use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply};
use muster_core::spaces::{self, MAX_SPACES, Member, NameRefusal};
use muster_core::{Space, SpaceId};

use super::{Supervisor, store_closed, store_error};

impl Supervisor {
    /// Answers a command about spaces or free terminals.
    pub(super) async fn on_space_command(&mut self, command: Command) -> Result<Reply, ErrorBody> {
        match command {
            Command::CreateSpace { name } => self.create_space(&name).await,
            Command::RenameSpace { space, name } => self.rename_space(space, &name).await,
            Command::DeleteSpace { space } => self.delete_space(space).await,
            Command::AddToSpace { space, member } => {
                self.check_member(member)?;
                self.join(space, member).await.map(|()| Reply::Done)
            }
            Command::RemoveFromSpace { space, member } => self.leave(space, member).await,
            Command::ReadPlace { session } => self.read_place(session).await,
            Command::FileSession { session, workspace } => {
                self.file_session(session, workspace).await
            }
            Command::AddProjectHere { session, space } => {
                self.add_project_here(session, space).await
            }
            Command::RestartIn { session, workspace } => self.restart_in(session, workspace).await,
            other => Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{other:?} is no command about spaces"),
            )),
        }
    }

    fn space(&self, id: SpaceId) -> Result<&Space, ErrorBody> {
        self.state
            .spaces
            .iter()
            .find(|space| space.id == id)
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no space {id}")))
    }

    /// `name` as a space's, where no other space but `except` has it.
    fn space_name(&self, name: &str, except: Option<SpaceId>) -> Result<String, ErrorBody> {
        let others = self
            .state
            .spaces
            .iter()
            .filter(|space| Some(space.id) != except)
            .map(|space| space.name.as_str());
        spaces::check_name(name, others).map_err(|refusal| {
            let code = match refusal {
                NameRefusal::Taken(_) => ErrorCode::Conflict,
                _ => ErrorCode::BadRequest,
            };
            ErrorBody::new(code, refusal.to_string())
        })
    }

    async fn create_space(&mut self, name: &str) -> Result<Reply, ErrorBody> {
        if self.state.spaces.len() >= MAX_SPACES {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("there are {MAX_SPACES} spaces already, the most there can be"),
            ));
        }
        let space = Space {
            id: SpaceId::new(),
            name: self.space_name(name, None)?,
            members: Vec::new(),
            created_at: self.clock.now(),
        };
        let stored = space.clone();
        self.store
            .call(move |store| store.insert_space(&stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        tracing::info!(space = %space.id, "space made");
        let id = space.id;
        self.state.spaces.push(space.clone());
        self.broadcast(Delta::SpaceUpserted(space));
        Ok(Reply::SpaceCreated { id })
    }

    async fn rename_space(&mut self, id: SpaceId, name: &str) -> Result<Reply, ErrorBody> {
        self.space(id)?;
        let name = self.space_name(name, Some(id))?;
        let stored = name.clone();
        self.store
            .call(move |store| store.rename_space(id, &stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        self.change_space(id, |space| space.name = name);
        Ok(Reply::Done)
    }

    async fn delete_space(&mut self, id: SpaceId) -> Result<Reply, ErrorBody> {
        self.space(id)?;
        self.store
            .call(move |store| store.delete_space(id))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        tracing::info!(space = %id, "space removed");
        self.state.spaces.retain(|space| space.id != id);
        self.broadcast(Delta::SpaceRemoved { id });
        Ok(Reply::Done)
    }

    /// Refuses a member that cannot be in a space: one that is not there,
    /// or a terminal filed under a project, which shows with its project.
    fn check_member(&self, member: Member) -> Result<(), ErrorBody> {
        match member {
            Member::Project(project) => {
                if !self.state.projects.iter().any(|found| found.id == project) {
                    return Err(ErrorBody::new(
                        ErrorCode::NotFound,
                        format!("no project {project}"),
                    ));
                }
            }
            Member::Session(session) => {
                let found = self
                    .state
                    .sessions
                    .iter()
                    .find(|found| found.id == session)
                    .ok_or_else(|| {
                        ErrorBody::new(ErrorCode::NotFound, format!("no session {session}"))
                    })?;
                if !found.is_free() {
                    return Err(ErrorBody::new(
                        ErrorCode::BadRequest,
                        "it is in a project, so it shows wherever its project does: put the \
                         project in the space",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Puts `member` in space `id`; again does nothing.
    pub(super) async fn join(&mut self, id: SpaceId, member: Member) -> Result<(), ErrorBody> {
        if self.space(id)?.holds(member) {
            return Ok(());
        }
        self.store
            .call(move |store| store.add_member(id, member))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        self.change_space(id, |space| space.members.push(member));
        Ok(())
    }

    async fn leave(&mut self, id: SpaceId, member: Member) -> Result<Reply, ErrorBody> {
        if !self.space(id)?.holds(member) {
            return Ok(Reply::Done);
        }
        self.store
            .call(move |store| store.remove_member(id, member))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        self.change_space(id, |space| space.members.retain(|kept| *kept != member));
        Ok(Reply::Done)
    }

    /// The space `wanted`, refused where it is not there: asked before
    /// what joins it is made, so nothing is made for a space that is gone.
    pub(super) fn space_to_join(&self, wanted: Option<SpaceId>) -> Result<(), ErrorBody> {
        wanted.map_or(Ok(()), |id| self.space(id).map(|_| ()))
    }

    /// Takes what `gone` says out of every space it was in, as the store
    /// already has, and tells the clients of each space changed.
    pub(super) fn forget_members(&mut self, gone: impl Fn(Member) -> bool) {
        let changed: Vec<SpaceId> = self
            .state
            .spaces
            .iter()
            .filter(|space| space.members.iter().any(|member| gone(*member)))
            .map(|space| space.id)
            .collect();
        for id in changed {
            self.change_space(id, |space| space.members.retain(|member| !gone(*member)));
        }
    }

    fn change_space(&mut self, id: SpaceId, change: impl FnOnce(&mut Space)) {
        if let Some(space) = self.state.spaces.iter_mut().find(|space| space.id == id) {
            change(space);
            let now = space.clone();
            self.broadcast(Delta::SpaceUpserted(now));
        }
    }
}
