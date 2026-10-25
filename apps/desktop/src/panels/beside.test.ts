// What stands beside the agent: a shell placed on `T` and a plugin's panel
// on its key, as the terminal UI's `app::shell` and `app::plugin_panel` open
// and put them away, against the mock daemon (ARC-24, PRD-15, PRD §6).

import { describe, expect, it, vi } from "vitest";
import type { App } from "../app/context";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { createDaemon } from "../state/daemon";
import { createUi } from "../state/ui";
import { openShell, panelBeside, togglePanel } from "./beside";
import { Layout } from "./layout";

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

/** The layouts the window asked the daemon to keep, in order. */
function kept(bridge: MockBridge): Layout[] {
  return bridge.sent.flatMap((command) =>
    typeof command === "object" &&
    "ChangePreference" in command &&
    typeof command.ChangePreference === "object" &&
    "Panels" in command.ChangePreference
      ? [Layout.parse(command.ChangePreference.Panels)]
      : [],
  );
}

describe("a shell beside the agent", () => {
  it("is the workspace's running shell, placed beside the session with the keys, on T", async () => {
    const { app, bridge } = await makeApp();
    expect(await openShell(app, desktop, codex)).toBe(gitLog);
    expect(app.ui.getState()).toMatchObject({ terminal: gitLog, keysIn: "terminal" });
    expect(kept(bridge).at(-1)?.placed()).toEqual([gitLog]);
    expect(
      bridge.sent.some((command) => typeof command === "object" && "StartSession" in command),
    ).toBe(false);
  });

  it("is started in a workspace with none, and placed once the daemon has it", async () => {
    const { app, bridge } = await makeApp();
    const shell = await openShell(app, resize, flaky);
    expect(bridge.sent).toContainEqual({
      StartSession: {
        workspace: resize,
        harness: "shell",
        title: null,
        size: { cols: 80, rows: 24 },
        prompt: null,
        started_by: null,
      },
    });
    expect(shell).not.toBeNull();
    expect(shell ? kept(bridge).at(-1)?.placed() : null).toEqual([shell]);
    await vi.waitFor(() =>
      expect(shell ? app.daemon.store.getState().sessions[shell] : undefined).toMatchObject({
        harness: "shell",
        title: "zsh",
        workspace: resize,
      }),
    );
  });

  it("on T again, gives the one placed the keys and places no other", async () => {
    const { app, bridge } = await makeApp();
    await openShell(app, desktop, codex);
    await settle();
    app.actions.keysTo("list");
    expect(await openShell(app, desktop, codex)).toBe(gitLog);
    expect(app.ui.getState()).toMatchObject({ terminal: gitLog, keysIn: "terminal" });
    expect(kept(bridge)).toHaveLength(1);
    expect(app.daemon.store.getState().sessions[gitLog]?.ended_at).toBeNull();
  });

  it("says why when the daemon refuses to start one", async () => {
    const { app, bridge } = await makeApp();
    bridge.failNext("StartSession", "no workspace 01JD", "not_found");
    expect(await openShell(app, resize, flaky)).toBeNull();
    expect(app.ui.getState().note).toEqual({ text: "no workspace 01JD", error: true });
  });
});

describe("a plugin's panel", () => {
  const graph = { plugin: "session-graph", panel: "graph", title: "graph" };

  it("opens beside the session selected, and shows there", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph, { session: codex, workspace: desktop });
    expect(bridge.sent.at(-1)).toEqual({
      Plugin: {
        OpenPanel: {
          plugin: "session-graph",
          panel: "graph",
          beside: { session: codex },
          size: { cols: 80, rows: 24 },
        },
      },
    });
    const state = app.daemon.store.getState();
    const panel = panelBeside(state, codex);
    expect(panel && state.sessions[panel]).toMatchObject({
      harness: "plugin:session-graph/graph",
      title: "graph",
      beside: codex,
    });
    // Another chat of the workspace never shows it.
    expect(panelBeside(state, ulid(103))).toBeNull();
    expect(app.ui.getState().note?.text).toBe("The plugin's panel is open.");
  });

  it("is put away on its key again: its program ended, and off the list", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph, { session: codex, workspace: desktop });
    const panel = panelBeside(app.daemon.store.getState(), codex);
    await togglePanel(app, graph, { session: codex, workspace: desktop });
    expect(app.ui.getState().note?.text).toBe("graph is put away.");
    await settle();
    expect(bridge.sent.slice(-2)).toEqual([
      { StopSession: { session: panel } },
      { RemoveSession: { session: panel } },
    ]);
    expect(panel && app.daemon.store.getState().sessions[panel]).toBeUndefined();
  });

  it("asks for a session where it opens beside one, and says the daemon's refusal", async () => {
    const { app, bridge } = await makeApp();
    await togglePanel(app, graph, { session: null, workspace: desktop });
    expect(app.ui.getState().note).toEqual({
      text: "Select a session to open graph beside it.",
      error: true,
    });
    bridge.failNext("Plugin", "Session graph needs zoe, which is not on PATH", "conflict");
    await togglePanel(app, graph, { session: codex, workspace: desktop });
    expect(app.ui.getState().note).toEqual({
      text: "Session graph needs zoe, which is not on PATH",
      error: true,
    });
  });
});
