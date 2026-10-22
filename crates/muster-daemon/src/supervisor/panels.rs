//! A plugin's panel opened in a workspace (`PRD-15`, `ARC-23`): the
//! person's word to open one checked against the registry as it reads
//! now, its command filled in from what Muster knows of where it opens,
//! and its session started there, in the workspace's folder, from the
//! basics of the environment sessions get, with what it is told
//! (`MUSTER_PLUGIN_*`) and the variables and secrets its manifest names
//! over them.

use muster_core::harness::SessionKind;
use muster_core::plugins::{Context, Missing, PanelOn};
use muster_core::protocol::plugins::Beside;
use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use muster_state::session::Start;

use super::Supervisor;
use super::sessions::Mode;
use crate::harness::{Harness, PanelLaunch, Place};
use crate::plugins::PluginError;

/// Why a plugin's panel does not open, said for the person.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PanelError {
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("the {plugin} plugin has no panel called {panel}")]
    NoPanel { plugin: String, panel: String },
    #[error("{title} is a session's panel: open it beside a session")]
    NoSession { title: String },
    #[error("{plugin} needs {program}, which is not on PATH{}", how(how_to_get.as_deref()))]
    Needs {
        plugin: String,
        program: String,
        how_to_get: Option<String>,
    },
    #[error("{title} cannot open: {missing}")]
    Missing { title: String, missing: Missing },
}

fn how(how_to_get: Option<&str>) -> String {
    how_to_get.map_or_else(String::new, |how| format!(": {how}"))
}

impl PanelError {
    /// The refusal as a client is told it (`ARC-07`).
    fn body(&self) -> ErrorBody {
        match self {
            Self::Plugin(error) => error.body(),
            Self::NoPanel { .. } => ErrorBody::new(ErrorCode::NotFound, self.to_string()),
            Self::NoSession { .. } => ErrorBody::new(ErrorCode::BadRequest, self.to_string()),
            Self::Needs { .. } | Self::Missing { .. } => {
                ErrorBody::new(ErrorCode::Conflict, self.to_string())
            }
        }
    }
}

impl Supervisor {
    /// `PluginCommand::OpenPanel`: `plugin`'s panel `panel` opened beside
    /// the session or on the workspace `beside` names, or the one open
    /// there already.
    pub(super) async fn open_panel(
        &mut self,
        plugin: &str,
        panel: &str,
        beside: Beside,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let (workspace, session) = match beside {
            Beside::Session(session) => (self.stored_session(session)?.workspace, Some(session)),
            Beside::Workspace(workspace) => (workspace, None),
        };
        let (launch, beside, context) =
            self.panel_launch(plugin, panel, workspace, session).await?;
        if let Some(open) = self.open_already(launch.harness(), workspace, beside) {
            return Ok(Reply::SessionStarted { id: open });
        }
        self.launch_panel(workspace, beside, launch, &context, size)
            .await
    }

    /// Starts the ended panel `session` again in the same session, opened
    /// afresh: its plugin as it reads now, told what Muster knows now.
    pub(super) async fn restart_panel(
        &mut self,
        session: SessionId,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let stored = self.stored_session(session)?;
        let SessionKind::Panel { plugin, panel } = stored.kind() else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} is no plugin's panel", stored.title),
            ));
        };
        let (mut launch, _, context) = self
            .panel_launch(plugin, panel, stored.workspace, stored.beside)
            .await?;
        launch.env.extend(context.variables());
        let path = self.panel_path(stored.workspace)?;
        self.reopen_and_launch(
            session,
            Harness::panel(launch),
            Place::Host(path),
            size,
            Mode::Fresh { prompt: None },
            Start::Idle,
        )
        .await?;
        Ok(Reply::SessionStarted { id: session })
    }

    /// What runs for `plugin`'s panel `panel` in `workspace`, beside the
    /// session `session` where it is a session's, and what it is told
    /// there: refused, saying why, where the plugin is not there, is off
    /// or cannot run, the panel is not its, a program it needs is not
    /// found, or a placeholder of its command has no value. The registry
    /// is read again first, so a manifest changed since its approval is
    /// caught.
    async fn panel_launch(
        &mut self,
        plugin: &str,
        panel: &str,
        workspace: WorkspaceId,
        session: Option<SessionId>,
    ) -> Result<(PanelLaunch, Option<SessionId>, Context), ErrorBody> {
        self.read_plugins().await;
        let refused = |error: PanelError| {
            tracing::info!(command = "open panel", %error, "command refused");
            error.body()
        };
        let (details, folder) = self
            .plugins
            .runnable(plugin)
            .map_err(|error| refused(error.into()))?;
        let found = details
            .panels
            .iter()
            .find(|candidate| candidate.id == panel)
            .ok_or_else(|| {
                refused(PanelError::NoPanel {
                    plugin: details.name.clone(),
                    panel: panel.to_owned(),
                })
            })?;
        let beside = match found.on {
            PanelOn::Session => Some(session.ok_or_else(|| {
                refused(PanelError::NoSession {
                    title: found.title.clone(),
                })
            })?),
            PanelOn::Workspace => None,
        };
        if let Some(needed) = details.requires.iter().find(|program| !program.found) {
            return Err(refused(PanelError::Needs {
                plugin: details.name.clone(),
                program: needed.program.clone(),
                how_to_get: details.how_to_get.clone(),
            }));
        }
        let context = self.panel_context(workspace, beside).await?;
        let mut command = context.expand(&found.command).map_err(|missing| {
            refused(PanelError::Missing {
                title: found.title.clone(),
                missing,
            })
        })?;
        // `./…` is in the plugin's folder.
        if let (Some(program), Some(folder)) = (command.first_mut(), &folder)
            && let Some(inside) = program.strip_prefix("./")
        {
            *program = folder.join(inside).to_string_lossy().into_owned();
        }
        let mut launch = PanelLaunch::new(plugin, panel, &found.title, command);
        launch.seen.clone_from(&details.env);
        for name in &details.secrets {
            if let Some(secret) =
                crate::secrets::plugin_secret(self.secrets.as_ref(), plugin, name).await
            {
                launch.secrets.push((name.clone(), secret));
            }
        }
        Ok((launch, beside, context))
    }

    /// Stops every panel of `plugin` still running, as the plugin is
    /// turned off or removed: its programs leave with it (`ARC-23`,
    /// `ARC-14`). The panels stay listed, ended.
    pub(super) async fn stop_panels_of(&mut self, plugin: &str) {
        for session in self.running_panels_of(plugin) {
            self.reopening.remove(&session);
            if let Err(error) = self.stop_session(session).await {
                tracing::warn!(%session, %error, "could not stop a plugin's panel");
            }
        }
    }

    /// Opens every panel of `plugin` still running afresh, as the plugin
    /// is linked, updated or approved again: each is stopped, and started
    /// again in the same session as it ends (`reopen_panel`), so none goes
    /// on running what the person no longer approved, or from a copy of
    /// the plugin Muster lets go of.
    pub(super) async fn reopen_panels_of(&mut self, plugin: &str) {
        for session in self.running_panels_of(plugin) {
            // One waiting its turn to resume ends at once, with no
            // terminal's end to wait for: opened again by the person.
            let waiting = self.resumes.is_waiting(session);
            match self.stop_session(session).await {
                Ok(_) if !waiting => {
                    self.reopening.insert(session);
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%session, %error, "could not stop a plugin's panel"),
            }
        }
    }

    /// The panel `session` ended: one stopped to be opened afresh is
    /// started again in the same session, in the size its terminal had,
    /// from its plugin as it reads now. One that cannot open now stays
    /// ended, and opening it again says why.
    pub(super) async fn reopen_panel(&mut self, session: SessionId, size: TermSize) {
        if !self.reopening.remove(&session) {
            return;
        }
        match self.restart_panel(session, size).await {
            Ok(_) => tracing::info!(%session, "a plugin's panel opened afresh"),
            Err(error) => {
                tracing::info!(%session, %error, "a plugin's panel stays ended");
            }
        }
    }

    /// The sessions of `plugin`'s panels still running.
    fn running_panels_of(&self, plugin: &str) -> Vec<SessionId> {
        self.state
            .sessions
            .iter()
            .filter(|session| {
                matches!(session.kind(), SessionKind::Panel { plugin: of, .. } if of == plugin)
            })
            .map(|session| session.id)
            .filter(|session| self.is_running(*session))
            .collect()
    }

    /// The panel running as `harness` in `workspace`, beside `beside`, if
    /// one is open there.
    fn open_already(
        &self,
        harness: &str,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
    ) -> Option<SessionId> {
        self.state
            .sessions
            .iter()
            .filter(|session| session.harness == harness && session.workspace == workspace)
            .filter(|session| session.beside == beside && session.ended_at.is_none())
            .find(|session| self.is_running(session.id))
            .map(|session| session.id)
    }

    /// What Muster knows of `workspace`, and of the session `beside` in it
    /// if a panel is beside one. The agent's record of its conversation is
    /// looked for off the loop.
    pub(super) async fn panel_context(
        &self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
    ) -> Result<Context, ErrorBody> {
        let mut context = self.known_context(workspace, beside)?;
        if let (Some(harness), Some(resume)) = (context.harness.clone(), context.resume.clone()) {
            let places = self.places.clone();
            let found = tokio::task::spawn_blocking(move || {
                crate::spend::transcript(&places, &harness, &resume)
            })
            .await;
            context.transcript = found
                .ok()
                .flatten()
                .map(|path| path.to_string_lossy().into_owned());
        }
        Ok(context)
    }

    /// What the state says of `workspace` and the session `beside` in it.
    fn known_context(
        &self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
    ) -> Result<Context, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == found.project);
        let branch = Some(found.kind.branch()).filter(|branch| !branch.is_empty());
        let mut context = Context {
            project: project.map(|project| project.name.clone()),
            root: project.map(|project| project.root.to_string_lossy().into_owned()),
            workspace: found
                .kind
                .host_path()
                .map(|path| path.to_string_lossy().into_owned()),
            branch: branch.map(str::to_owned),
            ticket: found.ticket_or_branch().map(String::from),
            pr: found.pull_request.as_ref().map(|pr| pr.url.clone()),
            ..Context::default()
        };
        if let Some(beside) = beside {
            let stored = self.stored_session(beside)?;
            if stored.workspace != workspace {
                return Err(ErrorBody::new(
                    ErrorCode::BadRequest,
                    format!("{} is not in that workspace", stored.title),
                ));
            }
            // The conversation it is on now, which the person may have
            // moved with `/resume`, or the one it last saved.
            let resume = self
                .live
                .get(&beside)
                .and_then(|live| live.machine.agent_session())
                .map(str::to_owned)
                .or(stored.resume_id);
            context.session = Some(beside.to_string());
            context.harness = Some(stored.harness);
            context.resume = resume;
        }
        Ok(context)
    }

    /// Starts `panel` in `workspace`'s folder, beside the session `beside`
    /// if it is beside one, told what Muster knows of where it is.
    #[cfg(test)]
    pub(super) async fn start_panel(
        &mut self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
        panel: PanelLaunch,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let context = self.panel_context(workspace, beside).await?;
        self.launch_panel(workspace, beside, panel, &context, size)
            .await
    }

    /// Starts `panel` in `workspace`'s folder, beside the session `beside`
    /// if it is beside one, told `context`: as any session, from the
    /// environment sessions get and in Muster's terminal, with its
    /// variables and Muster's (`MUSTER_PLUGIN_*`) over them. It is no
    /// agent: no hooks, no resume.
    async fn launch_panel(
        &mut self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
        mut panel: PanelLaunch,
        context: &Context,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let path = self.panel_path(workspace)?;
        if panel.command.first().is_none_or(String::is_empty) {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} names no program to run", panel.title),
            ));
        }
        panel.env.extend(context.variables());
        let harness = Harness::panel(panel);
        let id = self
            .add_session_beside(workspace, &harness, None, Start::Idle, beside)
            .await?;
        let mode = Mode::Fresh { prompt: None };
        let place = Place::Host(path);
        if let Err(error) = self.launch(id, harness.clone(), place, size, mode, Start::Idle, false)
        {
            // Stored before its program was started (`ARC-10`).
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {}: {error}", harness.label()),
            ));
        }
        tracing::info!(session = %id, panel = harness.id(), "a plugin's panel opened");
        Ok(Reply::SessionStarted { id })
    }

    /// Where a panel in `workspace` runs: on this Mac, never in a run's
    /// container.
    fn panel_path(&self, workspace: WorkspaceId) -> Result<std::path::PathBuf, ErrorBody> {
        self.workspace_path(workspace).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::BadRequest,
                "a plugin's panel runs on this Mac, never in a run's container",
            )
        })
    }
}
