import { oneLine } from "../../lib/text";
// What Settings says, in the terminal UI's words where it says the same
// thing (crates/muster-tui/src/view/settings.rs, plugins.rs,
// plugin_dialog.rs, telling.rs), as plain functions of the state.

import type { EventKind } from "../../ipc/generated/EventKind";
import type { Hook } from "../../ipc/generated/Hook";
import type { Kind } from "../../ipc/generated/Kind";
import type { PluginAction } from "../../ipc/generated/PluginAction";
import type { PluginChange } from "../../ipc/generated/PluginChange";
import type { PluginProblem } from "../../ipc/generated/PluginProblem";
import type { PluginSource } from "../../ipc/generated/PluginSource";
import type { Sound } from "../../ipc/generated/Sound";
import type { When } from "../../ipc/generated/When";

/** `text` with its first letter a capital, for a sentence. */
export function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** `text` without the full stop it may end with, to go on in a sentence of ours. */
export function sentence(text: string): string {
  return oneLine(text).replace(/\.$/, "");
}

/** `a`, `a and b`, `a, b and c`; `or` in place of `and` where asked. */
export function listed(items: readonly string[], word = "and"): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} ${word} ${items[items.length - 1]}`;
}

/** The widths a session sits beside the list from (`settings::SIDE_BY_SIDE_CHOICES`). */
export const SIDE_BY_SIDE_CHOICES: readonly number[] = [0, 80, 90, 100, 110, 120, 140, 160];

export function sideBySideLabel(columns: number): string {
  return columns === 0 ? "Any width" : `${columns} columns`;
}

/** The width `by` choices on from `now`, no further than either end (`step_side_by_side`). */
export function stepSideBySide(now: number, by: number): number {
  const found = SIDE_BY_SIDE_CHOICES.findIndex((choice) => choice >= now);
  const at = found < 0 ? SIDE_BY_SIDE_CHOICES.length - 1 : found;
  const next = Math.min(SIDE_BY_SIDE_CHOICES.length - 1, Math.max(0, at + by));
  return SIDE_BY_SIDE_CHOICES[next] as number;
}

export function sideBySideSaid(columns: number): string {
  return columns === 0
    ? "A session sits beside the list at any width."
    : `A session sits beside the list in a window of ${columns} columns or more.`;
}

/** The sounds every Mac has, which Settings offers (`telling::MAC_SOUNDS`). */
export const MAC_SOUNDS: readonly string[] = [
  "Basso",
  "Blow",
  "Bottle",
  "Frog",
  "Funk",
  "Glass",
  "Hero",
  "Morse",
  "Ping",
  "Pop",
  "Purr",
  "Sosumi",
  "Submarine",
  "Tink",
];

/** A sound as its choice is shown: `The Mac's default`, `None`, `Glass`. */
export function soundLabel(sound: Sound): string {
  if (sound === "default") return "The Mac's default";
  if (sound === "none") return "None";
  return sound;
}

/** A sound as a note says it: `the Mac's sound`, `no sound`, `Glass`. */
export function soundSaid(sound: Sound): string {
  if (sound === "default") return "the Mac's sound";
  if (sound === "none") return "no sound";
  return sound;
}

/** The sound `by` steps on from `sound`, round the Mac's default, none and its own (`Sound::step`). */
export function stepSound(sound: Sound, by: number): Sound {
  const all = ["default", "none", ...MAC_SOUNDS];
  const at = all.indexOf(sound);
  if (at < 0) return "default";
  const count = all.length;
  return all[(((at + by) % count) + count) % count] as Sound;
}

/** Whether choosing `sound` plays it: the Mac's own alert is played only by a notification. */
export function heard(sound: Sound): boolean {
  return sound !== "default" && sound !== "none";
}

/** Each kind of notification, as Settings lists them (`telling::Kind`). */
export const KINDS: readonly {
  kind: Kind;
  key: "questions" | "finishes" | "errors";
  label: string;
  does: string;
}[] = [
  {
    kind: "question",
    key: "questions",
    label: "Questions",
    does: "A question, a permission, a form",
  },
  {
    kind: "finish",
    key: "finishes",
    label: "Finishes",
    does: "Told once a turn has stayed finished a moment",
  },
  { kind: "error", key: "errors", label: "Errors", does: "A program that ended with an error" },
];

/** A length of time as people say it (`time::span`): `32 s`, `2 min`, `1 h 05 min`, `3 days`. */
export function span(millis: number): string {
  const seconds = Math.max(0, Math.floor(millis / 1000));
  if (seconds < 60) return `${seconds} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min`;
  if (seconds < 86_400) {
    const minutes = String(Math.floor((seconds % 3600) / 60)).padStart(2, "0");
    return `${Math.floor(seconds / 3600)} h ${minutes} min`;
  }
  const days = Math.floor(seconds / 86_400);
  return days === 1 ? "1 day" : `${days} days`;
}

/** A command as a shell would read it, each argument quoted where it must be (`command_line`). */
export function commandLine(args: readonly string[]): string {
  return args
    .map((raw) => {
      const arg = oneLine(raw);
      const plain = arg !== "" && /^[\p{L}\p{N}\-_./:=@%+,{}]+$/u.test(arg);
      return plain ? arg : `'${arg.replaceAll("'", "'\\''")}'`;
    })
    .join(" ");
}

/** Where a plugin is from, in a word (its row's Label). */
export function sourceWord(source: PluginSource): string {
  if (source === "builtin") return "built in";
  return "git" in source ? "git" : "folder";
}

/** Why a plugin does not run, in a few words for its row. */
export function problemWord(problem: PluginProblem): string {
  if (problem === "Changed") return "changed since you approved it";
  if (problem === "NotApproved") return "never approved";
  if (problem === "Unknown") return "not known";
  return "Missing" in problem ? "its files are gone" : "its manifest does not read";
}

/** Why a plugin does not run, in full (`PluginProblem`'s words). */
export function problemText(problem: PluginProblem): string {
  if (problem === "Changed") {
    return "its manifest changed since you approved it: update it, or link it again, to see what it runs now and approve it";
  }
  if (problem === "NotApproved") {
    return "it was never approved: install or link it to see what it runs and approve it";
  }
  if (problem === "Unknown") {
    return "it names no git repository or folder, and Muster has no plugin of its own by its id";
  }
  if ("Missing" in problem) return `${oneLine(problem.Missing.path)} is gone`;
  return `its manifest does not read: ${oneLine(problem.Unreadable.message)}`;
}

const ACTION_WORDS: Readonly<Record<PluginAction, string>> = {
  installed: "installed",
  linked: "linked",
  updated: "updated",
  turned_on: "turned on",
  turned_off: "turned off",
};

/** A plugin's last change, with when: `turned on 2 min ago from the command line`. */
export function changeWords(change: PluginChange, now: number): string {
  const by = change.by === "ui" ? "on the Settings screen" : "from the command line";
  return `${ACTION_WORDS[change.what]} ${span(now - change.at)} ago ${by}`;
}

/** What each event is, said short for a plugin's row: `needs you`, `finished`. */
const EVENT_WORDS: Readonly<Record<EventKind, string>> = {
  "session.started": "started",
  "session.needs_you": "needs you",
  "session.finished": "finished",
  "session.error": "failed",
  "session.ended": "ended",
  "worktree.created": "worktree made",
  "worktree.removed": "worktree removed",
  "pull_request.changed": "pull request changed",
  "clash.found": "clash found",
  "ticket.assigned": "ticket assigned",
};

/** The events a plugin's hooks hear, once each, in its manifest's order. */
export function hookWords(hooks: readonly Hook[]): string {
  const kinds = [...new Set(hooks.flatMap((hook) => hook.on))];
  return kinds.map((kind) => EVENT_WORDS[kind]).join(", ");
}

/** What an agent does that a hook hears, after `an agent`. */
const AGENT_DOES: Partial<Record<EventKind, string>> = {
  "session.started": "starts",
  "session.needs_you": "needs you",
  "session.finished": "finishes",
  "session.error": "fails",
  "session.ended": "ends",
};

const OTHER_EVENTS: Partial<Record<EventKind, string>> = {
  "worktree.created": "a worktree is made",
  "worktree.removed": "a worktree is removed",
  "pull_request.changed": "a pull request changes",
  "clash.found": "two workspaces' work would clash",
  "ticket.assigned": "a ticket is assigned",
};

/** When a hook runs, in a sentence: `when an agent needs you, finishes or fails`. */
export function hookWhen(on: readonly EventKind[]): string {
  const agent = on.flatMap((kind) => AGENT_DOES[kind] ?? []);
  const parts = [
    ...(agent.length > 0 ? [`an agent ${listed(agent, "or")}`] : []),
    ...on.flatMap((kind) => OTHER_EVENTS[kind] ?? []),
  ];
  return `when ${listed(parts, "or")}`;
}

/** A hook's filters as people read them (`When::words`): `project muster · harness claude`. */
export function whenWords(when: When): string {
  const words: string[] = [];
  if (when.project.length > 0) words.push(`project ${when.project.join(", ")}`);
  if (when.harness.length > 0) words.push(`harness ${when.harness.join(", ")}`);
  if (when.reason.length > 0) words.push(`reason ${when.reason.join(", ")}`);
  if (when.away === true) words.push("while you are away");
  if (when.away === false) words.push("while you look at Muster");
  return oneLine(words.join(" · "));
}
