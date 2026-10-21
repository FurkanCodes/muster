import { $, browser } from "@wdio/globals";

/** An element as `$` finds it. */
type ChainablePromiseElement = ReturnType<typeof $>;

// The window as the person drives it. The embedded WebDriver server makes
// every key in the page itself (a keydown on the element in focus, then,
// on a field, the character and an input event), so:
// - Muster's keys go with `press`, named as Muster names them (`d`, `Q`,
//   `⌘K`, `⇧Tab`, `Enter`): a capital is sent as itself, never Shift and
//   the letter, which the window would read as the small one;
// - text goes into a field with `type`, and into a terminal with
//   `typeInTerminal`: xterm takes keys and input both, and would get each
//   letter twice from keys.

const MODIFIERS: Readonly<Record<string, string>> = {
  "⌘": "Meta",
  "⌃": "Control",
  "⌥": "Alt",
  "⇧": "Shift",
};

const NAMED: Readonly<Record<string, string>> = {
  "↑": "ArrowUp",
  "↓": "ArrowDown",
  "←": "ArrowLeft",
  "→": "ArrowRight",
  "⏎": "Enter",
  Esc: "Escape",
  "⇥": "Tab",
  Space: " ",
};

/** The WebDriver keys for one of Muster's chords. */
export function keysOf(chord: string): string[] {
  const keys: string[] = [];
  let rest = chord;
  while (rest.length > 1 && MODIFIERS[rest[0] as string]) {
    keys.push(MODIFIERS[rest[0] as string] as string);
    rest = rest.slice(1);
  }
  const key = NAMED[rest] ?? rest;
  // With ⌘, ⌃ or ⌥ a letter is pressed as itself; the window names it a capital.
  keys.push(keys.length > 0 && key.length === 1 ? key.toLowerCase() : key);
  return keys;
}

/**
 * Presses each of `chords`, in turn, on what has focus. Enter or Space on
 * a focused button goes as a browser takes a key the person presses: the
 * key to the page first, then, if nothing there took it, a click on the
 * button. The driver's own keys are made in the page and have no default.
 */
export async function press(...chords: string[]): Promise<void> {
  for (const chord of chords) {
    if (["Enter", "⏎", "Space"].includes(chord)) {
      const done = await browser.execute(
        (key) => {
          const focused = document.activeElement;
          if (!(focused instanceof HTMLButtonElement) || focused.disabled) return false;
          const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
          if (focused.dispatchEvent(event)) focused.click();
          return true;
        },
        chord === "Space" ? " " : "Enter",
      );
      if (done) continue;
    }
    await browser.keys(keysOf(chord));
  }
}

/** Takes focus off any field, so a key reaches the window's keys rather than the field. */
export async function blur(): Promise<void> {
  await browser.execute(() => {
    (document.activeElement as HTMLElement | null)?.blur?.();
  });
}

/** Presses `chords` on the window itself, with no field in focus. */
export async function pressOnWindow(...chords: string[]): Promise<void> {
  await blur();
  await press(...chords);
}

/** Types `text` into the field `selector`, as a paste or an input method gives it. */
export async function type(selector: string, text: string): Promise<void> {
  const field = await $(selector);
  await field.click();
  await field.clearValue();
  await field.addValue(text);
}

/** Types a line into the terminal shown, and Enter. */
export async function typeInTerminal(line: string): Promise<void> {
  await $('section[aria-label="Terminal"]').click();
  await $('section[aria-label="Terminal"] textarea').addValue(line);
  await browser.keys("Enter");
}

/**
 * A session's id as the window knows it: `muster` prints ids with their
 * kind in front (`ses_…`), the protocol carries them bare.
 */
export function bareId(id: string): string {
  return id.replace(/^[a-z]+_/, "");
}

/** The text of `session`'s terminal, as e2e builds expose it (src/terminal/testing.ts). */
export async function terminalText(session: string): Promise<string> {
  return browser.execute(
    (id) =>
      (
        window as unknown as { __musterTerminals?: Record<string, () => string> }
      ).__musterTerminals?.[id]?.() ?? "",
    bareId(session),
  );
}

/** The sidebar's screens, by the words on them. */
export type Screen = "Sessions" | "Needs you" | "Tickets" | "Settings";

/** Goes to `screen` from the sidebar, as the person clicks it. */
export async function goTo(screen: Screen): Promise<void> {
  const item = await $(`nav[aria-label="Screens"]`).$(`button*=${screen}`);
  await item.click();
  await browser.waitUntil(async () => (await item.getAttribute("aria-current")) === "page", {
    timeoutMsg: `${screen} was not shown`,
  });
}

/** The screen the sidebar says is shown. */
export async function shownScreen(): Promise<Screen | null> {
  const current = await $('nav[aria-label="Screens"] [aria-current="page"]');
  const text = await current.getText();
  return SCREENS.find((screen) => text.includes(screen)) ?? null;
}

const SCREENS: readonly Screen[] = ["Sessions", "Needs you", "Tickets", "Settings"];

/** The flock in the title bar: working, waiting on the person, done. */
export async function flock(): Promise<string> {
  return (await $(".mu-flock").getAttribute("aria-label")) ?? "";
}

/** Waits until the flock says `working`, `waiting` and `done`. */
export async function untilFlock(working: number, waiting: number, done: number): Promise<void> {
  const wanted = `${working} working, ${waiting} waiting on you, ${done} done`;
  await browser.waitUntil(async () => (await flock()) === wanted, {
    timeoutMsg: `the flock never said "${wanted}"`,
  });
}

/** Whether the element `selector` is in the page and shown. */
export async function shown(selector: string): Promise<boolean> {
  const found = await $(selector);
  return (await found.isExisting()) && (await found.isDisplayed());
}

/** The window's only handle, chosen outright, as the harness starts the app. */
export async function theWindow(): Promise<void> {
  const [main] = await browser.getWindowHandles();
  if (main) await browser.switchToWindow(main);
}

/**
 * Puts the window where no other worktree's test window covers it all,
 * and in front: a window covered whole cannot be seen, and is sent nothing
 * (ARC-25), so another branch's tests running beside these would stall
 * them. `place` is this worktree's own (its port).
 */
export async function frontWindow(place: number): Promise<void> {
  await browser.setWindowRect(40 + (place % 7) * 60, 40 + (place % 5) * 40, 1200, 800);
  await toFront();
}

/** Brings the window to the front, as the person's click on it would. */
export async function toFront(): Promise<void> {
  await browser.execute(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
      }
    ).__TAURI_INTERNALS__
      .invoke("plugin:window|set_focus", { label: "main" })
      .catch(() => {}),
  );
}

/** The open dialog titled `title` (ui/Dialog.tsx), or the palette (`Command palette`, `Every key`). */
export function dialog(title: string) {
  return $(`//dialog[@open][@aria-label="${title}" or .//h2[normalize-space()="${title}"]]`);
}

/** Whether no dialog is open. */
export async function noDialog(): Promise<boolean> {
  return !(await $("//dialog[@open]").isExisting());
}

/** The button in `scope` whose words hold `name` (its key cap may follow them). */
export function button(scope: ChainablePromiseElement, name: string) {
  return scope.$(`.//button[contains(normalize-space(), "${name}")]`);
}

/** The field labelled `label` in `scope`: its `<label>`, or its own `aria-label`. */
export function field(scope: ChainablePromiseElement, label: string) {
  return scope.$(
    `.//*[@aria-label="${label}" or @id=(//label[starts-with(normalize-space(), "${label}")]/@for)]`,
  );
}

/**
 * Puts `text` in `element` (an input or a text area) as a paste does: its
 * value set the way the page's own code sees, then an input event. The
 * embedded driver's typing sets the value behind React's back.
 */
export async function fill(element: ChainablePromiseElement, text: string): Promise<void> {
  const found = await element;
  await found.click();
  await browser.execute(
    (node, value) => {
      const own = Object.getPrototypeOf(node) as object;
      const setter = Object.getOwnPropertyDescriptor(own, "value")?.set;
      setter?.call(node, value);
      node.dispatchEvent(new Event("input", { bubbles: true }));
    },
    found as unknown as HTMLInputElement,
    text,
  );
}

/** The radio in `scope` whose words hold `name` (an agent to choose). */
export function radio(scope: ChainablePromiseElement, name: string) {
  return scope.$(`.//*[@role="radio"][contains(normalize-space(), "${name}")]`);
}

/** The option in `scope` (a list, the palette) whose words hold `name`. */
export function option(scope: ChainablePromiseElement, name: string) {
  return scope.$(`.//*[@role="option"][contains(normalize-space(), "${name}")]`);
}

/** The alert in `scope`: what went wrong, as a dialog says it. */
export function alertIn(scope: ChainablePromiseElement) {
  return scope.$('.//*[@role="alert"]');
}

/**
 * The row on the screen in view named `name`: a project or a branch in All
 * sessions, or a session there or in a queue, by its title.
 */
export function row(name: string) {
  return $(
    `//main//*[(@role="treeitem" or @role="option") and (@aria-label="${name}" or starts-with(@aria-label, "${name},"))]`,
  );
}

/**
 * Waits until the row named `name` says `doing` after its name (`ended`,
 * `working`…): the window shows what the daemon said. A key pressed once
 * only `muster status` says it acts on what the window last knew, as the
 * person's would: the daemon answers before it tells its windows.
 */
export async function untilRowSays(name: string, doing: RegExp): Promise<void> {
  const found = row(name);
  await browser.waitUntil(
    async () => {
      const label = (await found.isExisting()) ? await found.getAttribute("aria-label") : "";
      return doing.test((label ?? "").slice(name.length + 2));
    },
    { timeoutMsg: `the window never showed ${name} as ${doing}` },
  );
}

/** Selects the row named `name` on Sessions, as a click on it does. */
export async function selectRow(name: string): Promise<void> {
  await goTo("Sessions");
  const found = row(name);
  await found.click();
  await browser.waitUntil(async () => (await found.getAttribute("aria-selected")) === "true", {
    timeoutMsg: `${name} was not selected`,
  });
}

/**
 * Sends the daemon `command` through the app, as a screen does: for a
 * test's setting up of what no key reaches. Its reply.
 */
export async function sendCommand(command: unknown): Promise<unknown> {
  return browser.execute(
    (sent) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__.invoke("command", { command: sent }),
    command as object,
  );
}
