import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { ask, launches } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, project, sessions, status, until, untilSession } from "./support/muster";
import { opened } from "./support/record";
import {
  branches,
  ghCalls,
  repository,
  tickets,
  ticketsClosed,
  trackerDown,
} from "./support/repos";
import { installTicketPlugin } from "./support/ticket-plugin";
import {
  bareId,
  button,
  dialog,
  field,
  noDialog,
  option,
  press,
  pressOnWindow,
  sendCommand,
} from "./support/window";

// Tickets over the real daemon and the stand-in gh (PRODUCT.md §6a,
// design/screens/Tickets and Assign): each project's open tickets, Mine and
// All kept in the project's settings, r reading the tracker again, a
// tracker out of reach and a project with none, a ticket's text shown as
// text, agents assigned (a worktree on the ticket's branch, the ticket as
// the first prompt), on top of another branch and another beside it, their
// marks on the ticket, b in the browser, closing it in the tracker and the
// clean up, and projects sharing a tracker in one group.

const HOSTILE =
  '<img src="https://untrusted.invalid/image"><script>window.ticketRan=true</script>\n\n![pixel](https://untrusted.invalid/pixel)\n\n[Read more](https://untrusted.invalid/read)';

const ticket = (key: string) => $(`//main//button[starts-with(@aria-label, "${key} ")]`);
const group = (name: string) => $(`//main//section[@aria-label="${name}'s tickets"]`);
const body = () => $("//main//article");

async function toTickets(): Promise<void> {
  await pressOnWindow("⌘3");
  await expect($("h1=Tickets")).toBeDisplayed();
}

/** The worktree of `app` on a branch for ticket `number`, once there is one. */
function ticketTrees(number: number) {
  return (project("app")?.workspaces ?? []).filter(
    (one) => one.kind === "worktree" && (one.branch ?? "").includes(`issue-${number}`),
  );
}

let app = "";

describe("Tickets", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    tickets([
      {
        number: 42,
        title: "Fix the login redirect",
        body: "After signing in, users land on /404.",
      },
      { number: 7, title: "Dark mode", body: HOSTILE, mine: true },
      { number: 9, title: "Faster start", body: "" },
    ]);
    app = repository("app");
    muster("project", "add", app);
    muster("project", "add", repository("site"));
    const plain = join(sandbox().root, "folders", "notes");
    mkdirSync(plain, { recursive: true });
    muster("project", "add", plain);
  });

  it("lists each project's open tickets from its tracker", async () => {
    await toTickets();
    for (const name of ["app", "site"]) {
      await expect(group(name)).toHaveText(expect.stringContaining("GitHub issues"));
      for (const key of ["#42", "#7", "#9"]) {
        await expect(
          group(name).$(`.//button[starts-with(@aria-label, "${key} ")]`),
        ).toBeDisplayed();
      }
    }
    expect(ghCalls().filter((call) => call.startsWith("issue list")).length).toBeGreaterThan(1);
  });

  it("says why a project has no tickets to show", async () => {
    await expect(group("notes").$('.//*[@role="alert"]')).toHaveText(
      expect.stringContaining("this project has no git remote to find its tickets by"),
    );
  });

  it("r reads the tracker again, and says when it is out of reach", async () => {
    const before = ghCalls().length;
    trackerDown(true);
    await pressOnWindow("r");
    await expect(group("app").$('.//*[@role="alert"]')).toHaveText(
      expect.stringContaining("Its tickets could not be fetched:"),
    );
    expect(ghCalls().length).toBeGreaterThan(before);
    trackerDown(false);
    await pressOnWindow("r");
    await expect(ticket("#42")).toBeDisplayed();
  });

  it("Mine and All change which tickets, kept in the project's settings", async () => {
    const id = project("app")?.id ?? "";
    const file = join(sandbox().config, "projects", `${id}.toml`);
    await group("app").$('.//*[@aria-label="Which of app\'s tickets"]').$("button=Mine").click();
    await until(
      () => existsSync(file) && readFileSync(file, "utf8").includes("mine = true"),
      "mine = true in the project's settings",
    );
    await expect(group("app")).toHaveText(expect.stringContaining("only yours"));
    await expect(group("app").$('.//button[starts-with(@aria-label, "#42 ")]')).not.toBeExisting();
    await expect(group("app").$('.//button[starts-with(@aria-label, "#7 ")]')).toBeDisplayed();
    expect(ghCalls().some((call) => call.includes("--assignee @me"))).toBe(true);
    await group("app").$('.//*[@aria-label="Which of app\'s tickets"]').$("button=All").click();
    await until(() => !readFileSync(file, "utf8").includes("mine = true"), "mine taken back");
    await expect(group("app").$('.//button[starts-with(@aria-label, "#42 ")]')).toBeDisplayed();
  });

  it("shows a ticket's text as text: no HTML, image or link loads or runs", async () => {
    await ticket("#7").click();
    await expect(body()).toHaveText(
      expect.stringContaining('<img src="https://untrusted.invalid/image">'),
    );
    await expect(body()).toHaveText(
      expect.stringContaining("![pixel](https://untrusted.invalid/pixel)"),
    );
    await expect(body()).toHaveText(
      expect.stringContaining("Read more (https://untrusted.invalid/read)"),
    );
    expect(await body().$$("img, script, a, iframe").length).toBe(0);
    const fetched = await browser.execute(() =>
      performance
        .getEntriesByType("resource")
        .map((entry) => entry.name)
        .filter((name) => name.includes("untrusted.invalid")),
    );
    expect(fetched).toEqual([]);
    expect(await browser.execute(() => "ticketRan" in window)).toBe(false);
  });

  it("assigns an agent: a worktree on the ticket's branch, the ticket its first prompt", async () => {
    await toTickets();
    await ticket("#42").click();
    await press("Enter");
    const assign = dialog("Assign an agent to #42");
    await expect(assign).toBeDisplayed();
    await button(assign, "Start").click();
    await until(noDialog, "the dialog to close");
    const [tree] = await until(
      () => (ticketTrees(42).length ? ticketTrees(42) : null),
      "#42's worktree",
    );
    if (!tree) throw new Error("no worktree");
    expect(branches(app)).toContain(tree.branch);
    const agent = await until(
      () => tree && project("app")?.workspaces.find((one) => one.id === tree.id)?.sessions[0],
      "its agent",
    );
    expect(agent.harness).toBe("claude");
    const launch = await until(() => launches(agent.id)[0], "the agent to start");
    expect(launch.cwd).toBe(tree.path);
    expect(launch.args.join(" ")).toContain("Fix the login redirect");
  });

  it("another agent on the same ticket starts in a worktree of its own", async () => {
    await toTickets();
    await ticket("#42").click();
    await pressOnWindow("a");
    const another = dialog("Assign an agent to #42");
    await expect(another).toBeDisplayed();
    await button(another, "Start").click();
    await until(noDialog, "the dialog to close");
    await until(() => ticketTrees(42).length === 2, "a second worktree for #42");
  });

  it("marks the agents on a ticket, and one needing the person", async () => {
    await toTickets();
    await until(
      async () => (await ticket("#42").$$(".mu-ticket-agent").length) === 2,
      "two agents on #42",
    );
    const [first] = ticketTrees(42).flatMap((tree) => tree.sessions);
    if (!first) throw new Error("no agent");
    await ask(await until(() => launches(first.id)[0], "its launch"), "Which page?");
    await untilSession(first.title, "needs_you");
    await expect(ticket("#42").$('[aria-label="needs you"]')).toBeDisplayed();
    // The row flashes as the queue's rows do when an agent on it comes to need the person.
    await expect(ticket("#42")).toHaveElementClass("mu-flash");
  });

  it("assigns an agent made on top of another worktree's branch", async () => {
    muster("workspace", "add", "--project", "app", "--branch", "feature/base");
    await until(
      () => project("app")?.workspaces.find((one) => one.branch === "feature/base"),
      "the base",
    );
    await toTickets();
    await ticket("#9").click();
    await press("Enter");
    const assign = dialog("Assign an agent to #9");
    await expect(assign).toBeDisplayed();
    await field(assign, "Made on top of").click();
    await option($('//*[@role="listbox"][@aria-label="Made on top of"]'), "feature/base").click();
    await button(assign, "Start").click();
    await until(noDialog, "the dialog to close");
    const [tree] = await until(
      () => (ticketTrees(9).length ? ticketTrees(9) : null),
      "#9's worktree",
    );
    expect(tree?.stacked_on).toBe("feature/base");
  });

  it("b opens the ticket in the browser, and only that", async () => {
    await toTickets();
    await ticket("#42").click();
    await pressOnWindow("b");
    await until(() => opened().includes("https://github.com/o/app/issues/42"), "#42 to be opened");
  });

  it("closes a ticket in its tracker, then cleans up: agents stopped, worktrees removed, branches kept", async () => {
    const trees = ticketTrees(42);
    const agents = trees.flatMap((tree) => tree.sessions.map((one) => one.id));
    await toTickets();
    await ticket("#42").click();
    await pressOnWindow("x");
    const close = dialog("Close #42 on GitHub?");
    await expect(close).toBeDisplayed();
    await button(close, "Close ticket").click();
    await until(() => ticketsClosed().includes(42), "gh to close #42");
    const cleanup = dialog("Closed #42. Clean up after it?");
    await expect(cleanup).toBeDisplayed();
    await press("Enter");
    await until(() => ticketTrees(42).length === 0, "#42's worktrees to go", 30_000);
    for (const id of agents) expect(sessions().find((one) => one.id === id)).toBeUndefined();
    for (const tree of trees) {
      expect(existsSync(tree.path)).toBe(false);
      expect(branches(app)).toContain(tree.branch);
    }
  });

  it("puts projects sharing a tracker in one group, naming them all", async () => {
    installTicketPlugin();
    for (const name of ["app", "site"]) {
      const id = bareId(project(name)?.id ?? "");
      await sendCommand({
        ChangeSetting: {
          project: id,
          change: { TicketsFrom: { tracker: "fixture", fields: { board: "WEB" } } },
        },
      });
    }
    await toTickets();
    await pressOnWindow("r");
    await expect(
      $(
        '//main//section[@aria-label="app, site\'s tickets" or @aria-label="site, app\'s tickets"]',
      ),
    ).toBeDisplayed();
    await expect(ticket("FIX-1")).toBeDisplayed();
    expect(await $$('//main//button[starts-with(@aria-label, "FIX-1 ")]').length).toBe(1);
  });
});
