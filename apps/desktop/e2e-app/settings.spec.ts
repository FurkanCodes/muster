import { chmodSync, existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { startAgent } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, project, session, settingsToml, status, until } from "./support/muster";
import { heard, opened } from "./support/record";
import { repository } from "./support/repos";
import { bareId, pressOnWindow, type } from "./support/window";

// Settings in the real app over a real daemon (PRODUCT.md §6 Settings,
// ARC-24): each row of General, Notifications, Agents and Naming chats
// changes Muster's own settings.toml, each row of a project's page the
// project's own toml, r reads a file changed by hand, Restart in place
// moves the daemon with its sessions running, and what would reach
// outside the app (a sound, System Settings, the editor) is recorded.

let repo = "";

const note = () => $(".mu-statusbar .mu-note");
const row = (key: string) => $(`[data-row="${key}"]`);
const named = (name: string) => $(`aria/${name}`);
/** An option of the menu open, by its words. */
const option = (words: string) => $(`//*[@role="option"][contains(., "${words}")]`);
/** Clicks what `find` finds, again if the window drew it afresh as it was clicked. */
async function steadyClick(find: () => ReturnType<typeof $>): Promise<void> {
  await until(async () => {
    try {
      await find().click();
      return true;
    } catch {
      return false;
    }
  }, "a click to land");
}

/** A Select, by what it chooses. */
const select = (label: string) => $(`button.mu-select[aria-label="${label}"]`);

/** The lines of `[section]` in `text`, up to the next section. */
function section(text: string, name: string): string {
  const at = text.indexOf(`[${name}]`);
  if (at < 0) return "";
  const rest = text.slice(at + name.length + 2);
  const next = rest.search(/^\[/m);
  return next < 0 ? rest : rest.slice(0, next);
}

/** Waits until settings.toml's `[name]` holds `line`. */
async function untilWritten(name: string, line: RegExp): Promise<void> {
  try {
    await until(() => line.test(section(settingsToml(), name)), `[${name}] to hold ${line}`);
  } catch (error) {
    throw new Error(`${String(error)}; settings.toml holds:\n${settingsToml()}`);
  }
}

/** The project's own overrides of its muster.toml, as Muster wrote them. */
function projectToml(name: string): string {
  const id = project(name)?.id ?? "";
  const folder = join(sandbox().config, "projects");
  if (!existsSync(folder)) return "";
  const file = readdirSync(folder).find(
    (found) => found.startsWith(id) || found.includes(bareId(id)),
  );
  return file ? readFileSync(join(folder, file), "utf8") : "";
}

/** Writes Muster's own settings by hand, as the person's editor would. */
function writeSettings(text: string): void {
  writeFileSync(join(sandbox().config, "settings.toml"), text);
}

async function openPage(name: string): Promise<void> {
  await pressOnWindow("⌘,");
  await $('[aria-label="Settings sections"]').$(`button*=${name}`).click();
  await expect($(`h2=${name}`)).toBeDisplayed();
}

describe("Settings over the real daemon", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("site");
    muster("project", "add", repo);
    muster("project", "add", repository("spare"));
    // An agent of the person's that says its version, one not installed.
    const bin = join(sandbox().root, "bin");
    writeFileSync(join(bin, "pi-agent"), "#!/bin/sh\necho 'pi 1.2.0'\n");
    chmodSync(join(bin, "pi-agent"), 0o755);
  });

  it("General: lines for each session, colours and the width a session sits beside from, in [look] and [layout]", async () => {
    await openPage("General");
    await row("general/session_lines").click();
    await browser.keys("ArrowRight");
    await expect(note()).toHaveText("Each session takes one line.");
    await untilWritten("look", /session_lines = 1/);
    await row("general/colours").click();
    await browser.keys("Enter");
    await expect($('[role="listbox"][aria-label="Colours"]')).toBeDisplayed();
    await browser.keys(["ArrowDown", "Enter"]);
    await expect(note()).toHaveText("Muster draws in your terminal's colours.");
    await untilWritten("look", /colours = "terminal"/);
    await row("general/side_by_side").click();
    await browser.keys("ArrowRight");
    await expect(note()).toHaveText(
      "A session sits beside the list in a window of 110 columns or more.",
    );
    await untilWritten("layout", /side_by_side_from = 110/);
  });

  it("General: r reads settings.toml changed by hand, and the panels are put back as at first", async () => {
    writeSettings(
      '[look]\nsession_lines = 2\n\n[layout]\npanels = "session | list"\nlist_folded = true\n',
    );
    await pressOnWindow("r");
    await expect(note()).toHaveText("Read the settings again.");
    await expect($("button=Two")).toHaveAttribute("aria-pressed", "true");
    // As the daemon reads the layout: the list at the right, folded away.
    await expect(row("general/panels")).toHaveText(expect.stringContaining("the list folded away"));
    await named("Put back as at first").click();
    await expect(note()).toHaveText("The panels are back where they were at first.");
    await until(
      () =>
        !/panels = "session \| list"/.test(settingsToml()) &&
        !/list_folded = true/.test(settingsToml()),
      `the panels to be put back in settings.toml:\n${settingsToml()}`,
    );
  });

  it("General: the theme is this window's own, never written to settings.toml", async () => {
    const before = settingsToml();
    await $('[role="group"][aria-label="Theme"]').$("button=Light").click();
    await expect($("html")).toHaveAttribute("data-theme", "light");
    await $('[role="group"][aria-label="Theme"]').$("button=Dark").click();
    await expect($("html")).toHaveAttribute("data-theme", "dark");
    expect(settingsToml()).toBe(before);
  });

  it("General: Restart in place moves the daemon onto the muster installed, its sessions running on", async () => {
    await startAgent("site", "Runs across the restart");
    const before = status();
    const kept = session("Runs across the restart", before);
    await openPage("General");
    await named("Restart in place").click();
    await expect(note()).toHaveText(expect.stringContaining("The daemon is now muster"));
    const after = status();
    expect(after.daemon?.pid).toBe(before.daemon?.pid);
    expect(session("Runs across the restart", after)?.id).toBe(kept?.id);
    expect(session("Runs across the restart", after)?.status).not.toBe("exited");
    // The window's link, dropped as the daemon moved, is up again (it tries every half second).
    await browser.pause(1500);
    await expect($("footer.mu-statusbar")).toHaveText(expect.stringContaining("Muster 0.4.1"));
  });

  it("Notifications: off and on in [notifications], a sound chosen is played and kept, the notch off", async () => {
    await openPage("Notifications");
    const told = $('[role="switch"][aria-label="Notifications"]');
    await told.click();
    await expect(note()).toHaveText(
      "Nothing is told: the waiting count and Needs you still say it.",
    );
    await untilWritten("notifications", /show = false/);
    // The row takes no second press until the daemon has answered the first.
    await expect(told).toHaveAttribute("aria-checked", "false");
    await until(async () => (await told.getAttribute("aria-busy")) !== "true", "the answer");
    await told.click();
    await untilWritten("notifications", /show = true/);
    await expect(told).toHaveAttribute("aria-checked", "true");

    await select("Sound for errors").click();
    await expect($('[role="listbox"][aria-label="Sound for errors"]')).toBeDisplayed();
    // From the Mac's default, past Basso, to Blow.
    await browser.keys(["ArrowDown", "ArrowDown"]);
    await expect(note()).toHaveText("Playing Blow");
    expect(heard().some((file) => file.endsWith("Basso.aiff"))).toBe(true);
    await until(() => heard().some((file) => file.endsWith("Blow.aiff")), "Blow to be played");
    await browser.keys("Enter");
    await expect(note()).toHaveText("Errors are told, with Blow.");
    await untilWritten("notifications", /errors_sound = "Blow"/);

    await $('[role="switch"][aria-label="Agents in the notch"]').click();
    await expect(note()).toHaveText("Muster leaves the notch alone.");
    await untilWritten("notch", /show = false/);
  });

  it("Notifications: Open System Settings opens the Mac's notifications at Muster's helper", async () => {
    await named("Open System Settings").click();
    await until(
      () =>
        opened().includes(
          "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.muster.Muster",
        ),
      "System Settings to be opened",
    );
    await expect(note()).toHaveText("Opened System Settings at Muster's notifications.");
  });

  it("Agents: where each is installed as the daemon finds it, its version, one not found, and settings.toml in the editor", async () => {
    writeSettings(
      `${settingsToml()}\n[agents.pi]\nlabel = "Pi"\nprogram = "pi-agent"\n\n[agents.ghost]\nlabel = "Ghost"\nprogram = "ghost-agent"\n`,
    );
    await openPage("Agents");
    await pressOnWindow("r");
    const bin = join(sandbox().root, "bin");
    const pi = $(".mu-agentrow*=Pi");
    await expect(pi).toHaveText(expect.stringContaining("1.2.0"));
    await expect(pi).toHaveText(
      expect.stringContaining(join(bin, "pi-agent").replace(/^\/(Users|home)\/[^/]+/, "~")),
    );
    await expect($(".mu-agentrow*=Ghost")).toHaveText(expect.stringContaining("✗ not found"));
    // Muster's own, the stand-ins on the daemon's PATH.
    await expect($(".mu-agentrow*=Claude Code")).toHaveText(expect.stringContaining("claude"));

    await named("Edit settings.toml").click();
    const file = join(sandbox().config, "settings.toml");
    await until(() => edited().includes(file), "settings.toml to be opened in the editor");
    await expect(note()).toHaveText(expect.stringContaining("in your editor"));
  });

  it("Naming chats: by the agent picked and a model, in [names]; off leaves chats their prompt", async () => {
    await openPage("Naming chats");
    await select("Named by").click();
    await steadyClick(() => option("Claude Code"));
    await expect(note()).toHaveText("Chats are named by the agent shown.");
    await untilWritten("names", /agent = "claude"/);
    await select("Its model").click();
    await steadyClick(() => option("Type one…"));
    await type('input[aria-label="Its model, typed"]', "claude-haiku-5-5");
    await browser.keys("Enter");
    await untilWritten("names", /model = "claude-haiku-5-5"/);
    await $('[role="switch"][aria-label="Chats named by an agent"]').click();
    await expect(note()).toHaveText("Chats keep the first line of their first prompt.");
    await untilWritten("names", /by_agent = false/);
  });

  it("a project's page: only my tickets, branches with muster/ and where its tickets come from, in its own toml", async () => {
    await openPage("site");
    // Its settings read, as the page asks for them on its way in.
    await expect(row(`project:${bareId(project("site")?.id ?? "")}/tickets_from`)).toHaveText(
      expect.stringContaining("GitHub"),
    );
    await steadyClick(() => $('[role="switch"][aria-label="Only my tickets"]'));
    await expect(note()).toHaveText("Tickets for site now lists only the tickets assigned to you.");
    await until(() => /mine = true/.test(projectToml("site")), "mine in the project's toml");
    await steadyClick(() => $('[role="switch"][aria-label="Branches with muster/"]'));
    await until(
      () => /muster_prefix = false/.test(projectToml("site")),
      `muster_prefix in the project's toml:\n${projectToml("site")}`,
    );
    await steadyClick(() => select("Tickets from"));
    await steadyClick(() => option("Linear"));
    await expect(note()).toHaveText("Linear needs its Team key, such as ENG.");
    // The cursor is in the field the tracker needs: typed there, as the person does.
    await expect($('input[id$="-team"]')).toBeFocused();
    await browser.keys(["E", "N", "G", "Enter"]);
    await expect(note()).toHaveText("site's tickets now come from Linear, team ENG.");
    await until(
      () =>
        /tracker = "linear"/.test(projectToml("site")) && /team = "ENG"/.test(projectToml("site")),
      `the tracker in the project's toml:\n${projectToml("site")}`,
    );
    // The repository's own muster.toml is never written.
    expect(existsSync(join(repo, "muster.toml"))).toBe(false);
  });

  it("a project's page: removed from Muster after asking, its folder kept", async () => {
    await openPage("spare");
    await named("Remove…").click();
    await expect($("dialog")).toHaveText(expect.stringContaining("Remove spare from Muster?"));
    await $("dialog").$("button=Remove spare").click();
    await expect(note()).toHaveText(
      "Muster no longer tracks the project; its folder is untouched.",
    );
    await until(() => !project("spare"), "the daemon to stop tracking spare");
    expect(existsSync(join(sandbox().root, "repos", "spare", "README.md"))).toBe(true);
  });
});

/** The files the app was asked to open in the editor (`e2e.rs`, `edited`). */
function edited(): string[] {
  const file = join(sandbox().record, "edited");
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean) : [];
}
