// Spaces and terminals in no project, as the window asks for them
// (`ARC-33`, `PRODUCT.md` §6a): the window's own space (`ui.space`, never
// the daemon's), the daemon's commands, and what each says once done. What
// a space lists is the model's (`Derived::space_rows`); nothing here filters.

import type { App } from "../app/context";
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

/** The space new things join: the one the window shows. */
export function spaceToJoin(app: App): SpaceId | null {
  return spaceShown(app.daemon.store.getState(), app.ui.getState().space)?.id ?? null;
}

/** The spaces `member` is in, by name, for its line's note. */
export function spacesOf(state: DaemonState, member: Member): string[] {
  const key = JSON.stringify(member);
  return state.spaces
    .filter((space) => space.members.some((held) => JSON.stringify(held) === key))
    .map((space) => space.name);
}

/** Shows `space`, or all of the list, and says so. */
export function goTo(app: App, space: SpaceId | null): void {
  app.actions.showSpace(space);
  const name = spaceShown(app.daemon.store.getState(), space)?.name;
  app.actions.tell(name ? `Showing ${name}.` : "Showing everything.");
}

/** `]` and `[`: the next space, or the one before; all of the list after the last. */
export function cycle(app: App, by: number): void {
  const state = app.daemon.store.getState();
  const round: (SpaceId | null)[] = [null, ...state.spaces.map((space) => space.id)];
  if (round.length === 1) {
    app.actions.tell("There is no space yet: make one from the projects' + menu.", true);
    return;
  }
  const at = Math.max(0, round.indexOf(spaceShown(state, app.ui.getState().space)?.id ?? null));
  goTo(app, round[(at + by + round.length) % round.length] ?? null);
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

/** Puts `member` in `space`, or takes it out. */
export function toggleMember(app: App, space: Space, member: Member, inIt: boolean): void {
  void command(
    app,
    inIt
      ? { RemoveFromSpace: { space: space.id, member } }
      : { AddToSpace: { space: space.id, member } },
    inIt ? `Taken out of ${space.name}; it is still in everything.` : `Put in ${space.name}.`,
  );
}

/** `+`: a terminal in no project, in the home folder, in the space shown, opened. */
export async function newTerminal(app: App, size = { cols: 100, rows: 30 }): Promise<void> {
  try {
    const reply = await app.bridge.command({
      StartSession: {
        workspace: null,
        harness: "shell",
        title: null,
        size,
        prompt: null,
        started_by: null,
        space: spaceToJoin(app),
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
  if (app.daemon.store.getState().spaces.length === 0)
    app.actions.openFlow({ kind: "space-name", space: null });
  else app.actions.openSpaces(true);
}

/** `F`: where the selected terminal in no project goes under a project. */
export function fileTerminal(app: App): void {
  const id = app.ui.getState().selected.all;
  const session = id ? app.daemon.store.getState().sessions[id] : undefined;
  if (!session || session.workspace !== null) {
    app.actions.tell(
      "Select a terminal in no project, under Terminals, to put it under one.",
      true,
    );
    return;
  }
  app.actions.openFlow({ kind: "file-terminal", session: session.id });
}
