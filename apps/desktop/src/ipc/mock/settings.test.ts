import { describe, expect, it } from "vitest";
import type { StateMessage } from "../generated/desktop/StateMessage";
import type { Reply } from "../generated/Reply";
import { MockBridge } from "./bridge";
import { ulid, workingDay } from "./scenario";

// The mock daemon answers Settings' commands as the daemon does, so the
// screen's tests mean what they say.

async function opened() {
  const bridge = new MockBridge(workingDay());
  const messages: StateMessage[] = [];
  await bridge.open((message) => messages.push(message));
  await Promise.resolve();
  return { bridge, messages };
}

function refusal(promise: Promise<Reply>) {
  return promise.then(
    () => {
      throw new Error("answered");
    },
    (error: { kind: string; code: string; message: string }) => ({
      kind: error.kind,
      code: error.code,
      message: error.message,
    }),
  );
}

describe("the mock daemon's settings", () => {
  it("makes a change to Muster's own settings and tells the page, then answers", async () => {
    const { bridge, messages } = await opened();
    expect(await bridge.command({ ChangePreference: { Notch: false } })).toBe("Done");
    const last = messages[messages.length - 1];
    expect(last?.kind === "patch" && last.patch.preferences?.notch).toBe(false);
  });

  it("refuses keys that would not read, as the settings file would", async () => {
    const { bridge } = await opened();
    expect(
      await refusal(
        bridge.command({ ChangePreference: { Keys: { action: "new_session", keys: ["d"] } } }),
      ),
    ).toEqual({
      kind: "refused",
      code: "bad_request",
      message:
        "~/.config/muster/settings.toml: keys.new_session: d is show_changes's on the same screen",
    });
    expect(
      await refusal(
        bridge.command({ ChangePreference: { Keys: { action: "nothing", keys: ["Z"] } } }),
      ),
    ).toMatchObject({
      message: "~/.config/muster/settings.toml: keys.nothing is not something a key does",
    });
    // Taking a key away always works, a plugin's panel's too.
    expect(
      await bridge.command({
        ChangePreference: { Keys: { action: "plugin.gone.panel", keys: null } },
      }),
    ).toBe("Done");
  });

  it("reads an edit by hand only on the person's word", async () => {
    const { bridge, messages } = await opened();
    bridge.settings.editByHand({ side_by_side_from: 0 });
    expect(bridge.world.preferences.side_by_side_from).toBe(100);
    await bridge.command("ReadPreferences");
    const last = messages[messages.length - 1];
    expect(last?.kind === "patch" && last.patch.preferences?.side_by_side_from).toBe(0);
  });

  it("answers a project's settings, and refuses a tracker that needs more", async () => {
    const { bridge } = await opened();
    const project = ulid(3);
    const reply = await bridge.command({ ProjectSettings: { project } });
    expect(reply).toMatchObject({
      ProjectSettings: {
        tickets_from: "Linear, team KEP",
        muster_prefix: { on: true, from: "project" },
      },
    });
    expect(
      await refusal(
        bridge.command({
          ChangeSetting: { project, change: { TicketsFrom: { tracker: "jira", fields: {} } } },
        }),
      ),
    ).toMatchObject({
      message: `~/.config/muster/projects/${project}.toml: tickets.tracker is jira, which needs tickets.site (Jira site, such as acme.atlassian.net)`,
    });
    // Only my tickets is the tickets mock's, so the Tickets screen agrees.
    expect(
      await bridge.command({ ChangeSetting: { project, change: { OnlyMyTickets: true } } }),
    ).toMatchObject({ ProjectSettings: { only_mine: { on: true, from: "yours" } } });
    expect(bridge.world.tickets[project]?.mine).toBe(true);
    expect(
      await bridge.command({
        ChangeSetting: { project, change: { TicketsFrom: { tracker: "github", fields: {} } } },
      }),
    ).toMatchObject({
      ProjectSettings: { tickets_from: "GitHub issues", tickets_set_by: "yours" },
    });
    expect(bridge.world.ticketSettings[project]?.tickets).toEqual({
      tracker: "github",
      fields: {},
    });
  });

  it("refuses removing a project with worktrees or running sessions", async () => {
    const { bridge } = await opened();
    expect(await refusal(bridge.command({ RemoveProject: { project: ulid(1) } }))).toMatchObject({
      code: "conflict",
      message: "muster has 2 worktree(s); remove them first",
    });
    expect(await refusal(bridge.command({ RemoveProject: { project: ulid(4) } }))).toMatchObject({
      message: "stop kep-api's 1 running session(s) first",
    });
    expect(await bridge.command({ RemoveProject: { project: ulid(3) } })).toBe("Done");
    expect(bridge.world.projects.map((project) => project.name)).toEqual([
      "muster",
      "lamphaus",
      "kep-api",
    ]);
  });

  it("lists models for an agent that can be asked, and refuses one that cannot", async () => {
    const { bridge } = await opened();
    expect(await bridge.command({ ListModels: { agent: "claude" } })).toEqual({
      Models: ["haiku", "sonnet", "opus"],
    });
    expect(await refusal(bridge.command({ ListModels: { agent: "gemini" } }))).toMatchObject({
      code: "not_found",
      message: "gemini cannot be asked to name chats",
    });
  });

  it("previews, installs, turns on and off and removes plugins with the daemon's refusals", async () => {
    const { bridge } = await opened();
    expect(
      await refusal(
        bridge.command({ Plugin: { SetEnabled: { id: "youtrack", on: true, by: "ui" } } }),
      ),
    ).toMatchObject({
      code: "conflict",
      message: expect.stringMatching(/^youtrack cannot be turned on: its manifest changed/),
    });
    expect(await refusal(bridge.command({ Plugin: { Remove: { id: "github" } } }))).toMatchObject({
      message: "github is one of Muster's own: it can be turned off, not replaced or removed",
    });
    expect(
      await refusal(
        bridge.command({
          Plugin: {
            Preview: {
              origin: { Git: { url: "https://github.com/acme/muster-youtrack", git_ref: null } },
            },
          },
        }),
      ),
    ).toMatchObject({
      message: expect.stringMatching(/^a plugin called youtrack is installed already/),
    });
    const preview = await bridge.command({
      Plugin: {
        Preview: {
          origin: { Git: { url: "https://github.com/acme/muster-standup", git_ref: null } },
        },
      },
    });
    if (typeof preview !== "object" || !("PluginPreview" in preview)) throw new Error("no preview");
    expect(preview.PluginPreview).toMatchObject({ kind: "install", id: "standup", current: null });
    expect(
      await bridge.command({
        Plugin: { Install: { preview: preview.PluginPreview.preview, by: "ui" } },
      }),
    ).toBe("Done");
    expect(bridge.world.plugins.find((plugin) => plugin.id === "standup")).toMatchObject({
      enabled: true,
      changed: { what: "installed", by: "ui" },
    });
    // A preview is installed once.
    expect(
      await refusal(
        bridge.command({
          Plugin: { Install: { preview: preview.PluginPreview.preview, by: "ui" } },
        }),
      ),
    ).toMatchObject({ code: "not_found" });
  });

  it("records a sound asked to be heard", async () => {
    const { bridge } = await opened();
    await bridge.hear("Glass");
    expect(bridge.heard).toEqual(["Glass"]);
  });
});
