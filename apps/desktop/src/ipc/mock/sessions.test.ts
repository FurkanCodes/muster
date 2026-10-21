// The mock daemon answers a shell started, a plugin's panel opened and the
// panels' layout kept as the daemon does, refusals in its words and codes.

import { describe, expect, it } from "vitest";
import { BridgeFailure } from "../bridge";
import { MockBridge } from "./bridge";
import { ulid, workingDay } from "./scenario";

const desktop = ulid(11);
const codex = ulid(101);

async function refusal(promise: Promise<unknown>): Promise<BridgeFailure> {
  try {
    await promise;
  } catch (error) {
    if (error instanceof BridgeFailure) return error;
    throw error;
  }
  throw new Error("not refused");
}

describe("the mock daemon's sessions and panels", () => {
  it("starts a shell in a workspace, named for its program", async () => {
    const mock = new MockBridge(workingDay());
    const reply = await mock.command({
      StartSession: {
        workspace: desktop,
        harness: "shell",
        title: null,
        size: { cols: 80, rows: 24 },
        prompt: null,
      },
    });
    if (typeof reply !== "object" || !("SessionStarted" in reply)) throw new Error("not started");
    expect(mock.world.sessions.find((one) => one.id === reply.SessionStarted.id)).toMatchObject({
      harness: "shell",
      title: "zsh",
      workspace: desktop,
      beside: null,
    });
    const unknown = await refusal(
      mock.command({
        StartSession: {
          workspace: desktop,
          harness: "vim",
          title: null,
          size: { cols: 1, rows: 1 },
          prompt: null,
        },
      }),
    );
    expect([unknown.message, unknown.code]).toEqual(['no harness called "vim"', "bad_request"]);
    const nowhere = await refusal(
      mock.command({
        StartSession: {
          workspace: "x",
          harness: "shell",
          title: null,
          size: { cols: 1, rows: 1 },
          prompt: null,
        },
      }),
    );
    expect(nowhere.code).toBe("not_found");
  });

  it("opens a plugin's panel beside a session, or answers with the one open there", async () => {
    const mock = new MockBridge(workingDay());
    const open = () =>
      mock.command({
        Plugin: {
          OpenPanel: {
            plugin: "zoetrope",
            panel: "graph",
            beside: { session: codex },
            size: { cols: 1, rows: 1 },
          },
        },
      });
    const first = await open();
    expect(await open()).toEqual(first);
    const off = new MockBridge(workingDay());
    for (const plugin of off.world.plugins) plugin.enabled = false;
    const refused = await refusal(
      off.command({
        Plugin: {
          OpenPanel: {
            plugin: "zoetrope",
            panel: "graph",
            beside: { workspace: desktop },
            size: { cols: 1, rows: 1 },
          },
        },
      }),
    );
    expect(refused.code).toBe("conflict");
    expect(refused.message).toContain("zoetrope is turned off");
    const onWorkspace = await refusal(
      mock.command({
        Plugin: {
          OpenPanel: {
            plugin: "zoetrope",
            panel: "graph",
            beside: { workspace: desktop },
            size: { cols: 1, rows: 1 },
          },
        },
      }),
    );
    expect(onWorkspace.message).toBe(
      "Session graph is a session's panel: open it beside a session",
    );
  });

  it("keeps the panels' layout and the list's fold in the person's settings", async () => {
    const mock = new MockBridge(workingDay());
    expect(await mock.command({ ChangePreference: { Panels: "session | (shell / plugin)" } })).toBe(
      "Done",
    );
    // What it lacks is completed where the default puts it, as the daemon reads it.
    expect(mock.world.preferences.panels).toBe("(list | session) | ((changes / shell) / plugin)");
    await mock.command({ ChangePreference: { ListFolded: true } });
    expect(mock.world.preferences.list_folded).toBe(true);
    const bad = await refusal(mock.command({ ChangePreference: { Panels: "list | terminal" } }));
    expect(bad.kind).toBe("invalid");
  });
});
