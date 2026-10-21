// What stands beside the agent: the shell on `T` and a plugin's panel on
// its key, as the terminal UI's `app::shell` and `app::plugin_panel` open
// and put them away, against the mock daemon (ARC-24, PRD-15, PRD §6).

import { describe, expect, it } from "vitest";
import type { App } from "../app/context";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { createDaemon } from "../state/daemon";
import { createUi } from "../state/ui";
import {
  chordOfWritten,
  panelActions,
  panelBeside,
  shellBeside,
  togglePanel,
  toggleShell,
} from "./beside";

const codex = ulid(101);
const desktop = ulid(11);
const resize = ulid(12);
const flaky = ulid(103);
const gitLog = ulid(102);

async function makeApp(
  bridge = new MockBridge(workingDay()),
): Promise<{ app: App; bridge: MockBridge }> {
  const { store, actions } = createUi();
  const app: App = { bridge, daemon: createDaemon(), ui: store, actions };
  await bridge.open((message) => app.daemon.apply(message));
  await Promise.resolve();
  return { app, bridge };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("the shell beside the agent", () => {
  it("is the running shell of the workspace, with the keys, on T", async () => {
    const { app, bridge } = await makeApp();
    expect(await toggleShell(app, desktop, codex)).toBe(gitLog);
    expect(app.ui.getState()).toMatchObject({
      shell: gitLog,
      terminal: gitLog,
      keysIn: "terminal",
    });
    expect(shellBeside(app.daemon.store.getState(), gitLog, codex)).toBe(gitLog);
    // Never beside itself.
    expect(shellBeside(app.daemon.store.getState(), gitLog, gitLog)).toBeNull();
    expect(
      bridge.sent.some((command) => typeof command === "object" && "StartSession" in command),
    ).toBe(false);
  });

  it("is started in a workspace with none, and shown once the daemon has it", async () => {
    const { app, bridge } = await makeApp();
    const shell = await toggleShell(app, resize, flaky);
    expect(bridge.sent.at(-1)).toEqual({
      StartSession: {
        workspace: resize,
        harness: "shell",
        title: null,
        size: { cols: 80, rows: 24 },
        prompt: null,
      },
    });
    expect(shell).not.toBeNull();
    const stored = shell ? app.daemon.store.getState().sessions[shell] : undefined;
    expect(stored).toMatchObject({ harness: "shell", title: "zsh", workspace: resize });
    expect(app.ui.getState().shell).toBe(shell);
  });

  it("is put away on T again, running on, the keys back with the list", async () => {
    const { app } = await makeApp();
    await toggleShell(app, desktop, codex);
    expect(await toggleShell(app, desktop, codex)).toBeNull();
    expect(app.ui.getState()).toMatchObject({ shell: null, keysIn: "list", terminal: codex });
    expect(app.ui.getState().note?.text).toBe("The shell is put away; it runs on.");
    expect(app.daemon.store.getState().sessions[gitLog]?.ended_at).toBeNull();
  });

  it("says why when the daemon refuses to start one", async () => {
    const { app, bridge } = await makeApp();
    bridge.failNext("StartSession", "no workspace 01JD", "not_found");
    expect(await toggleShell(app, resize, flaky)).toBeNull();
    expect(app.ui.getState().note).toEqual({ text: "no workspace 01JD", error: true });
  });
});

describe("a plugin's panel", () => {
  const graph = () => {
    const action = panelActions(workingDay().plugins, {})[0];
    if (!action) throw new Error("no panel");
    return action;
  };

  it("is known by its plugin's manifest, on the key it suggests or the person's", () => {
    expect(panelActions(workingDay().plugins, {})).toEqual([
      {
        name: "plugin.zoetrope.graph",
        plugin: "zoetrope",
        panel: "graph",
        title: "Session graph",
        on: "session",
        keys: ["G"],
      },
    ]);
    expect(
      panelActions(workingDay().plugins, { "plugin.zoetrope.graph": ["ctrl-g"] })[0]?.keys,
    ).toEqual(["ctrl-g"]);
    const off = workingDay().plugins.map((plugin) => ({ ...plugin, enabled: false }));
    expect(panelActions(off, {})).toEqual([]);
  });

  it("opens beside the session selected, and shows there", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph(), { session: codex, workspace: desktop });
    expect(bridge.sent.at(-1)).toEqual({
      Plugin: {
        OpenPanel: {
          plugin: "zoetrope",
          panel: "graph",
          beside: { session: codex },
          size: { cols: 80, rows: 24 },
        },
      },
    });
    const state = app.daemon.store.getState();
    const panel = panelBeside(state, codex);
    expect(panel && state.sessions[panel]).toMatchObject({
      harness: "plugin:zoetrope/graph",
      title: "Session graph",
      beside: codex,
    });
    // Another chat of the workspace never shows it.
    expect(app.ui.getState().note?.text).toBe("The plugin's panel is open.");
  });

  it("is put away on its key again: its program ended, and off the list", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph(), { session: codex, workspace: desktop });
    const panel = panelBeside(app.daemon.store.getState(), codex);
    await togglePanel(app, graph(), { session: codex, workspace: desktop });
    expect(app.ui.getState().note?.text).toBe("Session graph is put away.");
    await settle();
    expect(bridge.sent.slice(-2)).toEqual([
      { StopSession: { session: panel } },
      { RemoveSession: { session: panel } },
    ]);
    expect(panel && app.daemon.store.getState().sessions[panel]).toBeUndefined();
  });

  it("asks for a session where it opens beside one, and says the daemon's refusal", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph(), { session: null, workspace: desktop });
    expect(app.ui.getState().note).toEqual({
      text: "Select a session to open Session graph beside it.",
      error: true,
    });
    bridge.failNext("Plugin", "zoetrope needs zoetrope, which is not on PATH", "conflict");
    await togglePanel(app, graph(), { session: codex, workspace: desktop });
    expect(app.ui.getState().note).toEqual({
      text: "zoetrope needs zoetrope, which is not on PATH",
      error: true,
    });
  });
});

describe("a key as [keys] writes it", () => {
  it("is named as the window's listener names it", () => {
    expect(chordOfWritten("T")).toBe("T");
    expect(chordOfWritten("G")).toBe("G");
    expect(chordOfWritten("g")).toBe("g");
    expect(chordOfWritten("ctrl-g")).toBe("⌃G");
    expect(chordOfWritten("shift-g")).toBe("G");
    expect(chordOfWritten("alt-p")).toBe("⌥P");
    expect(chordOfWritten("f5")).toBe("F5");
    expect(chordOfWritten("esc")).toBe("Escape");
    expect(chordOfWritten("shift-tab")).toBe("⇧Tab");
  });
});
