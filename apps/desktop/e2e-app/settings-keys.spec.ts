import { $, expect } from "@wdio/globals";
import { askingSession } from "./support/agents";
import { muster, settingsToml, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { goTo, pressOnWindow } from "./support/window";

// Settings › Keys in the real app over a real daemon (PRODUCT.md §6 Keys,
// ARC-24): a key pressed is the action's, written to [keys] in
// settings.toml and answered at once by the screens; the keys the TUI
// refuses are refused here in its words; ⌫ puts an action's keys back and
// ⇧R every key, each taken out of the file.

const note = () => $(".mu-statusbar .mu-note");
const key = (name: string) => $(`[data-row="keys/key.${name}"]`);

/** settings.toml's [keys], as Muster wrote it. */
function keys(): string {
  const text = settingsToml();
  const at = text.indexOf("[keys]");
  if (at < 0) return "";
  const rest = text.slice(at + "[keys]".length);
  const next = rest.search(/^\[/m);
  return next < 0 ? rest : rest.slice(0, next);
}

async function openKeys(): Promise<void> {
  await pressOnWindow("⌘,");
  await $('[aria-label="Settings sections"]').$("button*=Keys").click();
  await expect($("h2=Keys")).toBeDisplayed();
}

describe("Settings › Keys over the real daemon", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    muster("project", "add", repository("app"));
    await askingSession("app", "Asks for a name", "May I?");
    await openKeys();
  });

  it("ARC-24 sets a key by pressing it, refusing one another action has on the same screen, and writes it", async () => {
    await key("new_session").click();
    await expect(key("new_session")).toHaveElementClass("is-waiting");
    await pressOnWindow("d");
    await expect($('[role="alert"]')).toHaveText(
      "d already does “What the worktree or session has changed, and its turns” on this screen. Press another key.",
    );
    expect(keys()).not.toMatch(/new_session/);
    await pressOnWindow("N");
    await expect(note()).toHaveText("The key is set.");
    await until(() => /new_session = \["N"\]/.test(keys()), `new_session in [keys]:\n${keys()}`);
    await expect(key("new_session")).toHaveText(expect.stringContaining("yours"));
  });

  it("ARC-24 refuses the keys every screen keeps, ⌘'s, and typing inside a terminal; ⎋ leaves them", async () => {
    await key("leave_session").click();
    await pressOnWindow("x");
    await expect($('[role="alert"]')).toHaveText(
      expect.stringContaining(
        "x would reach the program as typing. A key inside a terminal needs ⌃ or ⌥",
      ),
    );
    await pressOnWindow("↑");
    await expect($('[role="alert"]')).toHaveText(
      "↑ is a key that moves about, the same on every screen. Press another key.",
    );
    await pressOnWindow("⌘K");
    await expect($('[role="alert"]')).toHaveText(
      "⌘ keys are this window's own. Press a key with ⌃ or ⌥, or none.",
    );
    await pressOnWindow("Esc");
    await expect(note()).toHaveText("Its keys stay as they are.");
    expect(keys()).not.toMatch(/leave_session/);
  });

  it("ARC-24 a key set is the one the screens answer at once", async () => {
    await key("rename").click();
    await pressOnWindow("E");
    await expect(note()).toHaveText("The key is set.");
    await until(() => /rename = \["E"\]/.test(keys()), `rename in [keys]:\n${keys()}`);
    await goTo("Needs you");
    await pressOnWindow("R");
    await expect($('input[name="session-name"]')).not.toBeExisting();
    await pressOnWindow("E");
    await expect($('input[name="session-name"]')).toBeDisplayed();
    await pressOnWindow("Esc");
    await expect($("footer.mu-statusbar")).toHaveText(expect.stringContaining("⇧Erename"));
  });

  it("ARC-24 ⌫ puts an action's keys back, and ⇧R every key, out of settings.toml", async () => {
    await openKeys();
    await key("rename").click();
    await pressOnWindow("Esc", "Backspace");
    await expect(note()).toHaveText("Its keys are back as Muster has them.");
    await until(() => !/rename =/.test(keys()), `rename out of [keys]:\n${keys()}`);
    expect(keys()).toMatch(/new_session/);
    await pressOnWindow("R");
    await expect(note()).toHaveText("Every key is back as Muster has it.");
    await until(() => !/\[keys\]/.test(settingsToml()) || keys().trim() === "", "no [keys] left");
    await expect(key("new_session").$(".mu-kbd")).toHaveText("n");
  });
});
