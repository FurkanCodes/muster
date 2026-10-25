import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it } from "vitest";
import type { App } from "../app/context";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { renderApp, renderWithApp } from "../test/render";
import { openList } from "../test/select";
import { catalogue } from "./catalogue";

beforeAll(() => {
  // jsdom has no modal dialogs; WebKit's own are tested end to end (e2e/flows.spec.ts).
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

/** The window's actions, as the mock build hands them to tests. */
function ui(): App["actions"] {
  return (window as unknown as { __musterUi: App["actions"] }).__musterUi;
}

async function windowOver(bridge = new MockBridge(workingDay())) {
  const rendered = renderApp(bridge);
  await screen.findByRole("img", { name: /working, .* waiting on you/ });
  return rendered;
}

function select(screenId: Parameters<App["actions"]["select"]>[0], id: string) {
  act(() => {
    ui().show(screenId);
    ui().select(screenId, id);
  });
}

const named = (bridge: MockBridge, name: string) =>
  bridge.sent.filter((command) => typeof command === "object" && name in command);

/** The project page's box, as `n` or a project's line shows it. */
async function startBox(project: string) {
  const page = await screen.findByRole("region", { name: project });
  return within(page).getByRole("form", { name: "Start work" });
}

/** Chooses `option` in the box's select named `label`. */
async function choose(box: HTMLElement, label: string, option: string | RegExp) {
  await userEvent.click(within(box).getByRole("combobox", { name: label }));
  await userEvent.click(within(await openList(label)).getByRole("option", { name: option }));
}

describe("PRD-03 work starts on the project's page", () => {
  it("n opens it ready where the session works, says who works there, and not in a new worktree", async () => {
    await windowOver();
    select("all", ulid(103));
    await userEvent.keyboard("n");
    const box = await startBox("muster");
    expect(within(box).getByRole("textbox", { name: "First prompt" })).toHaveFocus();
    expect(within(box).getByRole("status")).toHaveTextContent(
      "2 agents already work here: two agents in one place change the same files.",
    );
    await choose(box, "Where", "A new worktree");
    expect(within(box).queryByRole("status")).toBeNull();
  });

  it("makes the worktree, then starts in it; a refused launch is tried again there, never in another", async () => {
    const { bridge } = await windowOver();
    select("all", ulid(1));
    const box = await startBox("muster");
    await userEvent.type(within(box).getByRole("textbox", { name: "Branch" }), "spike/webgl");
    // The daemon refuses the first launch.
    bridge.failNext("StartSession", "could not start Claude Code: no such file");
    await userEvent.click(within(box).getByRole("button", { name: /^Start Claude Code/ }));
    expect(await within(box).findByRole("alert")).toHaveTextContent(
      "could not start Claude Code: no such file",
    );
    // Where now names the worktree made, so the next try starts there.
    await waitFor(() =>
      expect(within(box).getByRole("combobox", { name: "Where" })).toHaveTextContent("spike/webgl"),
    );
    await userEvent.click(within(box).getByRole("button", { name: /^Start Claude Code/ }));
    await waitFor(() => expect(named(bridge, "StartSession")).toHaveLength(2));
    expect(named(bridge, "CreateWorktree")).toHaveLength(1);
    const made = (named(bridge, "CreateWorktree")[0] as { CreateWorktree: { branch: string } })
      .CreateWorktree;
    expect(made.branch).toBe("spike/webgl");
    const starts = named(bridge, "StartSession") as { StartSession: { workspace: string } }[];
    expect(starts[0]?.StartSession.workspace).toBe(starts[1]?.StartSession.workspace);
  });

  it("offers the person's own agents after Muster's and before a shell; one that takes no prompt gets none", async () => {
    const world = workingDay();
    world.preferences.agents = [
      { name: "goose", label: "Goose", mark: "•", colour: null, takes_prompt: false, asks: false },
    ];
    const { bridge } = await windowOver(new MockBridge(world));
    select("needs-you", ulid(101));
    await userEvent.keyboard("n");
    const box = await startBox("muster");
    await userEvent.click(within(box).getByRole("combobox", { name: "Agent" }));
    const names = within(await openList("Agent"))
      .getAllByRole("option")
      .map((option) => option.textContent);
    expect(names).toEqual([
      "✓Claude Codelast chosen",
      "Codex",
      "Gemini CLI",
      "OpenCode",
      "Copilot CLI",
      "•Goose",
      "❯A shell",
    ]);
    await userEvent.click(screen.getByRole("option", { name: /Goose/ }));
    expect(within(box).getByRole("textbox", { name: "First prompt" })).toBeDisabled();
    await userEvent.click(within(box).getByRole("button", { name: /^Start Goose/ }));
    await waitFor(() => expect(named(bridge, "StartSession")).toHaveLength(1));
    expect(named(bridge, "StartSession")[0]).toMatchObject({
      StartSession: { harness: "goose", prompt: null, workspace: ulid(11) },
    });
  });
});

describe("PRD-01 refusals said before asking, in the TUI's words", () => {
  it("says what to select, and why a plain folder has no worktrees", async () => {
    const { bridge } = await windowOver();
    select("all", ulid(1));
    await userEvent.keyboard("s");
    expect(await screen.findByText("Select a session to stop.")).toBeVisible();
    await userEvent.keyboard("u");
    expect(
      await screen.findByText("Select a session in a worktree to move its branch onto its base."),
    ).toBeVisible();
    act(() => bridge.drop(["/Users/you/code/labs"]));
    expect(await screen.findByText("Added labs.")).toBeVisible();
    await userEvent.keyboard("w");
    expect(
      await screen.findByText(
        "labs is a plain folder, not a git repository, so it has no worktrees: press n for a session in it.",
      ),
    ).toBeVisible();
  });
});

describe("ARC-24 the palette's catalogue", () => {
  it("puts actions on the selected first, then anywhere, sessions asking first, tickets, settings", async () => {
    const { app } = renderWithApp(null);
    await waitFor(() => expect(app.daemon.store.getState().loaded).toBe(true));
    app.actions.select("all", ulid(101));
    const items = catalogue(app, "", [
      {
        project: ulid(1),
        projectName: "muster",
        issue: { key: "205", title: "Resize settles", url: "", labels: [], author: null },
      },
    ]);
    expect([...new Set(items.map((item) => item.group))]).toEqual([
      "Actions on Sketch the Tauri shell",
      "Anywhere",
      "Sessions",
      "Tickets",
      "Settings",
    ]);
    const sessions = items.filter((item) => item.group === "Sessions").map((item) => item.label);
    expect(sessions.slice(0, 2)).toEqual([
      "Sketch the Tauri shell",
      "Share the Gradle cache across worktrees",
    ]);
    expect(items.find((item) => item.group === "Tickets")?.lead).toEqual({
      kind: "ticket",
      key: "#205",
    });
    // Typed letters narrow each group, those starting a word first.
    const typed = catalogue(app, "stop", []).map((item) => item.label);
    expect(typed).toEqual(["Stop the session"]);
  });

  it("lists Settings' pages, and once something is typed each setting and each project's page", async () => {
    const { app } = renderWithApp(null);
    await waitFor(() => expect(app.daemon.store.getState().loaded).toBe(true));
    const untyped = catalogue(app, "", []).filter((item) => item.group === "Settings");
    expect(untyped.map((item) => item.label)).toEqual([
      "General",
      "Notifications",
      "Agents",
      "Naming chats",
      "Plugins",
      "Keys",
    ]);
    const reset = catalogue(app, "reset", []);
    expect(reset.map((item) => [item.label, item.meta])).toEqual([
      ["Put the panels back as at first, a reset", "General"],
    ]);
    act(() => reset[0]?.run());
    expect(app.ui.getState().screen).toBe("settings");
    expect(app.ui.getState().selected.settings).toBe("general/panels");
    const project = catalogue(app, "muster", []).find((item) => item.id.startsWith("settings:"));
    expect(project).toMatchObject({ label: "muster", meta: "Settings" });
    act(() => project?.run());
    expect(app.ui.getState().selected.settings).toBe(`project:${ulid(1)}`);
  });
});

describe("PRD-01 the first run and the daemon out of reach", () => {
  it("ARC-33 rounds up the sessions, then welcomes a first project, a space named first", async () => {
    const world = workingDay();
    world.projects = [];
    world.workspaces = [];
    world.sessions = [];
    const bridge = new MockBridge(world);
    renderApp(bridge);
    expect(await screen.findByRole("heading", { name: "Add a project to start" })).toBeVisible();
    expect(screen.getByText("Or drop a folder on this window.")).toBeVisible();
    await userEvent.keyboard("a");
    // Everything lives in a space: one is named first, then the project is added in it.
    const named = await screen.findByRole("dialog", { name: "New space" });
    expect(named).toHaveTextContent("Everything lives in a space");
    await userEvent.type(within(named).getByLabelText("Name"), "Home");
    await userEvent.click(within(named).getByRole("button", { name: /Make it/ }));
    expect(await screen.findByRole("dialog", { name: "Add a project" })).toBeVisible();
    expect(bridge.world.spaces?.map((space) => space.name)).toEqual(["Home"]);
  });

  it("waits for the daemon where nothing was known yet, saying since when and where its log is", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.open = async (onMessage) => {
      onMessage({ kind: "link", link: { state: "down", reason: "no daemon answers.", since: 0 } });
    };
    renderApp(bridge);
    expect(await screen.findByRole("heading", { name: "Muster's daemon is away" })).toBeVisible();
    expect(screen.getByRole("status", { name: "Waiting for the daemon" })).toBeVisible();
    expect(screen.getByText(/^no daemon answers\. Out of reach since/)).toHaveTextContent(
      "Its log is daemon.log in Muster's runtime folder.",
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Muster's daemon is out of reach.");
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
