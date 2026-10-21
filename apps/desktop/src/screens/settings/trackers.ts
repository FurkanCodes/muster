// Where a project's tickets can come from (`app::tickets_from`): its git
// remote, or a ticket plugin that is on and can run, Muster's own in the
// daemon's order, then anyone's by name, each with the fields its
// manifest declares (`[[tickets.settings]]`).

import type { PluginInfo } from "../../ipc/generated/PluginInfo";
import type { ProjectId } from "../../ipc/generated/ProjectId";
import type { TicketSetting } from "../../ipc/generated/TicketSetting";
import type { TicketsFrom } from "../../ipc/generated/TicketsFrom";
import type { Draft, Read } from "./context";

export interface Offered {
  /** Its plugin's id. */
  id: string;
  /** As its manifest's `[tickets] name` says: `GitHub`. */
  name: string;
  description: string | null;
  settings: readonly TicketSetting[];
}

export function offeredTrackers(plugins: readonly PluginInfo[]): Offered[] {
  const runs = plugins.flatMap((plugin) => {
    const tickets = plugin.details?.tickets;
    if (!plugin.enabled || plugin.problem !== null || !tickets) return [];
    const offered: Offered = {
      id: plugin.id,
      name: tickets.name,
      description: plugin.details?.description ?? null,
      settings: tickets.settings,
    };
    return [{ theirs: plugin.source !== "builtin", offered }];
  });
  const own = runs.filter((run) => !run.theirs).map((run) => run.offered);
  const theirs = runs
    .filter((run) => run.theirs)
    .map((run) => run.offered)
    .sort((a, b) => a.name.localeCompare(b.name));
  return [...own, ...theirs];
}

/** What is written for `project`'s tickets now, if its settings are read. */
export function writtenOf(read: Read | undefined): TicketsFrom | null {
  return read?.state === "loaded" ? read.settings.tickets : null;
}

/** The tracker chosen for `project`: the one being chosen, else the one written. */
export function chosenTracker(read: Read | undefined, draft: Draft | undefined): string | null {
  return draft ? draft.tracker : (writtenOf(read)?.tracker ?? null);
}

export interface Field {
  setting: TicketSetting;
  text: string;
}

/**
 * The fields of `tracker` for `project`: what the person typed for it,
 * else what is written now, else, for a site, the one another project of
 * the same tracker uses, as a company's projects usually share one
 * (`fields_for`).
 */
export function fieldsFor(
  offered: readonly Offered[],
  projects: ReadonlyMap<ProjectId, Read>,
  project: ProjectId,
  tracker: string | null,
  draft: Draft | undefined,
): Field[] {
  const found = offered.find((candidate) => candidate.id === tracker);
  if (!tracker || !found) return [];
  const written = (of: ProjectId) => {
    const from = writtenOf(projects.get(of));
    return from?.tracker === tracker ? from.fields : null;
  };
  const here = written(project) ?? {};
  const typed = draft?.typed[tracker] ?? {};
  return found.settings.map((setting) => {
    let text = typed[setting.key] ?? here[setting.key];
    if (text === undefined && setting.key === "site") {
      for (const other of projects.keys()) {
        if (other === project) continue;
        const site = written(other)?.site;
        if (site) {
          text = site;
          break;
        }
      }
    }
    return { setting, text: text ?? "" };
  });
}

/** The first field that is needed and empty, if one is. */
export function missingField(fields: readonly Field[]): Field | undefined {
  return fields.find((field) => field.setting.required && field.text.trim() === "");
}

/** The choice as `ChangeSetting` writes it whole: what was typed, trimmed, the empty left out. */
export function ticketsFrom(tracker: string | null, fields: readonly Field[]): TicketsFrom | null {
  if (tracker === null) return null;
  const out: Record<string, string> = {};
  for (const field of fields) {
    const text = field.text.trim();
    if (text) out[field.setting.key] = text;
  }
  return { tracker, fields: out };
}
