import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { sandbox } from "./support/harness";
import { muster, settingsToml, status, until } from "./support/muster";
import { pressOnWindow, type } from "./support/window";

// Settings › Plugins in the real app over a real daemon (PRD-15, ARC-24):
// plugins from folders of the sandbox's own, linked after everything they
// run is shown and approved, turned off and on, approved again once their
// manifest changes, and removed, each checked in the daemon's own list and
// in settings.toml; and the daemon's refusals, said in its words.

/** A plugin as `muster plugin list --json` writes it. */
interface Listed {
  id: string;
  enabled: boolean;
  runs: boolean;
  problem: unknown;
  version: string;
}

/** The plugins as the daemon lists them (`muster plugin list --json`). */
function plugins(): Listed[] {
  return (JSON.parse(muster("plugin", "list", "--json")) as { plugins: Listed[] }).plugins;
}

function listed(id: string): Listed | undefined {
  return plugins().find((plugin) => plugin.id === id);
}

const note = () => $(".mu-statusbar .mu-note");
const row = (id: string) => $(`[data-row="plugins/plugin.${id}"]`);

/** A plugin's folder in the sandbox: its manifest, and a program for its panel and its hook. */
function folder(name: string, manifest: string): string {
  const root = join(sandbox().root, "plugins", name);
  mkdirSync(root, { recursive: true });
  writeFileSync(join(root, "muster-plugin.toml"), manifest);
  for (const program of ["panel", "note"]) {
    writeFileSync(join(root, program), "#!/bin/sh\nexec /bin/sh\n");
    chmodSync(join(root, program), 0o755);
  }
  return root;
}

function notesManifest(version: string, extra = ""): string {
  return `id = "notes"
name = "Notes"
version = "${version}"
description = "A panel of notes, and a line when an agent finishes."
muster = "0.4.0"

[[panels]]
id = "pad"
title = "pad"
command = ["./panel", "{branch}"]
key = "N"

[[hooks]]
id = "said"
on = ["session.finished"]
command = ["./note", "{project}"]
${extra}`;
}

/** Adds the folder `path` from the Add dialog, and waits for what the daemon shows of it. */
async function add(path: string): Promise<void> {
  await pressOnWindow("a");
  await expect($("dialog")).toHaveText(expect.stringContaining("Add a plugin"));
  await type("dialog input", path);
  await browser.keys("Enter");
}

describe("Settings › Plugins over the real daemon", () => {
  let notes = "";

  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    notes = folder("notes", notesManifest("0.1.0"));
    await pressOnWindow("⌘,");
    await $('[aria-label="Settings sections"]').$("button*=Plugins").click();
    await expect($("h2=Plugins")).toBeDisplayed();
  });

  it("PRD-15 links a folder after showing everything it runs, and turns it on", async () => {
    await add(notes);
    const preview = $("dialog");
    await expect(preview).toHaveText(expect.stringContaining("Link Notes?"));
    await expect(preview).toHaveText(expect.stringContaining("./panel {branch}"));
    await expect(preview).toHaveText(expect.stringContaining("./note {project}"));
    await expect(preview).toHaveText(
      expect.stringContaining("linked where it is and read from there"),
    );
    await preview.$("button=Link and turn on").click();
    await expect(note()).toHaveText(expect.stringContaining("notes"));
    await until(() => listed("notes")?.enabled, "the daemon to list notes, on");
    expect(settingsToml()).toMatch(/\[plugins\.notes\]/);
    await expect(row("notes")).toHaveText(expect.stringContaining("panel: pad"));
  });

  it("PRD-15 says nothing was added when the person says no", async () => {
    const other = folder("other", notesManifest("0.1.0").replace('id = "notes"', 'id = "other"'));
    await add(other);
    await expect($("dialog")).toHaveText(expect.stringContaining("Link Notes?"));
    await browser.keys("n");
    await expect(note()).toHaveText("Nothing was added.");
    expect(listed("other")).toBeUndefined();
  });

  it("ARC-24 turns one off and on, as the daemon keeps it", async () => {
    const toggle = $('[role="switch"][aria-label="Notes"]');
    await toggle.click();
    await expect(note()).toHaveText("Turned notes off.");
    await until(() => listed("notes")?.enabled === false, "notes to be off in the daemon");
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await until(async () => (await toggle.getAttribute("aria-busy")) !== "true", "the answer");
    await toggle.click();
    await until(() => listed("notes")?.enabled === true, "notes to be on again");
  });

  it("PRD-15 a manifest changed is off until approved again, showing what it runs now", async () => {
    writeFileSync(
      join(notes, "muster-plugin.toml"),
      notesManifest("0.2.0").replace(
        'command = ["./note", "{project}"]',
        'command = ["./note", "{project}", "{session}"]',
      ),
    );
    // The screen reads the plugins again as it is shown; r does too.
    await pressOnWindow("r");
    await expect(row("notes")).toHaveText(expect.stringContaining("Off:"));
    // Not turned on as it is now: it is approved first.
    await $('[role="switch"][aria-label="Notes"]').click();
    await expect(note()).toHaveText(expect.stringContaining("notes cannot be turned on"));
    // On as the person left it, but not run: changed since they approved it.
    expect(listed("notes")?.problem).not.toBeNull();
    await row("notes").$("button*=Notes").click();
    await pressOnWindow("u");
    const preview = $("dialog");
    await expect(preview).toHaveText(expect.stringContaining("0.2.0"));
    await expect(preview).toHaveText(expect.stringContaining("./note {project} {session}"));
    await browser.keys("y");
    await expect(preview).not.toBeExisting();
    try {
      await until(
        () => listed("notes")?.runs && listed("notes")?.version === "0.2.0",
        "notes 0.2.0 approved and running",
      );
    } catch (error) {
      throw new Error(`${String(error)}: the daemon lists ${JSON.stringify(listed("notes"))}`);
    }
    await expect(row("notes")).not.toHaveText(expect.stringContaining("Off:"));
  });

  it("ARC-24 the daemon's refusals: a manifest that does not read, a path not from /, Muster's own", async () => {
    const broken = folder("broken", 'id = "broken"\nname = 7\n');
    await add(broken);
    await expect($("dialog [role='alert']")).toHaveText(expect.stringContaining("name"));
    await type("dialog input", "~/plugins/mine");
    await browser.keys("Enter");
    await expect($("dialog [role='alert']")).toHaveText(
      expect.stringContaining("Type the folder's whole path, from /"),
    );
    await browser.keys("Escape");
    await expect($("dialog")).not.toBeExisting();

    await row("github").$("button*=GitHub").click();
    await pressOnWindow("x");
    await expect(note()).toHaveText(
      "github is Muster's own: it cannot be removed, only turned off.",
    );
    await pressOnWindow("u");
    await expect(note()).toHaveText("github is Muster's own: it updates with Muster.");
    expect(listed("broken")).toBeUndefined();
  });

  it("PRD-15 the approval shows a program it needs that is not found, as the daemon looks for it", async () => {
    const needy = folder(
      "needy",
      notesManifest("0.1.0")
        .replace('id = "notes"', 'id = "needy"')
        .replace('name = "Notes"', 'name = "Needy"')
        .replace('muster = "0.4.0"', 'muster = "0.4.0"\nrequires = ["no-such-program"]')
        .replace('key = "N"', 'key = "Y"'),
    );
    await add(needy);
    const preview = $("dialog");
    await expect(preview).toHaveText(expect.stringContaining("Link Needy?"));
    await expect(preview).toHaveText(expect.stringContaining("no-such-program"));
    await expect(preview).toHaveText(expect.stringContaining("not found"));
    await browser.keys("n");
    expect(listed("needy")).toBeUndefined();
  });

  it("PRD-15 removes one after asking; its folder stays", async () => {
    await row("notes").$("button*=Notes").click();
    await pressOnWindow("x");
    const dialog = $("dialog");
    await expect(dialog).toHaveText(expect.stringContaining("Remove notes?"));
    await dialog.$("button=Remove notes").click();
    await expect(note()).toHaveText("Removed notes.");
    await until(() => !listed("notes"), "the daemon to forget notes");
    expect(settingsToml()).not.toMatch(/\[plugins\.notes\]/);
    expect(existsSync(join(notes, "muster-plugin.toml"))).toBe(true);
  });
});
