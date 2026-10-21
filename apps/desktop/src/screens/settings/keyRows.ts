// Settings › Keys's rows (`app::rebind`): every action under where it
// works, in the table's order, then each plugin's panels under its name,
// narrowed to those whose words or keys hold what is typed, ignoring case.

import { ACTIONS, isSet, type Keymap, labelsOf, whatOf } from "../../keys/keymap";
import { capital } from "./words";

export interface KeyEntry {
  /** Its name in `[keys]`. */
  name: string;
  /** The heading it is listed under. */
  group: string;
  what: string;
  labels: readonly string[];
  /** Whether its keys are the person's. */
  set: boolean;
}

export function keyEntries(map: Keymap): KeyEntry[] {
  const own = ACTIONS.map((action) => ({
    name: action.name,
    group: action.group,
    what: capital(action.what),
    labels: labelsOf(map, action.name),
    set: isSet(map, action.name),
  }));
  const panels = map.panels.map((panel) => ({
    name: panel.name,
    group: `${panel.action.pluginName}'s panels`,
    what: capital(whatOf(map, panel.name)),
    labels: labelsOf(map, panel.name),
    set: isSet(map, panel.name),
  }));
  return [...own, ...panels];
}

/** The entries whose words, name or keys hold `find`, ignoring case; all where it is empty. */
export function narrowed(entries: readonly KeyEntry[], find: string): KeyEntry[] {
  const wanted = find.trim().toLowerCase();
  if (!wanted) return [...entries];
  return entries.filter(
    (entry) =>
      entry.what.toLowerCase().includes(wanted) ||
      entry.name.toLowerCase().includes(wanted) ||
      entry.group.toLowerCase().includes(wanted) ||
      entry.labels.some((label) => label.toLowerCase() === wanted),
  );
}
