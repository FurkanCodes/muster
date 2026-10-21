// What Settings › Plugins says of a plugin, as data its row, its details
// and the approval dialog draw (view/plugins.rs, view/plugin_dialog.rs):
// where it is from, what it brings, and everything it runs, word for word
// (`PRD-15`). What comes from a plugin is shown without control characters
// (`PRD-07`).

import type { PluginDetails } from "../../ipc/generated/PluginDetails";
import type { PluginInfo } from "../../ipc/generated/PluginInfo";
import type { PluginSource } from "../../ipc/generated/PluginSource";
import { chordLabel, parseChord } from "../../keys/chord";
import { type Keymap, labelOf, panelName } from "../../keys/keymap";
import { commandLine, hookWhen, hookWords, oneLine, whenWords } from "./words";

/** Whether `address` names a folder rather than a git repository (`plugins::is_folder`). */
export function isFolder(address: string): boolean {
  return /^[/~.]/.test(address);
}

/** Muster's own first, as the daemon lists them, then the person's by name. */
export function pluginOrder(plugins: readonly PluginInfo[]): PluginInfo[] {
  const name = (plugin: PluginInfo) => plugin.details?.name ?? plugin.id;
  return [
    ...plugins.filter((plugin) => plugin.source === "builtin"),
    ...plugins
      .filter((plugin) => plugin.source !== "builtin")
      .sort((a, b) => name(a).localeCompare(name(b))),
  ];
}

export function runs(plugin: PluginInfo): boolean {
  return plugin.enabled && plugin.problem === null;
}

/** Whether approving it again makes it run: changed since, or never approved. */
export function wantsApproval(plugin: PluginInfo): boolean {
  return plugin.problem === "Changed" || plugin.problem === "NotApproved";
}

/** Its name as its manifest says, or its id where that does not read. */
export function pluginName(plugin: PluginInfo): string {
  return oneLine(plugin.details?.name ?? plugin.id);
}

/** Where it is from, in words: `built into Muster`, `the folder ~/x, read where it is`. */
export function sourceText(source: PluginSource): string {
  if (source === "builtin") return "built into Muster";
  if ("folder" in source) return `the folder ${oneLine(source.folder.path)}, read where it is`;
  const at = source.git.git_ref ? `, ${oneLine(source.git.git_ref)}` : "";
  return `${oneLine(source.git.url)}${at}, commit ${oneLine(source.git.commit.slice(0, 7))}`;
}

/** A key as a manifest writes it, on a key cap; as written where it is no key. */
function keyLabel(written: string): string {
  const chord = parseChord(written);
  return chord ? chordLabel(chord) : oneLine(written);
}

/** The key a panel opens on: as bound while its plugin runs, else as it suggests. */
function panelKey(plugin: PluginInfo, panel: { id: string; key: string | null }, map: Keymap) {
  const name = panelName(plugin.id, panel.id);
  const bound = map.panels.find((bound) => bound.name === name);
  if (bound) return labelOf(map, name);
  return panel.key ? keyLabel(panel.key) : null;
}

/** What it brings, on its row: `panel: graph (⇧G) · tickets: YouTrack · hooks: needs you`. */
export function brings(plugin: PluginInfo, map: Keymap): string {
  const details = plugin.details;
  if (!details) return "";
  const out: string[] = [];
  if (details.panels.length > 0) {
    const panels = details.panels.map((panel) => {
      const key = panelKey(plugin, panel, map);
      return key ? `${oneLine(panel.title)} (${key})` : oneLine(panel.title);
    });
    out.push(`${details.panels.length === 1 ? "panel" : "panels"}: ${panels.join(", ")}`);
  }
  if (details.tickets) out.push(`tickets: ${oneLine(details.tickets.name)}`);
  if (details.hooks.length > 0) {
    out.push(`${details.hooks.length === 1 ? "hook" : "hooks"}: ${hookWords(details.hooks)}`);
  }
  return out.join(" · ");
}

/** One thing it runs: what it is, the command word for word, and when or where. */
export interface Runs {
  what: string;
  command: string;
  /** `beside a session, key ⇧G`; `when an agent needs you; at most 15 seconds each time`. */
  how: string;
}

export function runsOf(details: PluginDetails): Runs[] {
  const panels = details.panels.map((panel) => ({
    what: `Panel ${oneLine(panel.title)}`,
    command: commandLine(panel.command),
    how: `beside ${panel.on === "session" ? "a session" : "a workspace"}${panel.key ? `, key ${keyLabel(panel.key)}` : ""}`,
  }));
  const tickets = details.tickets
    ? [
        {
          what: "Tickets",
          command: commandLine(details.tickets.command),
          how: `as a ticket source, ${oneLine(details.tickets.name)}${
            details.tickets.settings.length > 0
              ? `, asking for ${details.tickets.settings.map((setting) => `${oneLine(setting.label).toLowerCase()}${setting.required ? "" : " if you like"}`).join("; ")}`
              : ""
          }`,
        },
      ]
    : [];
  const hooks = details.hooks.map((hook) => {
    const only = whenWords(hook.when);
    return {
      what: `Hook ${oneLine(hook.id)}`,
      command: commandLine(hook.command),
      how: `${hookWhen(hook.on)}${only ? `, only ${only}` : ""}; at most ${hook.timeout} seconds each time`,
    };
  });
  return [...panels, ...tickets, ...hooks];
}

/** The programs it needs that are not found. */
export function missingPrograms(details: PluginDetails): string[] {
  return details.requires.filter((required) => !required.found).map((required) => required.program);
}
