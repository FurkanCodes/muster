import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { agentOf, ask, askingSession, show, startAgent, work } from "./support/agents";
import { keptState, restartApp, sandbox } from "./support/harness";
import { muster, session, settingsToml, status, until, untilSession } from "./support/muster";
import { daemonStarts, heard, holdDaemon, revealed } from "./support/record";
import { repository } from "./support/repos";
import {
  bareId,
  flock,
  goTo,
  press,
  pressOnWindow,
  shownScreen,
  terminalText,
  untilFlock,
} from "./support/window";

// The window as a whole over the real daemon (PRODUCT.md §6a, ARCHITECTURE.md
// §5c): the link's states and the daemon started by the app, the banner
// while it is out of reach with its log and Try now, the title bar, the
// sidebar and the window's keys, the theme and the colours programs are
// told, a window that cannot be seen streaming nothing and coming back
// current, a sound heard as it is chosen, and the status bar's notes said to
// a screen reader.

const kep = async () => (await $(".app-titlebar .mu-kep").getAttribute("aria-label")) ?? "";
const banner = () => $(".ui-flow-away");
const note = () => $('[data-said="note"]');
/** Text shown on the screen in view. */
const inMain = (text: string) => $(`//main//*[contains(text(), "${text}")]`);

async function openSettings(section: string): Promise<void> {
  await pressOnWindow("⌘,");
  await $('section[aria-label="Settings sections"]').$(`button*=${section}`).click();
}

/** The theme this window draws in, as the page says. */
const drawnIn = () => browser.execute(() => document.documentElement.dataset.theme ?? "");

/**
 * An agent of the person's own that asks its terminal for its background
 * (OSC 11) as it starts and keeps the answer: run unseen by the window, so
 * only the daemon can answer it, from the colours the window told it.
 */
function asker(): string {
  const answer = join(sandbox().root, "answers");
  mkdirSync(answer, { recursive: true });
  const script = join(sandbox().root, "bin", "asker");
  writeFileSync(
    script,
    `#!/bin/sh
[ -t 0 ] || exit 0
stty raw -echo
printf '\\033]11;?\\007'
dd bs=1 count=25 2>/dev/null > "${answer}/$MUSTER_SESSION"
stty sane
exec /bin/sh
`,
    { mode: 0o755 },
  );
  return answer;
}

describe("The window as a whole", () => {
  let app = "";

  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    app = repository("app");
    muster("project", "add", app);
    muster("project", "add", repository("other"));
  });

  it("starts the daemon it finds none of, once, and says the link is up", async () => {
    expect(daemonStarts()).toEqual(["daemon --detach"]);
    await expect($(".app-statusbar")).toHaveText(expect.stringContaining("Muster 0.4.1"));
    expect(await kep()).toBe("Nothing works or waits");
    await expect($("button.mu-command")).toBeEnabled();
  });

  it("counts the flock and Kep's mood in the title bar as the daemon has them, and the machine's memory", async () => {
    const asking = await askingSession("app", "Asking agent", "Which way?");
    await untilFlock(0, 1, 0);
    expect(await kep()).toBe("Something asks for you");
    const working = await startAgent("app", "Working agent");
    await work(working);
    await untilSession("Working agent", "running");
    await untilFlock(1, 1, 0);
    await work(asking);
    await untilSession("Asking agent", "running");
    await untilFlock(2, 0, 0);
    expect(await kep()).toBe("Kep is watching the flock");
    // The daemon's report of the machine's memory, as a meter.
    const meter = await $(".app-titlebar .mu-meter");
    await meter.waitForExist({ timeout: 30_000 });
    expect(await meter.getAttribute("aria-label")).toMatch(/^Memory \d+ percent used$/);
  });

  it("goes to each screen from the sidebar and on ⌘1, ⌘2, ⌘3 and ⌘,", async () => {
    for (const screen of ["Not seen", "Working", "All sessions", "Tickets", "Settings"] as const) {
      await goTo(screen);
      expect(await shownScreen()).toBe(screen);
    }
    for (const [key, screen] of [
      ["⌘1", "Needs you"],
      ["⌘2", "All sessions"],
      ["⌘3", "Tickets"],
      ["⌘,", "Settings"],
      ["⌘1", "Needs you"],
    ] as const) {
      await pressOnWindow(key);
      expect(await shownScreen()).toBe(screen);
    }
  });

  it("lists the projects with their sessions, narrows the screens to one, and folds on ⌘\\", async () => {
    const sidebar = $('nav[aria-label="Screens"]');
    const appItem = sidebar.$("button*=app");
    await expect(appItem).toHaveText(expect.stringContaining("2"));
    await expect(sidebar.$("button*=other")).toBeDisplayed();
    // Narrowed to the other project, All sessions lists none of app's.
    muster("session", "start", "other", "--harness", "claude", "--title", "Other agent");
    await goTo("All sessions");
    await expect(inMain("Other agent")).toBeDisplayed();
    await expect(inMain("Asking agent")).toBeDisplayed();
    await sidebar.$("button*=other").click();
    await expect(sidebar.$("button*=other")).toHaveAttribute("aria-pressed", "true");
    await expect(inMain("Asking agent")).not.toBeDisplayed();
    await expect(inMain("Other agent")).toBeDisplayed();
    await sidebar.$("button*=other").click();
    await expect(inMain("Asking agent")).toBeDisplayed();
    // ⌘\ folds the sidebar to its marks, and back.
    await pressOnWindow("⌘\\");
    await expect(sidebar).toHaveElementClass("mu-nav--folded");
    await expect(sidebar.$("div*=Projects")).not.toBeDisplayed();
    await pressOnWindow("⌘\\");
    await expect(sidebar).not.toHaveElementClass("mu-nav--folded");
    // And on the person's fold_list key, the terminal UI's ⌃\\.
    await pressOnWindow("⌃\\");
    await expect(sidebar).toHaveElementClass("mu-nav--folded");
    await pressOnWindow("⌃\\");
    await expect(sidebar).not.toHaveElementClass("mu-nav--folded");
  });

  it("draws in the theme chosen, says so to a screen reader, and tells programs its colours", async () => {
    const answers = asker();
    mkdirSync(sandbox().config, { recursive: true });
    writeFileSync(join(sandbox().config, "settings.toml"), '[agents.asker]\nprogram = "asker"\n');
    await openSettings("General");
    // r reads Muster's own settings again, after an edit by hand.
    await pressOnWindow("r");
    await until(
      () => status().running && settingsToml().includes("asker"),
      "the settings to be read",
    );
    const theme = $('[role="group"][aria-label="Theme"]');
    await theme.$("button=Dark").click();
    expect(await drawnIn()).toBe("dark");
    await expect(note()).toHaveText("This window is dark.");
    expect(await note().getAttribute("aria-live")).toBe("polite");
    // An agent asking its terminal's background is told the dark theme's.
    await until(() => {
      try {
        muster("session", "start", "app", "--harness", "asker", "--title", "Dark asker");
        return true;
      } catch {
        return false;
      }
    }, "the daemon to know the agent asker");
    const dark = await until(() => {
      const id = session("Dark asker")?.id;
      const file = id ? join(answers, id) : "";
      return id && existsSync(file) && readFileSync(file, "utf8").includes("rgb:") && file;
    }, "the dark asker's answer");
    expect(readFileSync(dark, "utf8")).toContain("rgb:1313/1313/1b1b");

    await theme.$("button=Light").click();
    expect(await drawnIn()).toBe("light");
    await expect(note()).toHaveText("This window is light.");
    // The window's own, kept by the app in the sandbox (src-tauri/src/kept.rs).
    await until(() => keptState()["muster.theme"] === "light", "the theme to be kept");
    muster("session", "start", "app", "--harness", "asker", "--title", "Light asker");
    const light = await until(() => {
      const id = session("Light asker")?.id;
      const file = id ? join(answers, id) : "";
      return id && existsSync(file) && readFileSync(file, "utf8").includes("rgb:") && file;
    }, "the light asker's answer");
    expect(readFileSync(light, "utf8")).toContain("rgb:ffff/ffff/ffff");
    await theme.$("button=Dark").click();
  });

  it("plays each sound as the person moves to it, and keeps the one chosen", async () => {
    await openSettings("Notifications");
    const button = $('button[aria-label="Sound for errors"]');
    const was = await button.getText();
    await button.click();
    await $('[role="listbox"][aria-label="Sound for errors"]').waitForDisplayed();
    await press("↓");
    const played = await until(() => heard()[0], "a sound to be heard");
    expect(played).toMatch(/^\/System\/Library\/Sounds\/\w+\.aiff$/);
    const sound = played.replace(/^.*\/(\w+)\.aiff$/, "$1");
    expect(sound).not.toBe(was);
    await press("Enter");
    await expect(button).toHaveText(sound);
    await expect(note()).toHaveText(`Errors are told, with ${sound}.`);
    await until(() => settingsToml().includes(sound), "the sound in settings.toml");
    expect(heard()).toEqual([played]);
  });

  it("streams nothing while it cannot be seen, and comes back current", async () => {
    await pressOnWindow("⌘1");
    const asking = await agentOf("Asking agent");
    await ask(asking, "Shall it go on?");
    await untilSession("Asking agent", "needs_you");
    await untilFlock(1, 1, 0);
    const id = session("Asking agent")?.id ?? "";
    await until(
      async () => (await terminalText(id)).includes("Shall it go on?"),
      "the question in its terminal",
    );

    await browser.minimizeWindow();
    await until(
      async () => (await browser.execute(() => document.visibilityState)) === "hidden",
      "the window to be hidden",
    );
    // What happens meanwhile reaches the daemon but not the window.
    show(asking, "\r\nprinted while hidden\r\n");
    const working = await agentOf("Working agent");
    await ask(working, "Hidden question?");
    await untilSession("Working agent", "needs_you");
    await browser.pause(1_000);
    expect(await flock()).toBe("1 working, 1 waiting on you, 0 done");
    expect(await terminalText(id)).not.toContain("printed while hidden");
    // Nobody looks: what is new stays new.
    expect(session("Working agent")?.unseen).toBe(true);

    // Shown again, as a notification's click shows it: all that changed at once.
    await browser.execute(
      (session) =>
        (
          window as unknown as {
            __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
          }
        ).__TAURI_INTERNALS__.invoke("command", { command: { Reveal: { session } } }),
      bareId(id),
    );
    await until(
      async () => (await browser.execute(() => document.visibilityState)) === "visible",
      "the window to be shown again",
    );
    await untilFlock(0, 2, 0);
    await until(
      async () => (await terminalText(id)).includes("printed while hidden"),
      "the terminal to catch up",
    );
  });

  it("says the daemon is out of reach, shows its log, and is back as the daemon is", async () => {
    const before = status().daemon?.pid;
    holdDaemon(true);
    muster("daemon", "stop");
    await expect(banner()).toBeDisplayed();
    await expect(banner()).toHaveText(expect.stringContaining("Muster's daemon is out of reach."));
    expect(await kep()).toBe("The daemon is out of reach");
    await expect($("main.app-screen")).toHaveAttribute("inert");
    await expect($("button.mu-command")).toBeDisabled();
    // What it last knew stays as it was.
    expect(await flock()).toBe("0 working, 2 waiting on you, 0 done");

    await banner().$("button=Show its log").click();
    await until(
      () => revealed().includes(join(sandbox().runtime, "daemon.log")),
      "the log to be opened",
    );

    // The app starts it again by itself, and is refused while it is held.
    await until(() => daemonStarts().length >= 2, "the app to start the daemon again", 30_000);
    // Why, in the link's words: the start refused, then nothing to reach.
    await expect(banner()).toHaveText(
      /could not (start|reach) Muster's daemon|could not reach the daemon/,
    );
    holdDaemon(false);
    const tries = daemonStarts().length;
    await banner().$("button=Try now").click();
    await until(() => daemonStarts().length > tries, "Try now to start the daemon", 5_000);
    await until(() => status().running, "the daemon to run again", 10_000);
    await expect(banner()).not.toBeExisting();
    expect(status().daemon?.pid).not.toBe(before);
    await expect($("main.app-screen")).not.toHaveAttribute("inert");
    await expect($("button.mu-command")).toBeEnabled();
  });

  it("waits for a daemon it cannot start, in place of a screen, then connects as it can", async () => {
    holdDaemon(true);
    muster("daemon", "stop");
    await restartApp();
    await expect($("h2=Waiting for the daemon")).toBeDisplayed();
    await expect(banner()).toBeDisplayed();
    expect(await kep()).toBe("The daemon is out of reach");
    holdDaemon(false);
    await banner().$("button=Try now").click();
    await until(() => status().running, "the daemon to run", 10_000);
    await expect(banner()).not.toBeExisting();
    await expect($("nav[aria-label='Screens']").$("button*=app")).toBeDisplayed();
  });
});
