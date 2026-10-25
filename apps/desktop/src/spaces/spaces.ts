// Spaces and terminals in no project, as the window asks for them
// (`ARC-33`, `PRODUCT.md` §6a): everything lives in one space; the window's
// own space (`ui.space`, never the daemon's or kept); where something new
// goes, a space named first where there is none; the daemon's commands, and
// what each says once done. What a space lists is the model's
// (`Derived::space_rows`); nothing here filters.

import type { App } from "../app/context";
import { addProjectIn } from "../flows/actions";
import type { Later } from "../flows/types";
import type { Member } from "../ipc/generated/Member";
import type { SessionId } from "../ipc/generated/SessionId";
import type { Space } from "../ipc/generated/Space";
import type { SpaceId } from "../ipc/generated/SpaceId";
import { errorText } from "../lib/text";
import type { DaemonState } from "../state/daemon";
import { whenKnown } from "../state/whenKnown";

/** The space the window shows, while it is there; null for all of the list. */
export function spaceShown(state: DaemonState, space: SpaceId | null): Space | null {
  return space === null ? null : (state.spaces.find((one) => one.id === space) ?? null);
}

/** The space `id`'s line is in: a space's own, a project's, so that of everything under it, a free terminal's. */
export function spaceOfLine(state: DaemonState, id: string | undefined): SpaceId | null {
  if (!id) return null;
  if (state.spaces.some((space) => space.id === id)) return id;
  const session = state.sessions[id];
  const workspace = state.workspaces[session?.workspace ?? id];
  const member: Member | null = state.projects[id]
    ? { Project: id }
    : workspace
      ? { Project: workspace.project }
      : session
        ? { Session: id }
        : null;
  if (!member) return null;
  const key = JSON.stringify(member);
  return (
    state.spaces.find((space) => space.members.some((held) => JSON.stringify(held) === key))?.id ??
    null
  );
}

/** The space something new goes in: the selected line's, else the one shown, else the first. */
export function spaceToJoin(app: App): SpaceId | null {
  const state = app.daemon.store.getState();
  const ui = app.ui.getState();
  return (
    spaceOfLine(state, ui.selected.all) ??
    spaceShown(state, ui.space)?.id ??
    state.spaces[0]?.id ??
    null
  );
}

/** With no space yet, asks for one's name, `later` waiting on it; then true. */
export function spaceFirst(app: App, later: Later): boolean {
  if (app.daemon.store.getState().spaces.length > 0) return false;
  app.actions.openFlow({ kind: "space-name", space: null, after: later });
  return true;
}

/** Does `later` in `space`, just made: its delta may come after the reply that made it. */
export function doLater(app: App, later: Later, space: SpaceId): void {
  if (later.kind === "terminal") void terminalIn(app, space);
  else if (later.kind === "add-project") addProjectIn(app);
  else if (later.kind === "clone-project") app.actions.openFlow({ kind: "clone-project" });
  else void moveTo(app, space, later.member);
}

/** The spaces `member` is in, by name, for its line's note. */
export function spacesOf(state: DaemonState, member: Member): string[] {
  const key = JSON.stringify(member);
  return state.spaces
    .filter((space) => space.members.some((held) => JSON.stringify(held) === key))
    .map((space) => space.name);
}

/** Asks the daemon, saying `done` once it has, or why it would not. */
export async function command(
  app: App,
  asked: Parameters<App["bridge"]["command"]>[0],
  done?: string,
): Promise<void> {
  try {
    await app.bridge.command(asked);
    if (done) app.actions.tell(done);
  } catch (error) {
    app.actions.tell(errorText(error), true);
  }
}

/** Moves `member` to `space`, out of the one it was in. */
export function moveTo(app: App, space: SpaceId, member: Member): Promise<void> {
  const name = app.daemon.store.getState().spaces.find((one) => one.id === space)?.name;
  return command(
    app,
    { MoveToSpace: { space, member } },
    name ? `Moved to ${name}.` : "Moved to the space.",
  );
}

/** `+`: a terminal in no project, in the home folder, in the selected line's space, else in none. */
export async function newTerminal(app: App): Promise<void> {
  await terminalIn(app, spaceOfLine(app.daemon.store.getState(), app.ui.getState().selected.all));
}

/** A terminal in no project, in the home folder, in `space`, or in none to roam the machine; opened. */
export async function terminalIn(
  app: App,
  space: SpaceId | null,
  size = { cols: 100, rows: 30 },
): Promise<void> {
  try {
    const reply = await app.bridge.command({
      StartSession: {
        workspace: null,
        harness: "shell",
        title: null,
        size,
        prompt: null,
        started_by: null,
        space,
      },
    });
    if (typeof reply === "object" && "SessionStarted" in reply) {
      const id: SessionId = reply.SessionStarted.id;
      app.actions.show("all");
      whenKnown(app, "sessions", id, () => app.actions.select("all", id));
    }
  } catch (error) {
    app.actions.tell(errorText(error), true);
  }
}

/** `S`: the menu of spaces, or a new one where there is none yet. */
export function openSpaces(app: App): void {
  app.actions.openFlow({ kind: "space-name", space: null });
}

/** `F`: where the selected terminal in no project goes under a project. */
export function fileTerminal(app: App): void {
  const id = app.ui.getState().selected.all;
  const session = id ? app.daemon.store.getState().sessions[id] : undefined;
  if (!session || session.workspace !== null) {
    app.actions.tell(
      "Select a terminal in no project, under its space, to put it under one.",
      true,
    );
    return;
  }
  app.actions.openFlow({ kind: "file-terminal", session: session.id });
}
