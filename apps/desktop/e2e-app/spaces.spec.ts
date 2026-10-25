import { $, $$, browser, expect } from "@wdio/globals";
import { muster, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { button, dialog, field, fill, goTo, noDialog, pressOnWindow } from "./support/window";

// Spaces in the real app over a real daemon (PRD §2, §6a, ARC-33):
// everything lives in exactly one space. Everything lists each space under
// its heading; choosing one lists only what is in it. A new terminal goes in
// the selected line's space and folds away with its heading. A project's
// menu moves it to another space or a new one; a space heading's menu adds
// to it, renames it, and removes it only once empty. ] and [ go round, and
// the window opens on everything.

const line = (label: string) => $(`[role="treeitem"][aria-label="${label}"]`);
const lineLike = (start: string) => $(`[role="treeitem"][aria-label^="${start}"]`);
const heading = () => $('button[aria-label="Spaces"]');

/** The open menu's item that starts with `label`. */
async function item(label: string) {
  await until(async () => (await $$('[role="menuitem"]').length) > 0, "a menu to open");
  const items = await $$('[role="menuitem"]');
  for (const one of items) if ((await one.getText()).startsWith(label)) return one;
  throw new Error(`no menu item ${label}`);
}

/** Every menu item's words, its key after them. */
async function itemWords(): Promise<string[]> {
  const items = await $$('[role="menuitem"]');
  const words: string[] = [];
  for (const one of items) words.push(await one.getText());
  return words;
}

/** Right-clicks the line labelled `label`, as the mouse does. */
async function rightClick(label: string) {
  await expect(line(label)).toBeDisplayed();
  await browser.execute((wanted) => {
    const found = document.querySelector(`[role="treeitem"][aria-label="${wanted}"]`);
    const box = found?.getBoundingClientRect();
    found?.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        button: 2,
        clientX: (box?.left ?? 0) + 40,
        clientY: (box?.top ?? 0) + 10,
      }),
    );
  }, label);
}

/** The space named `name`, as `muster status` gives it. */
const spaceNamed = (name: string) => status().spaces?.find((one) => one.name === name);
const projectId = (name: string) => status().projects?.find((one) => one.name === name)?.id ?? "?";

/** The tree's lines, top to bottom, by their labels. */
function lines(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[role="treeitem"]')].map(
      (one) => one.getAttribute("aria-label") ?? "",
    ),
  );
}

/** Shows `name` from the list's heading, or everything. */
async function show(name: string) {
  await heading().click();
  await (await item(name)).click();
  await expect(heading()).toHaveText(expect.stringContaining(name));
}

/** The id of the terminal made in a space, for the tests after it. */
let terminal = "";
const terminalTitle = () => status().terminals?.find((one) => one.id === terminal)?.title ?? "?";
const terminalLine = () => lineLike(`Shell: ${terminalTitle()}`);

describe("Spaces, where everything lives, over the real daemon", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    // With no space, the daemon makes a Home for the first project.
    muster("project", "add", repository("web"));
    muster("project", "add", repository("api"));
    muster("space", "add", "Client");
    muster("space", "mv", "api", "Client");
    await goTo("Sessions");
    await expect(line("web")).toBeDisplayed();
  });

  it("ARC-33 everything lists each space under its heading, and a space chosen lists only what it holds", async () => {
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await until(async () => {
      const shown = await lines();
      return (
        shown.indexOf("web") === shown.indexOf("Home") + 1 &&
        shown.indexOf("api") === shown.indexOf("Client") + 1
      );
    }, "each project under its space's heading");
    await show("Client");
    await expect(line("api")).toBeDisplayed();
    await expect(line("web")).not.toBeExisting();
    // A space alone has no heading of its own.
    await expect(line("Client")).not.toBeExisting();
    await show("Everything");
    await expect(line("web")).toBeDisplayed();
  });

  it("ARC-33 a terminal goes in the selected line's space, and folds away with its heading", async () => {
    await line("api").click();
    const before = new Set((status().terminals ?? []).map((one) => one.id));
    await $('button[aria-label="Add a project"]').click();
    await (await item("Terminal in no project")).click();
    const made = await until(
      () => status().terminals?.find((one) => !before.has(one.id)),
      "the new terminal",
    );
    terminal = made.id;
    await until(() => spaceNamed("Client")?.members.includes(terminal), "the terminal in Client");
    await pressOnWindow("Q");
    await expect(terminalLine()).toBeDisplayed();
    await line("Client").click();
    await pressOnWindow("e");
    await expect(terminalLine()).not.toBeExisting();
    await expect(line("api")).not.toBeExisting();
    await expect(line("web")).toBeDisplayed();
    await pressOnWindow("e");
    await expect(terminalLine()).toBeDisplayed();
  });

  it("ARC-33 a project's menu moves it; a space's heading offers what goes in it and is removed only once empty", async () => {
    await rightClick("web");
    await (await item("Move to Client")).click();
    await until(
      () => spaceNamed("Client")?.members.includes(projectId("web")),
      "web moved to Client",
    );
    expect(spaceNamed("Home")?.members).toEqual([]);

    await rightClick("Client");
    await until(async () => (await itemWords()).length > 0, "the space's menu");
    const offered = await itemWords();
    const wanted = [
      "Open a folder…",
      "Clone from GitHub…",
      "Terminal",
      "Rename Client…",
      "Remove Client…",
    ];
    expect(offered).toHaveLength(wanted.length);
    wanted.forEach((words, at) => expect(offered[at]?.startsWith(words)).toBe(true));
    await (await item("Remove Client…")).click();
    const asked = dialog("Remove Client?");
    await expect(asked).toBeDisplayed();
    await button(asked, "Remove the space").click();
    await expect(asked.$('.//*[@role="alert"]')).toHaveText(
      expect.stringContaining("Client still holds 2 projects and 1 terminal"),
    );
    expect(spaceNamed("Client")).toBeDefined();
    await pressOnWindow("Escape");
    await until(noDialog, "the dialog to close");
  });

  it("ARC-33 a project moves to a new space, and an emptied space is renamed and removed", async () => {
    await rightClick("api");
    await (await item("Move to a new space…")).click();
    const named = dialog("New space");
    await expect(named).toBeDisplayed();
    await fill(field(named, "Name"), "Office");
    await button(named, "Make it").click();
    await until(noDialog, "the dialog to close");
    await until(
      () => spaceNamed("Office")?.members.includes(projectId("api")),
      "api in the new space",
    );

    await rightClick("Home");
    await (await item("Rename Home…")).click();
    const renaming = dialog("Rename Home");
    await fill(field(renaming, "Name"), "Spare");
    await button(renaming, "Rename").click();
    await until(noDialog, "the dialog to close");
    await until(() => spaceNamed("Spare"), "the daemon to hold the new name");

    await rightClick("Spare");
    await (await item("Remove Spare…")).click();
    await button(dialog("Remove Spare?"), "Remove the space").click();
    await until(noDialog, "the dialog to close");
    await until(() => !spaceNamed("Spare"), "the empty space removed");
    await expect(line("Spare")).not.toBeExisting();
  });

  it("ARC-33 ] and [ go round the spaces, everything between the last and the first", async () => {
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Client"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Office"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await pressOnWindow("[");
    await expect(heading()).toHaveText(expect.stringContaining("Office"));
  });

  it("ARC-33 the window opens on everything, whatever space it showed", async () => {
    await expect(heading()).toHaveText(expect.stringContaining("Office"));
    await browser.refresh();
    await goTo("Sessions");
    await expect(heading()).toBeDisplayed();
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await expect(line("Client")).toBeDisplayed();
    await expect(line("Office")).toBeDisplayed();
  });
});
