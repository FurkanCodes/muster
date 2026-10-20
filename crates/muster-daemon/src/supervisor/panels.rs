//! A plugin's panel opened in a workspace (`PRD-15`, `ARC-23`): what
//! Muster knows of where it opens, and its session started there, in the
//! workspace's folder, from the environment every session gets, scrubbed,
//! with what it is told (`MUSTER_PLUGIN_*`) over it.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`Command::OpenPanel` (protocol 42) opens panels; until then only the tests do"
    )
)]

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use muster_state::session::Start;

use super::Supervisor;
use super::sessions::Mode;
use crate::harness::{Harness, PanelLaunch, Place};

/// What Muster knows of the session or workspace a panel is beside, each
/// value as its placeholder fills it in (`{branch}`) and its variable
/// tells it (`MUSTER_PLUGIN_BRANCH`); a value it does not know is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PanelContext {
    /// The project's name.
    pub(crate) project: Option<String>,
    /// The project's folder.
    pub(crate) root: Option<String>,
    /// The workspace's folder.
    pub(crate) workspace: Option<String>,
    /// What the workspace has checked out; none in a plain folder.
    pub(crate) branch: Option<String>,
    /// The session's id.
    pub(crate) session: Option<String>,
    /// The session's harness: `claude`, `codex`, an agent the person added.
    pub(crate) harness: Option<String>,
    /// The id of the agent's conversation, as its resume takes it.
    pub(crate) resume: Option<String>,
    /// The agent's own record of the conversation: Claude Code's
    /// transcript, Codex's rollout (`spend::transcript`).
    pub(crate) transcript: Option<String>,
    /// The ticket the workspace's branch is for.
    pub(crate) ticket: Option<String>,
    /// The pull request from the workspace's branch, by its address.
    pub(crate) pr: Option<String>,
}

impl PanelContext {
    /// The `MUSTER_PLUGIN_*` variables of the values it has.
    pub(crate) fn variables(&self) -> Vec<(String, String)> {
        [
            ("PROJECT", &self.project),
            ("ROOT", &self.root),
            ("WORKSPACE", &self.workspace),
            ("BRANCH", &self.branch),
            ("SESSION", &self.session),
            ("HARNESS", &self.harness),
            ("RESUME", &self.resume),
            ("TRANSCRIPT", &self.transcript),
            ("TICKET", &self.ticket),
            ("PR", &self.pr),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            let value = value.as_ref()?;
            Some((format!("MUSTER_PLUGIN_{name}"), value.clone()))
        })
        .collect()
    }
}

impl Supervisor {
    /// What Muster knows of `workspace`, and of the session `beside` in it
    /// if a panel is beside one. The agent's record of its conversation is
    /// looked for off the loop.
    pub(super) async fn panel_context(
        &self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
    ) -> Result<PanelContext, ErrorBody> {
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
    ) -> Result<PanelContext, ErrorBody> {
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
        let mut context = PanelContext {
            project: project.map(|project| project.name.clone()),
            root: project.map(|project| project.root.to_string_lossy().into_owned()),
            workspace: found
                .kind
                .host_path()
                .map(|path| path.to_string_lossy().into_owned()),
            branch: branch.map(str::to_owned),
            ticket: branch
                .and_then(muster_core::tickets::issue_of_branch)
                .map(String::from),
            pr: found.pull_request.as_ref().map(|pr| pr.url.clone()),
            ..PanelContext::default()
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
    /// if it is beside one, told what Muster knows of where it is: as any
    /// session, from the environment sessions get, scrubbed, and in Muster's
    /// terminal; with its plugin's variables, and Muster's
    /// (`MUSTER_PLUGIN_*`) over them. It is no agent: no hooks, no resume.
    pub(super) async fn start_panel(
        &mut self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
        mut panel: PanelLaunch,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let context = self.panel_context(workspace, beside).await?;
        let Some(path) = self.workspace_path(workspace) else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "a plugin's panel runs on this Mac, never in a run's container",
            ));
        };
        if panel.command.first().is_none_or(String::is_empty) {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} names no program to run", panel.title),
            ));
        }
        panel.env.extend(context.variables());
        let harness = Harness::panel(panel);
        let id = self
            .add_session(workspace, &harness, None, Start::Idle)
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
}
