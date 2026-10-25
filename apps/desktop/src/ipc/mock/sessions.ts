// What the mock daemon answers for the sessions list and its panels, as the
// daemon's own code does: a shell started in a workspace (`StartSession`,
// `supervisor::sessions`), a plugin's panel opened beside a session or on a
// workspace, or the one open there already (`PluginCommand::OpenPanel`,
// `supervisor::panels`). The panels' layout is kept with the rest of
// Muster's own settings (`settings.ts`).

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { MockBridge } from "./bridge";
import { NOW, ulid } from "./scenario";
import { homeFor, moveTo } from "./spaces";

const HARNESSES: Record<string, string> = {
  shell: "Shell",
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  opencode: "OpenCode",
  copilot: "Copilot CLI",
};

function refuse(text: string, code: "not_found" | "bad_request" | "conflict"): never {
  throw new BridgeFailure("refused", text, code);
}

/** A new session's id, after the scenario's. */
function nextId(mock: MockBridge): string {
  const taken = mock.world.sessions.map((session) => Number(session.id.slice(4)) || 0);
  return ulid(Math.max(900, ...taken) + 1);
}

/** Stores a new session and tells the page, before its program starts (`ARC-10`). */
function started(
  mock: MockBridge,
  fields: Pick<Session, "workspace" | "harness" | "title" | "beside">,
): string {
  const id = nextId(mock);
  mock.world.sessions.push({
    id,
    resume_id: null,
    created_at: NOW,
    ended_at: null,
    status_since: NOW,
    activity: null,
    spend: null,
    tidy: null,
    unseen: false,
    status: { state: "fresh" },
    started_by: null,
    agent: null,
    ...fields,
  });
  // The program at its prompt: a shell's names it, a panel draws itself.
  mock.world.terminals[id] =
    fields.harness === "shell" ? "\x1b[35m❯\x1b[0m " : `${fields.title}\r\n`;
  mock.setStatus(id, { state: "running" });
  return id;
}

export function sessionsCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string") return undefined;

  if ("StartSession" in command) {
    const { workspace, harness, title, prompt, space } = command.StartSession;
    const label = HARNESSES[harness];
    if (!label) refuse(`no harness called "${harness}"`, "bad_request");
    if (workspace === null && harness !== "shell")
      refuse(`${label} starts in a project: a terminal can start in none`, "bad_request");
    if (workspace !== null && !mock.world.workspaces.some((stored) => stored.id === workspace))
      refuse(`no workspace wsp_${workspace}`, "not_found");
    if (prompt?.trim() && harness === "shell") refuse("Shell takes no prompt", "bad_request");
    // A terminal in no project is in a space (`ARC-33`).
    const home = workspace === null ? homeFor(mock, space) : null;
    // A shell is named for the program in front in it, its own at first.
    const id = started(mock, {
      workspace,
      harness,
      title: title?.trim() || (harness === "shell" ? "zsh" : label),
      beside: null,
    });
    if (home !== null) moveTo(mock, home, { Session: id });
    return { SessionStarted: { id } };
  }

  if ("Plugin" in command && typeof command.Plugin === "object" && "OpenPanel" in command.Plugin) {
    const { plugin, panel, beside } = command.Plugin.OpenPanel;
    const found = mock.world.plugins.find((one) => one.id === plugin);
    if (!found) refuse(`there is no plugin called ${plugin}`, "not_found");
    if (!found.enabled)
      refuse(
        `${plugin} is turned off: turn it on on the Settings screen's Plugins section, or with \`muster plugin enable ${plugin}\``,
        "conflict",
      );
    const manifest = found.details;
    const declared = manifest?.panels.find((one) => one.id === panel);
    if (!manifest || !declared)
      refuse(`the ${manifest?.name ?? plugin} plugin has no panel called ${panel}`, "not_found");
    const besideSession = "session" in beside ? beside.session : null;
    const workspace =
      "session" in beside
        ? mock.world.sessions.find((one) => one.id === beside.session)?.workspace
        : beside.workspace;
    if (!workspace) refuse(`no session ses_${besideSession}`, "not_found");
    if (declared.on === "session" && !besideSession)
      refuse(`${declared.title} is a session's panel: open it beside a session`, "bad_request");
    const needed = manifest.requires.find((program) => !program.found);
    if (needed)
      refuse(
        `${manifest.name} needs ${needed.program}, which is not on PATH${manifest.how_to_get ? `: ${manifest.how_to_get}` : ""}`,
        "conflict",
      );
    const harness = `plugin:${plugin}/${panel}`;
    const besideIt = declared.on === "session" ? besideSession : null;
    const open = mock.world.sessions.find(
      (one) =>
        one.harness === harness &&
        one.workspace === workspace &&
        one.beside === besideIt &&
        one.ended_at === null,
    );
    if (open) return { SessionStarted: { id: open.id } };
    const id = started(mock, { workspace, harness, title: declared.title, beside: besideIt });
    return { SessionStarted: { id } };
  }

  return undefined;
}
