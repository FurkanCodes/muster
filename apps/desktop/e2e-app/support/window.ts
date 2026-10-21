import { $, browser } from "@wdio/globals";

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

/** Presses each of `chords`, in turn, on what has focus. */
export async function press(...chords: string[]): Promise<void> {
  for (const chord of chords) await browser.keys(keysOf(chord));
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

/** The text of `session`'s terminal, as e2e builds expose it (src/terminal/testing.ts). */
export async function terminalText(session: string): Promise<string> {
  return browser.execute(
    (id) =>
      (
        window as unknown as { __musterTerminals?: Record<string, () => string> }
      ).__musterTerminals?.[id]?.() ?? "",
    session,
  );
}

/** The sidebar's screens, by the words on them. */
export type Screen = "Needs you" | "Not seen" | "Working" | "All sessions" | "Tickets" | "Settings";

/** Goes to `screen` from the sidebar, as the person clicks it. */
export async function goTo(screen: Screen): Promise<void> {
  const item = await $(`nav[aria-label="Screens"]`).$(`button*=${screen}`);
  await item.click();
  await browser.waitUntil(async () => (await item.getAttribute("aria-current")) === "page", {
    timeoutMsg: `${screen} was not shown`,
  });
}

/** The screen the sidebar says is shown. */
export async function shownScreen(): Promise<string> {
  const current = await $('nav[aria-label="Screens"] [aria-current="page"]');
  return (await current.getText()).replace(/\d+$/, "").trim();
}

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
