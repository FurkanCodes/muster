// A project's page, beside the list while its line is selected: the app's
// counterpart of the terminal UI's project and worktree lines. What the
// project is and where; a box to start work in it, the prompt first and
// under it one row of where it runs, which agent and Start (as the new
// session dialog asks, in fewer words); and under the box where its work is: its checkout and each worktree
// with its branch, what its work comes to, where it meets other work, its
// pull request, and the sessions in it, each a click from its terminal.

import { type KeyboardEvent, type RefObject, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useNow } from "../../app/clock";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { Page, PageActions, PageHeader, PageMeta, PageTitle } from "../../app/Page";
import { offered } from "../../flows/agents";
import { secondAgentWarning } from "../../flows/secondAgent";
import { makeWorktree, openStarted, startSession } from "../../flows/start";
import { useTask } from "../../flows/useTask";
import type { AddedAgent } from "../../ipc/generated/AddedAgent";
import type { ProjectId } from "../../ipc/generated/ProjectId";
import type { WorkspaceId } from "../../ipc/generated/WorkspaceId";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { useShownSessionActions } from "../../session/SessionActionsContext";
import type { DaemonState } from "../../state/daemon";
import { branchOf, projectColour, shownHarness, standsBeside } from "../../state/selectors";
import { Banner, BannerBody } from "../../ui/Banner";
import { Button, IconButton, Spacer } from "../../ui/controls";
import { ListItemEnd, ListItemMeta, ListItemTitle, RowButton } from "../../ui/List";
import { Section, SectionTitle } from "../../ui/Section";
import { Select } from "../../ui/Select";
import {
  AgentLogo,
  BranchName,
  Clash,
  DiffStat,
  ErrorText,
  MarkSpace,
  NewsDot,
  PullRequest,
  StatusMark,
  Swatch,
} from "../../ui/status";
import { Input, PlainTextArea } from "../../ui/TextField";
import "./project.css";

const NO_AGENTS: AddedAgent[] = [];

/** Where work starts: a new worktree, or one of the project's places by id. */
type Place = "new" | WorkspaceId;

/** A home folder written `~`, as the list writes a project's folder. */
function homely(path: string): string {
  return path.replace(/^\/(Users|home)\/[^/]+/, "~");
}

/** The project's checkout and worktrees, in the model's order: the checkout first. */
function placesOf(state: DaemonState, project: ProjectId): WorkspaceId[] {
  const out: WorkspaceId[] = [];
  let inProject = false;
  for (const row of state.derived.rows) {
    if (row.kind === "project") inProject = row.id === project;
    else if (inProject && row.kind === "workspace") {
      // A run's container is the run's own, not a place to start work.
      if (state.workspaces[row.id]?.kind.kind !== "container") out.push(row.id);
    }
  }
  return out;
}

/** A place as the box offers it: `id` and its words, joined to compare cheaply. */
function placeOptions(state: DaemonState, project: ProjectId): string[] {
  return placesOf(state, project).map((id) => {
    const workspace = state.workspaces[id];
    const branch = workspace ? branchOf(workspace) : "";
    const words =
      workspace?.kind.kind === "checkout" ? `Its checkout · ⎇ ${branch}` : `⎇ ${branch}`;
    return `${id}\t${words}`;
  });
}

export function ProjectPage({ project }: { project: ProjectId }) {
  const app = useApp();
  const name = useDaemon((state) => state.projects[project]?.name ?? "");
  const root = useDaemon((state) => state.projects[project]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, project));
  const checkout = useDaemon(
    (state) =>
      Object.values(state.workspaces).find(
        (workspace) => workspace.project === project && workspace.kind.kind === "checkout",
      ) ?? null,
  );
  const plain = checkout !== null && checkout.kind.branch === "";
  const places = useDaemon(useShallow((state) => placesOf(state, project)));
  // A plain folder has no worktrees: work starts in the folder itself.
  const [place, setPlace] = useState<Place>(() => (plain && checkout ? checkout.id : "new"));
  const field = useRef<HTMLTextAreaElement>(null);
  const startIn = (workspace: WorkspaceId) => {
    setPlace(workspace);
    field.current?.focus();
  };
  // Work asked to start here (`n`, ⌘N, New session): the box ready, where it was asked.
  const asked = useUi((state) => (state.compose?.project === project ? state.compose : null));
  const request = asked?.request;
  const askedPlace = asked?.place ?? null;
  useEffect(() => {
    if (request === undefined) return;
    setPlace(askedPlace ?? "new");
    field.current?.focus();
    app.actions.composed();
  }, [app, request, askedPlace]);
  return (
    <Page label={name} className="project-page">
      <PageHeader>
        <Swatch colour={colour} />
        <PageTitle>{name}</PageTitle>
        <PageMeta className="mu-mono" title={root}>
          {homely(root)}
        </PageMeta>
        <PageActions>
          {plain ? null : (
            <Button
              size="sm"
              icon="plus"
              onClick={() => app.actions.openFlow({ kind: "new-worktree", project })}
            >
              New worktree
            </Button>
          )}
          <Button size="sm" icon="ticket" onClick={() => app.actions.show("tickets")}>
            Tickets
          </Button>
        </PageActions>
      </PageHeader>
      <div className="project-body">
        <Composer
          project={project}
          plain={plain}
          place={plain && checkout ? checkout.id : place}
          onPlace={setPlace}
          field={field}
        />
        <Section className="project-where">
          <SectionTitle className="project-where-title">Where its work is</SectionTitle>
          {places.length === 0 ? (
            <p className="mu-meta">Nothing yet: the work you start shows here.</p>
          ) : (
            <ul className="project-places">
              {places.map((id) => (
                <PlaceCard key={id} id={id} onStart={startIn} />
              ))}
            </ul>
          )}
        </Section>
      </div>
    </Page>
  );
}

/** The box that starts work in the project: the prompt, then a row of where, which agent, and Start. */
function Composer({
  project,
  plain,
  place,
  onPlace,
  field,
}: {
  project: ProjectId;
  plain: boolean;
  place: Place;
  onPlace: (place: Place) => void;
  field: RefObject<HTMLTextAreaElement | null>;
}) {
  const app = useApp();
  const added = useDaemon((state) => state.preferences?.agents ?? NO_AGENTS);
  const last = useUi((state) => state.lastAgent);
  const agents = offered(added, last, "session");
  const [harness, setHarness] = useState(() => agents[0]?.id ?? "claude");
  const agent = agents.find((offer) => offer.id === harness) ?? agents[0];
  const [prompt, setPrompt] = useState("");
  const [branch, setBranch] = useState("");
  const places = useDaemon(useShallow((state) => placeOptions(state, project)));
  const warning = useDaemon((state) =>
    place !== "new" && harness !== "shell" ? secondAgentWarning(state.sessions, place) : null,
  );
  const task = useTask();
  const takesPrompt = agent?.takesPrompt ?? false;

  const submit = () =>
    task.run(async () => {
      let workspace = place;
      if (workspace === "new") {
        workspace = await makeWorktree(app, project, branch);
        // Kept, so a refused launch is tried again in it, not in another new one.
        onPlace(workspace);
      }
      const session = await startSession(
        app,
        workspace,
        harness,
        takesPrompt ? prompt.trim() || null : null,
      );
      setPrompt("");
      setBranch("");
      openStarted(app, session, harness);
    });

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && event.metaKey) {
      event.preventDefault();
      void submit();
    } else if (event.key === "Escape") {
      event.currentTarget.blur();
    }
  };

  const where = [
    ...(plain ? [] : [{ value: "new", label: "A new worktree" }]),
    ...places.map((line) => {
      const [value = "", label = ""] = line.split("\t");
      return { value, label };
    }),
  ];
  const start = harness === "shell" ? "Start a shell" : `Start ${agent?.name ?? "the agent"}`;

  return (
    <form
      className="project-composer"
      aria-label="Start work"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <PlainTextArea
        ref={field}
        className="project-prompt"
        aria-label="First prompt"
        rows={3}
        placeholder={
          takesPrompt ? "What should the agent do?" : `${agent?.name ?? "It"} takes no first prompt`
        }
        value={prompt}
        disabled={!takesPrompt}
        onChange={(event) => setPrompt(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <div className="project-composer-bar">
        {plain ? (
          <span className="mu-meta project-composer-note">In the folder itself</span>
        ) : (
          <Select
            label="Where"
            size="md"
            variant="quiet"
            options={where}
            value={place}
            disabled={task.busy}
            onChange={(next) => onPlace(next)}
          />
        )}
        {place === "new" && !plain ? (
          <Input
            mono
            className="project-branch"
            aria-label="Branch"
            placeholder="branch name, if you like"
            autoComplete="off"
            spellCheck={false}
            value={branch}
            disabled={task.busy}
            onChange={(event) => setBranch(event.target.value)}
          />
        ) : null}
        <Select
          label="Agent"
          size="md"
          variant="quiet"
          options={agents.map((offer) => ({
            value: offer.id,
            label: offer.name,
            mark: <AgentLogo harness={offer.id} />,
            note: offer.id === last ? "last chosen" : undefined,
          }))}
          value={harness}
          disabled={task.busy}
          onChange={setHarness}
        />
        <Spacer />
        <Button
          variant="primary"
          type="submit"
          busy={task.busy}
          kbd={takesPrompt ? "⌘⏎" : undefined}
        >
          {task.busy ? "Starting…" : start}
        </Button>
      </div>
      {warning ? (
        <Banner tone="overlap">
          <BannerBody>{warning}</BannerBody>
        </Banner>
      ) : null}
      {task.error ? <ErrorText size="meta">{task.error}</ErrorText> : null}
    </form>
  );
}

/** The project's checkout or a worktree: where it is, what its work comes to, and its sessions. */
function PlaceCard({ id, onStart }: { id: WorkspaceId; onStart: (id: WorkspaceId) => void }) {
  const app = useApp();
  const kind = useDaemon((state) => state.workspaces[id]?.kind.kind ?? "worktree");
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[id];
    return workspace ? branchOf(workspace) : "";
  });
  const plain = useDaemon((state) => state.workspaces[id]?.kind.branch === "");
  const added = useDaemon((state) => state.workspaces[id]?.changes?.added ?? 0);
  const removed = useDaemon((state) => state.workspaces[id]?.changes?.removed ?? 0);
  const clash = useDaemon((state) => {
    const clashes = state.workspaces[id]?.clashes ?? [];
    if (clashes.length === 0) return null;
    return clashes.some((one) => one.conflicts > 0) ? "conflict" : "overlap";
  });
  const pr = useDaemon((state) => state.workspaces[id]?.pull_request ?? null);
  const sessions = useDaemon(
    useShallow((state) =>
      state.derived.rows
        .filter((row) => row.kind === "session" && state.sessions[row.id]?.workspace === id)
        .map((row) => row.id),
    ),
  );
  return (
    <li className="project-place">
      <div className="project-place-head">
        {plain ? (
          <span className="mu-cut">The folder itself</span>
        ) : (
          <BranchName className="mu-cut">{branch}</BranchName>
        )}
        {kind === "checkout" ? <span className="mu-meta">your checkout</span> : null}
        {added || removed ? <DiffStat added={added} removed={removed} /> : null}
        {clash ? <Clash kind={clash} /> : null}
        {pr ? <PullRequest pr={pr} /> : null}
        <Spacer />
        <IconButton
          icon="plus"
          size="sm"
          label={`Start work in ${plain ? "the folder" : branch}`}
          onClick={() => onStart(id)}
        />
        {kind === "worktree" ? (
          <IconButton
            icon="close"
            size="sm"
            label={`Remove the worktree ${branch}`}
            onClick={() => app.actions.openFlow({ kind: "remove-worktree", workspace: id })}
          />
        ) : null}
      </div>
      {sessions.length === 0 ? (
        <p className="mu-meta project-place-empty">No session here yet.</p>
      ) : (
        <ul className="project-sessions">
          {sessions.map((session) => (
            <PlaceSession key={session} id={session} />
          ))}
        </ul>
      )}
    </li>
  );
}

/**
 * A session in a place: how it stands and what it is doing, a click from its
 * terminal; under the pointer, the way to close it from here (after asking,
 * while it runs), as ⌘K or a right-click on its line does.
 */
function PlaceSession({ id }: { id: string }) {
  const actions = useActions();
  const tools = useShownSessionActions();
  const now = useNow();
  const session = useDaemon((state) => state.sessions[id]);
  if (!session) return null;
  const beside = standsBeside(session);
  const what = beside ? (shownHarness(session) === "shell" ? "shell" : "panel") : doing(session);
  const unseen = session.unseen && session.status.state !== "needs_you";
  return (
    <li className="project-session-line">
      <RowButton
        className="project-session"
        label={`${session.title}, ${what}`}
        onClick={() => {
          actions.select("all", id);
          actions.keysTo("list");
        }}
      >
        {beside ? <MarkSpace /> : <StatusMark kind={statusKind(session)} />}
        <AgentLogo harness={shownHarness(session)} />
        <ListItemTitle className="project-session-name" title={session.title}>
          {session.title}
        </ListItemTitle>
        <ListItemMeta className="project-session-what" title={what}>
          {what}
        </ListItemMeta>
        <ListItemEnd>
          {ago(now - session.status_since)}
          {unseen ? <NewsDot /> : null}
        </ListItemEnd>
      </RowButton>
      <span className="project-session-close">
        <IconButton
          icon="trash"
          size="sm"
          label={`Close ${session.title}`}
          title="Close the session"
          onClick={() => tools.close(id)}
        />
      </span>
    </li>
  );
}
