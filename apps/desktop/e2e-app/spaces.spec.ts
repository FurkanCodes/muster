import { $, $$, browser, expect } from "@wdio/globals";
import { muster, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { button, dialog, field, fill, goTo, noDialog, pressOnWindow } from "./support/window";

// Spaces and terminals in no project in the real app over a real daemon
// (PRD §2, §6a, ARC-33): a space made from a shell names itself in the
// projects' heading; choosing it lists only what is in it, and everything
// again; a terminal in no project is listed under Terminals, and folds away.
// Then all from the window: a space made from the + menu, a terminal made
// in it joining it, taken out and put back from its menu, ] and [ going
// round, and the space renamed and removed from the heading; a project put
// in a space and taken out from its menu.

const line = (label: string) => $(`[role="treeitem"][aria-label="${label}"]`);
const lineLike = (start: string) => $(`[role="treeitem"][aria-label^="${start}"]`);
const heading = () => $('button[aria-label="Spaces"]');
/** The open menu's item that starts with `label`. */
async function item(label: string) {
  const items = await $$('[role="menuitem"]');
  for (const one of items) if ((await one.getText()).startsWith(label)) return one;
  throw new Error(`no menu item ${label}`);
}

describe("Spaces and terminals in no project, over the real daemon", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    muster("project", "add", repository("web"));
    muster("project", "add", repository("api"));
    await goTo("Sessions");
    await expect(line("web")).toBeDisplayed();
  });

  it("ARC-33 a space made from a shell is the projects' heading's, and shows only what is in it", async () => {
    await expect(heading()).not.toBeExisting();
    muster("space", "add", "Client");
    await expect(heading()).toBeDisplayed();
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await heading().click();
    await (await item("Client")).click();
    await expect(heading()).toHaveText(expect.stringContaining("Client"));
    await expect(line("web")).not.toBeExisting();
    await heading().click();
    await (await item("Everything")).click();
    await expect(line("web")).toBeDisplayed();
    await expect(line("api")).toBeDisplayed();
  });

  it("ARC-33 a terminal in no project is listed under Terminals, and folds away", async () => {
    muster("term", "new", "--free", "--bg", "scratch");
    const terminals = line("Terminals in no project");
    await expect(terminals).toBeDisplayed();
    await expect(lineLike("Shell: scratch")).toBeDisplayed();
    await until(
      () => status().terminals?.some((session) => session.title === "scratch"),
      "the daemon to hold a terminal in no project",
    );
    await terminals.click();
    await pressOnWindow("e");
    await expect(lineLike("Shell: scratch")).not.toBeExisting();
    await pressOnWindow("e");
    await expect(lineLike("Shell: scratch")).toBeDisplayed();
  });

  it("ARC-33 a space made from the + menu is shown at once, and a terminal made in it joins it", async () => {
    await $('button[aria-label="Add a project"]').click();
    await (await item("New space…")).click();
    const named = dialog("New space");
    await expect(named).toBeDisplayed();
    await fill(field(named, "Name"), "Work");
    await button(named, "Make it").click();
    await until(noDialog, "the dialog to close");
    await expect(heading()).toHaveText(expect.stringContaining("Work"));
    await expect(line("web")).not.toBeExisting();
    await expect(lineLike("Shell: scratch")).not.toBeExisting();

    const before = new Set((status().terminals ?? []).map((one) => one.id));
    await $('button[aria-label="Add a project"]').click();
    await (await item("Terminal in no project")).click();
    const made = await until(
      () => status().terminals?.find((one) => !before.has(one.id)),
      "the new terminal",
    );
    await until(
      () => spaceNamed("Work")?.members.includes(made.id),
      "the terminal to join the space shown",
    );
    terminal = made.id;
    await expect(terminalLine()).toBeDisplayed();
  });

  it("ARC-33 a terminal's menu takes it out of the space and puts it back", async () => {
    await menuOf(terminalTitle(), "Take out of Work");
    await until(
      () => !spaceNamed("Work")?.members.includes(terminal),
      "the terminal to leave the space",
    );
    await expect(terminalLine()).not.toBeExisting();
    await heading().click();
    await (await item("Everything")).click();
    await menuOf(terminalTitle(), "Put in Work");
    await until(
      () => spaceNamed("Work")?.members.includes(terminal),
      "the terminal to join the space again",
    );
    // Still running all along: a space only groups.
    expect(status().terminals?.some((one) => one.id === terminal)).toBe(true);
  });

  it("ARC-33 ] and [ go round the spaces, everything between the last and the first", async () => {
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Client"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Work"));
    await pressOnWindow("]");
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await pressOnWindow("[");
    await expect(heading()).toHaveText(expect.stringContaining("Work"));
  });

  it("ARC-33 the heading's menu renames the space shown", async () => {
    await heading().click();
    await (await item("Rename Work…")).click();
    const named = dialog("Rename Work");
    await expect(named).toBeDisplayed();
    await fill(field(named, "Name"), "Office");
    await button(named, "Rename").click();
    await until(noDialog, "the dialog to close");
    await expect(heading()).toHaveText(expect.stringContaining("Office"));
    await until(() => spaceNamed("Office"), "the daemon to hold the new name");
    expect(spaceNamed("Work")).toBeUndefined();
  });

  it("ARC-33 removing the space shown shows everything, and what was in it stays", async () => {
    await heading().click();
    await (await item("Remove Office…")).click();
    const asked = dialog("Remove Office?");
    await expect(asked).toBeDisplayed();
    await button(asked, "Remove the space").click();
    await until(noDialog, "the dialog to close");
    await until(() => !spaceNamed("Office"), "the daemon to let the space go");
    await expect(heading()).toHaveText(expect.stringContaining("Everything"));
    await expect(terminalLine()).toBeDisplayed();
    expect(status().terminals?.some((one) => one.id === terminal)).toBe(true);
  });

  it("ARC-33 a project's menu puts it in a space and takes it out: its line's right-click, its page's ⋯", async () => {
    const client = () => spaceNamed("Client");
    const api = () => status().projects?.find((one) => one.name === "api")?.id ?? "?";
    await rightClick("api");
    await (await item("Put in Client")).click();
    await until(() => client()?.members.includes(api()), "the project to join the space");
    await heading().click();
    await (await item("Client")).click();
    await expect(line("api")).toBeDisplayed();
    await expect(line("web")).not.toBeExisting();
    await line("api").click();
    await $('//section[@aria-label="api"]//button[@aria-label="More"]').click();
    await (await item("Take out of Client")).click();
    await until(() => !client()?.members.includes(api()), "the project to leave the space");
    await expect(line("api")).not.toBeExisting();
    // Only the group changed: the project is still Muster's.
    expect(status().projects?.some((one) => one.name === "api")).toBe(true);
    await heading().click();
    await (await item("Everything")).click();
  });
});

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

/** The id of the terminal made in a space, for the tests after it. */
let terminal = "";

/** That terminal's title now: a shell's follows what runs in it. */
function terminalTitle(): string {
  return status().terminals?.find((one) => one.id === terminal)?.title ?? "?";
}

/** That terminal's line, by its title now. */
const terminalLine = () => lineLike(`Shell: ${terminalTitle()}`);

/** The space named `name`, as `muster status` gives it. */
function spaceNamed(name: string) {
  return status().spaces?.find((one) => one.name === name);
}

/** Right-clicks the shell titled `title`'s line, as the mouse does, and chooses `label` of its menu. */
async function menuOf(title: string, label: string) {
  const start = `Shell: ${title}`;
  await expect(lineLike(start)).toBeDisplayed();
  await browser.execute((wanted) => {
    const found = document.querySelector(`[role="treeitem"][aria-label^="${wanted}"]`);
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
  }, start);
  await (await item(label)).click();
}
