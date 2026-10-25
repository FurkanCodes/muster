// What the mock daemon answers about spaces and terminals in no project, as
// the daemon's own code does (`supervisor::spaces`, `supervisor::free`): a
// space made, renamed, removed, things put in it and taken out; a terminal's
// folder (`mock.world.folders`, what its shell `cd`'d to: the home folder
// until a test says), filed where it is, made a project there, or started
// again in a workspace. Muster never types into the shell.

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Member } from "../generated/Member";
import type { Reply } from "../generated/Reply";
import type { Space } from "../generated/Space";
import type { MockBridge } from "./bridge";
import { NOW, ulid } from "./scenario";

/** The mock's home folder, where a terminal in no project starts. */
export const HOME = "/Users/ada";

function refuse(text: string, code: "not_found" | "bad_request" | "conflict"): never {
  throw new BridgeFailure("refused", text, code);
}

function same(a: Member, b: Member): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function named(mock: MockBridge, name: string, except: string | null): string {
  const trimmed = name.trim();
  if (!trimmed) refuse("a space needs a name", "bad_request");
  if (trimmed.length > 40) refuse("a space's name is at most 40 characters", "bad_request");
  const taken = (mock.world.spaces ?? []).find(
    (space) => space.id !== except && space.name.toLowerCase() === trimmed.toLowerCase(),
  );
  if (taken) refuse(`there is a space called ${taken.name} already`, "conflict");
  return trimmed;
}

function spaceOf(mock: MockBridge, id: string): Space {
  const found = (mock.world.spaces ?? []).find((space) => space.id === id);
  if (!found) refuse(`no space ${id}`, "not_found");
  return found;
}

function changed(mock: MockBridge, space: Space): void {
  // Copies, as the daemon's deltas are: a space changed in place would look
  // the same to a window that compares what it holds.
  mock.publish({
    spaces: (mock.world.spaces ?? []).map((held) => ({ ...held, members: [...held.members] })),
  });
  void space;
}

/** The folder a terminal's shell is in now. */
export function folderOf(mock: MockBridge, session: string): string {
  return mock.world.folders?.[session] ?? HOME;
}

/** The workspace whose folder holds `folder`, the deepest where several do. */
function holding(mock: MockBridge, folder: string): string | null {
  let best: { id: string; depth: number } | null = null;
  for (const workspace of mock.world.workspaces) {
    const path = "path" in workspace.kind ? workspace.kind.path : null;
    if (!path || !(folder === path || folder.startsWith(`${path}/`))) continue;
    const depth = path.split("/").length;
    if (!best || depth > best.depth) best = { id: workspace.id, depth };
  }
  return best?.id ?? null;
}

/** Puts `member` in space `id`, as a start or an add in a space does. */
export function join(mock: MockBridge, id: string | null, member: Member): void {
  if (id === null) return;
  const space = spaceOf(mock, id);
  if (!space.members.some((held) => same(held, member))) space.members.push(member);
  changed(mock, space);
}

export function spacesCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string") return undefined;
  mock.world.spaces ??= [];
  const spaces = mock.world.spaces;
  if ("CreateSpace" in command) {
    const space: Space = {
      id: ulid(700 + spaces.length),
      name: named(mock, command.CreateSpace.name, null),
      members: [],
      created_at: NOW,
    };
    spaces.push(space);
    changed(mock, space);
    return { SpaceCreated: { id: space.id } };
  }
  if ("RenameSpace" in command) {
    const space = spaceOf(mock, command.RenameSpace.space);
    space.name = named(mock, command.RenameSpace.name, space.id);
    changed(mock, space);
    return "Done";
  }
  if ("DeleteSpace" in command) {
    const space = spaceOf(mock, command.DeleteSpace.space);
    mock.world.spaces = spaces.filter((kept) => kept.id !== space.id);
    changed(mock, space);
    return "Done";
  }
  if ("AddToSpace" in command) {
    join(mock, command.AddToSpace.space, command.AddToSpace.member);
    return "Done";
  }
  if ("RemoveFromSpace" in command) {
    const { space: id, member } = command.RemoveFromSpace;
    const space = spaceOf(mock, id);
    space.members = space.members.filter((held) => !same(held, member));
    changed(mock, space);
    return "Done";
  }
  if ("ReadPlace" in command) {
    const folder = folderOf(mock, command.ReadPlace.session);
    return { Place: { folder, workspace: holding(mock, folder), front: null } };
  }
  if ("FileSession" in command) {
    const { session, workspace } = command.FileSession;
    const folder = folderOf(mock, session);
    if (holding(mock, folder) !== workspace)
      refuse(`its shell is in ${folder}, not in that folder`, "conflict");
    mock.updateSession(session, { workspace });
    mock.emitSession(session);
    for (const space of spaces)
      space.members = space.members.filter((held) => !same(held, { Session: session }));
    mock.publish({ spaces: [...spaces] });
    return "Done";
  }
  if ("RestartIn" in command) {
    const { session, workspace } = command.RestartIn;
    const old = mock.world.sessions.find((stored) => stored.id === session);
    if (!old) refuse(`no session ${session}`, "not_found");
    old.workspace = workspace;
    mock.emitSession(session);
    return { SessionStarted: { id: session } };
  }
  return undefined;
}
