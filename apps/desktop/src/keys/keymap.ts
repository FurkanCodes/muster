// The person's keys (PRODUCT.md §6, ARCHITECTURE.md §5a "The keys are the
// person's"): Muster's own table (`muster_core::keys`, exported as
// keys.json) with the keys the person set over it (`preferences.keys`) and
// the panels of the plugins that run, each with its key. This is the
// terminal UI's `Keymap` read the same way, so a key does here what it does
// there, and a key the person sets is refused here as there: one another
// action has on a screen they share, one every screen keeps, or, inside a
// terminal, one its program could take as typed.

import table from "../ipc/generated/keys.json";
import type { PluginInfo } from "../ipc/generated/PluginInfo";
import { type Chord, chordLabel, domChord, inTab, parseChord, sameChord, untyped } from "./chord";

/** Where a key is pressed, as the table names it. */
export type KeyScreen =
  | "sessions"
  | "needs_you"
  | "tickets"
  | "settings"
  | "changes"
  | "in_session"
  | "in_tab"
  | "prefix";

export interface KeyAction {
  /** Its name in `[keys]`. */
  name: string;
  screens: readonly KeyScreen[];
  /** Its keys unless the person sets others, as `[keys]` writes them. */
  keys: readonly string[];
  /** What it does, in the table's words. */
  what: string;
  /** Where Settings › Keys lists it. */
  group: string;
}

/** Every action, in the order Settings › Keys lists them. */
export const ACTIONS = table.actions as readonly KeyAction[];

const BY_NAME = new Map(ACTIONS.map((action) => [action.name, action]));

/** What each screen is called where keys are listed. */
export const SCREEN_TITLES = Object.fromEntries(
  table.screens.map((screen) => [screen.id, screen.title]),
) as Readonly<Record<KeyScreen, string>>;

/** The keys every screen keeps, as `[keys]` writes them, with what each is. */
const KEPT = new Map(table.kept.map((kept) => [kept.key, kept.used]));

/** The lists of sessions, where plugins' panels open. */
const SESSION_LISTS: readonly KeyScreen[] = ["sessions", "needs_you"];

const ID = /^[a-z0-9][a-z0-9-]{0,31}$/;

export function actionNamed(name: string): KeyAction | undefined {
  return BY_NAME.get(name);
}

/** What `chord` is if every screen keeps it (`a key that moves about`), else nothing. */
export function keptAs(chord: Chord): string | undefined {
  const written = `${chord.ctrl ? "ctrl-" : ""}${chord.alt ? "alt-" : ""}`;
  const key = chord.key;
  const name = "named" in key ? key.named : "f" in key ? `f${key.f}` : key.char;
  return KEPT.get(written + name);
}

/** A plugin's panel, as its key opens it (`keys::PanelAction`). */
export interface PanelAction {
  plugin: string;
  panel: string;
  /** Its plugin's name, under which Settings › Keys lists it. */
  pluginName: string;
  title: string;
  /** The key its manifest suggests, as written. */
  suggested: string | null;
}

/** The name in `[keys]` of `plugin`'s panel `panel`: `plugin.zoetrope.graph`. */
export function panelName(plugin: string, panel: string): string {
  return `plugin.${plugin}.${panel}`;
}

/** Whether `name` names a plugin's panel's action, known or not. */
export function isPanelName(name: string): boolean {
  if (!name.startsWith("plugin.")) return false;
  const rest = name.slice("plugin.".length);
  const dot = rest.indexOf(".");
  return dot > 0 && ID.test(rest.slice(0, dot)) && ID.test(rest.slice(dot + 1));
}

/** What the key of the panel titled `title` does. */
export function panelWhat(title: string): string {
  return `${title} beside the session (again: put it away)`;
}

/** The panels of the plugins that run, in the daemon's order (`App::panel_actions`). */
export function panelsOf(plugins: readonly PluginInfo[]): PanelAction[] {
  return plugins
    .filter((plugin) => plugin.enabled && plugin.problem === null && plugin.details !== null)
    .flatMap((plugin) =>
      (plugin.details?.panels ?? []).map((panel) => ({
        plugin: plugin.id,
        panel: panel.id,
        pluginName: plugin.details?.name ?? plugin.id,
        title: panel.title,
        suggested: panel.key,
      })),
    );
}

/** Why a key cannot be an action's, in the core's terms (`KeysError`). */
export type Refusal =
  | { kind: "kept"; key: string; used: string }
  | { kind: "typed"; key: string }
  | { kind: "in_tab"; key: string }
  | { kind: "taken"; key: string; other: string };

/** What reads in the person's keys but does nothing (`KeysWarning`). */
export type KeysWarning =
  | { kind: "not_an_action"; name: string }
  | { kind: "panel_key"; name: string; key: string; why: string }
  | { kind: "suggested"; plugin: string; panel: string; key: string; why: string };

interface BoundPanel {
  readonly action: PanelAction;
  readonly name: string;
  readonly chords: readonly Chord[];
  /** Whether the person set them. */
  readonly set: boolean;
}

export interface Keymap {
  /** Each of Muster's actions' keys, the first the one hints show. */
  readonly keys: ReadonlyMap<string, readonly Chord[]>;
  /** The names set that are no action of Muster's, with their keys as written. */
  readonly aside: ReadonlyMap<string, readonly string[]>;
  readonly panels: readonly BoundPanel[];
  readonly dropped: readonly KeysWarning[];
  /** Why the person's keys do not read, if they do not: Muster's hold meanwhile. */
  readonly problem: string | null;
}

function parsed(keys: readonly string[]): Chord[] {
  return keys.flatMap((key) => {
    const chord = parseChord(key);
    return chord ? [chord] : [];
  });
}

const DEFAULTS: ReadonlyMap<string, readonly Chord[]> = new Map(
  ACTIONS.map((action) => [action.name, parsed(action.keys)]),
);

function holds(chords: readonly Chord[] | undefined, chord: Chord): boolean {
  return chords?.some((held) => sameChord(held, chord)) ?? false;
}

function shares(a: KeyAction, b: KeyAction): boolean {
  return a.screens.some((screen) => b.screens.includes(screen));
}

/** Another of Muster's actions that has `chord` on a screen `action` works on. */
function takenBy(
  keys: ReadonlyMap<string, readonly Chord[]>,
  action: KeyAction,
  chord: Chord,
): KeyAction | undefined {
  return ACTIONS.find(
    (other) =>
      other.name !== action.name && shares(other, action) && holds(keys.get(other.name), chord),
  );
}

function panelWith(panels: readonly BoundPanel[], chord: Chord): BoundPanel | undefined {
  return panels.find((bound) => holds(bound.chords, chord));
}

/** The action that has `chord` on one of the lists of sessions. */
function onSessionLists(
  keys: ReadonlyMap<string, readonly Chord[]>,
  chord: Chord,
): KeyAction | undefined {
  return ACTIONS.find(
    (action) =>
      action.screens.some((screen) => SESSION_LISTS.includes(screen)) &&
      holds(keys.get(action.name), chord),
  );
}

function check(
  keys: ReadonlyMap<string, readonly Chord[]>,
  panels: readonly BoundPanel[],
  action: KeyAction,
  chord: Chord,
): Refusal | null {
  const key = chordLabel(chord);
  const used = keptAs(chord);
  if (used) return { kind: "kept", key, used };
  if (action.screens.includes("in_session") && !untyped(chord)) return { kind: "typed", key };
  if (action.screens.includes("in_tab") && !inTab(chord)) return { kind: "in_tab", key };
  const other = takenBy(keys, action, chord);
  if (other) return { kind: "taken", key, other: other.name };
  const panel = action.screens.some((screen) => SESSION_LISTS.includes(screen))
    ? panelWith(panels, chord)
    : undefined;
  return panel ? { kind: "taken", key, other: panel.name } : null;
}

/** Muster's own keys, nobody's set. */
export function defaultKeymap(): Keymap {
  return { keys: DEFAULTS, aside: new Map(), panels: [], dropped: [], problem: null };
}

/** The person's keys over Muster's (`Keymap::with`); Muster's own where they do not read. */
function withKeys(set: Readonly<Record<string, readonly string[]>>): Keymap {
  const keys = new Map(DEFAULTS);
  const aside = new Map<string, readonly string[]>();
  for (const name of Object.keys(set).sort()) {
    const written = set[name] ?? [];
    if (!BY_NAME.has(name)) {
      aside.set(name, written);
      continue;
    }
    const chords: Chord[] = [];
    for (const key of written) {
      const chord = parseChord(key);
      if (!chord) return { ...defaultKeymap(), problem: `${JSON.stringify(key)} is not a key` };
      chords.push(chord);
    }
    keys.set(name, chords);
  }
  for (const action of ACTIONS) {
    for (const chord of keys.get(action.name) ?? []) {
      const refused = check(keys, [], action, chord);
      if (refused) {
        return { ...defaultKeymap(), problem: `keys.${action.name}: ${refusalWords(refused)}` };
      }
    }
  }
  return { keys, aside, panels: [], dropped: [], problem: null };
}

/** Why `chord` cannot be the panel `name`'s, if it cannot (`free_for_panel`). */
function notFreeForPanel(
  keys: ReadonlyMap<string, readonly Chord[]>,
  panels: readonly BoundPanel[],
  name: string,
  chord: Chord,
): string | null {
  const used = keptAs(chord);
  if (used) return `is ${used}`;
  const action = onSessionLists(keys, chord);
  if (action) return `is Muster's for ${action.name}`;
  const other = panelWith(panels, chord);
  return other && other.name !== name ? `is ${other.name}'s` : null;
}

/**
 * The person's keys over Muster's, with the panels of the plugins that run
 * (`Keymap::with(..).unwrap_or_default().with_panels(..)`): the keys the
 * person set for panels bound first, then the ones the others suggest, so
 * a plugin added later never takes a key the person gave another's panel;
 * a key that cannot be a panel's is dropped and warned of.
 */
export function keymapOf(
  set: Readonly<Record<string, readonly string[]>>,
  panels: readonly PanelAction[] = [],
): Keymap {
  const map = withKeys(set);
  const bound = panels.map((action) => {
    const name = panelName(action.plugin, action.panel);
    return { action, name, chords: [] as Chord[], set: map.aside.has(name) };
  });
  const dropped: KeysWarning[] = [];
  for (const personal of [true, false]) {
    for (const panel of bound) {
      if (panel.set !== personal) continue;
      const written = personal
        ? (map.aside.get(panel.name) ?? [])
        : panel.action.suggested
          ? [panel.action.suggested]
          : [];
      for (const key of written) {
        const chord = parseChord(key);
        const why = chord
          ? notFreeForPanel(map.keys, bound, panel.name, chord)
          : `${JSON.stringify(key)} is not a key`;
        if (chord && why === null) panel.chords.push(chord);
        else if (personal)
          dropped.push({ kind: "panel_key", name: panel.name, key, why: why ?? "" });
        else
          dropped.push({
            kind: "suggested",
            plugin: panel.action.pluginName,
            panel: panel.action.title,
            key,
            why: why ?? "",
          });
      }
    }
  }
  return { ...map, panels: bound, dropped };
}

function panelNamed(map: Keymap, name: string): BoundPanel | undefined {
  return map.panels.find((bound) => bound.name === name);
}

/**
 * Whether `chord` may be the keys of `name`, one of Muster's actions or a
 * panel's, beside the others' (`Keymap::check`, `Keymap::check_panel`);
 * `null` when it may.
 */
export function checkKey(map: Keymap, name: string, chord: Chord): Refusal | null {
  const action = BY_NAME.get(name);
  if (action) return check(map.keys, map.panels, action, chord);
  const key = chordLabel(chord);
  const used = keptAs(chord);
  if (used) return { kind: "kept", key, used };
  const other = onSessionLists(map.keys, chord)?.name ?? panelWith(map.panels, chord)?.name;
  return other && other !== name ? { kind: "taken", key, other } : null;
}

/** `name`'s keys, Muster's action's or a panel's; the first is the one hints show. */
export function chordsOf(map: Keymap, name: string): readonly Chord[] {
  return map.keys.get(name) ?? panelNamed(map, name)?.chords ?? [];
}

/** `name`'s first key as the person reads it; `none` where it has none. */
export function labelOf(map: Keymap, name: string): string {
  const first = chordsOf(map, name)[0];
  return first ? chordLabel(first) : "none";
}

/** Each of `name`'s keys as the person reads them. */
export function labelsOf(map: Keymap, name: string): string[] {
  return chordsOf(map, name).map(chordLabel);
}

/** Whether `name`'s keys are the person's, not Muster's (or its plugin's suggestion). */
export function isSet(map: Keymap, name: string): boolean {
  const defaults = DEFAULTS.get(name);
  if (!defaults) return panelNamed(map, name)?.set ?? false;
  const keys = chordsOf(map, name);
  return (
    keys.length !== defaults.length ||
    keys.some((chord, at) => !sameChord(chord, defaults[at] as Chord))
  );
}

/** What `name`'s keys do, in the table's words. */
export function whatOf(map: Keymap, name: string): string {
  const action = BY_NAME.get(name);
  if (action) return action.what;
  const panel = panelNamed(map, name);
  return panel ? panelWhat(panel.action.title) : name;
}

/** What `chord` does on `screen`: one of Muster's actions, else a panel's on the lists of sessions. */
export function actionAt(map: Keymap, screen: KeyScreen, chord: Chord): string | null {
  const action = ACTIONS.find(
    (candidate) => candidate.screens.includes(screen) && holds(map.keys.get(candidate.name), chord),
  );
  if (action) return action.name;
  return SESSION_LISTS.includes(screen) ? (panelWith(map.panels, chord)?.name ?? null) : null;
}

const answered = new WeakMap<Keymap, Map<KeyScreen, ReadonlyMap<string, string>>>();

/** Every key `screen` answers, as the window's listener names it, with what it does. */
export function screenKeys(map: Keymap, screen: KeyScreen): ReadonlyMap<string, string> {
  let screens = answered.get(map);
  if (!screens) {
    screens = new Map();
    answered.set(map, screens);
  }
  const held = screens.get(screen);
  if (held) return held;
  const out = new Map<string, string>();
  screens.set(screen, out);
  for (const action of ACTIONS) {
    if (!action.screens.includes(screen)) continue;
    for (const chord of map.keys.get(action.name) ?? []) {
      const dom = domChord(chord);
      if (!out.has(dom)) out.set(dom, action.name);
    }
  }
  if (SESSION_LISTS.includes(screen)) {
    for (const panel of map.panels) {
      for (const chord of panel.chords) {
        const dom = domChord(chord);
        if (!out.has(dom)) out.set(dom, panel.name);
      }
    }
  }
  return out;
}

/** The keys the person set that do nothing (`Keymap::warnings`). */
export function warningsOf(map: Keymap): KeysWarning[] {
  const aside: KeysWarning[] = [...map.aside.keys()]
    .filter((name) => !panelNamed(map, name))
    .map((name) => ({ kind: "not_an_action", name }));
  return [...aside, ...map.dropped];
}

/** A warning, in the core's words. */
export function warningText(warning: KeysWarning): string {
  switch (warning.kind) {
    case "not_an_action":
      return `keys.${warning.name} is not something a key does in Muster: kept, for a plugin's action of that name, and doing nothing meanwhile`;
    case "panel_key":
      return `keys.${warning.name}: ${warning.key} ${warning.why}, so it does nothing for the panel: set it another`;
    case "suggested":
      return `the ${warning.plugin} plugin's ${warning.panel} panel suggests ${warning.key}, which ${warning.why}: set it a key of its own here`;
  }
}

/** A refusal as the daemon's settings file would say it, after `keys.<action>:`. */
function refusalWords(refusal: Refusal): string {
  switch (refusal.kind) {
    case "kept":
      return `${refusal.key} is ${refusal.used}, which is the same on every screen`;
    case "typed":
      return `${refusal.key} would never reach a session's program; a key there needs ⌃ or ⌥, or is a capital letter`;
    case "in_tab":
      return `${refusal.key} cannot be told from typing in a terminal tab; a key there is a letter with ⌃ or ⌥`;
    case "taken":
      return `${refusal.key} is ${refusal.other}'s on the same screen`;
  }
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A refusal as Settings › Keys says it under the row, naming both actions by what they do. */
export function refusalText(map: Keymap, refusal: Refusal): string {
  switch (refusal.kind) {
    case "kept":
      return `${refusal.key} is ${refusal.used}, the same on every screen. Press another key.`;
    case "typed":
      return `${refusal.key} would reach the program as typing. A key inside a terminal needs ⌃ or ⌥, or is a capital letter. Press another key.`;
    case "in_tab":
      return `${refusal.key} cannot be told from typing in a terminal tab. A key there is a letter with ⌃ or ⌥. Press another key.`;
    case "taken":
      return `${refusal.key} already does “${capital(whatOf(map, refusal.other))}” on this screen. Press another key.`;
  }
}
