// The settings as one list of rows, as the terminal UI keeps them
// (`App::setting_rows`): Muster's own sections, then each project's. Each
// page shows its section's rows; ↑↓ go through them all in order, from one
// page to the next, and the selection stays on a row that exists.

import type { ProjectId } from "../../ipc/generated/ProjectId";

/** A page of Settings. */
export type Section =
  | "general"
  | "notifications"
  | "agents"
  | "naming"
  | "plugins"
  | "keys"
  | `project:${ProjectId}`;

export const MUSTER_SECTIONS: readonly { id: Section; title: string }[] = [
  { id: "general", title: "General" },
  { id: "notifications", title: "Notifications" },
  { id: "agents", title: "Agents" },
  { id: "naming", title: "Naming chats" },
  { id: "plugins", title: "Plugins" },
  { id: "keys", title: "Keys" },
];

export function projectSection(project: ProjectId): Section {
  return `project:${project}`;
}

/** The project a section is the page of, if it is one's. */
export function projectOf(section: Section): ProjectId | null {
  return section.startsWith("project:") ? section.slice("project:".length) : null;
}

/** What a row's control is, for its keys and hints. */
export type RowKind =
  | "switch"
  | "select"
  | "segmented"
  | "button"
  | "field"
  | "plugin"
  | "key"
  | "item";

export interface Row {
  /** `general/theme`, `project:<id>/only_mine`, `keys/key.rename`. */
  key: string;
  section: Section;
  kind: RowKind;
}

export function rowKey(section: Section, row: string): string {
  return `${section}/${row}`;
}

/** A row key's page; a page's own name, as the command palette asks for one, is that page. */
export function sectionOf(key: string): Section {
  const at = key.indexOf("/");
  return (at < 0 ? key : key.slice(0, at)) as Section;
}

/** A row's own part of its key: `theme`, `plugin.github`. */
export function rowName(key: string): string {
  return key.slice(key.indexOf("/") + 1);
}

/** What a project's page shows rows for, as its settings stand. */
export interface ProjectRows {
  id: ProjectId;
  /** Whether its settings have been read. */
  loaded: boolean;
  /** The fields of the tracker chosen, by key. */
  fields: readonly string[];
  /** Whether the daemon says how branches are named. */
  prefix: boolean;
}

/**
 * The first protocol whose daemon has a project's worktrees build into one
 * folder and takes `SharedBuilds` (`SHARED_BUILDS_PROTOCOL`).
 */
export const SHARED_BUILDS_PROTOCOL = 47;

/**
 * The first protocol whose daemon lets agents start agents within the
 * person's limits and takes `AgentsStartAgents` (`DELEGATION_PROTOCOL`).
 */
export const DELEGATION_PROTOCOL = 51;

export interface RowsInput {
  /** The agents, Muster's own then the person's, by id. */
  agents: readonly string[];
  /** The plugins, by id, as the daemon lists them. */
  plugins: readonly string[];
  projects: readonly ProjectRows[];
  /** The keys shown, by action or panel name, as Settings › Keys narrows them. */
  keys: readonly string[];
  /** Whether the daemon has a project's worktrees build into one folder. */
  sharedBuilds: boolean;
  /** Whether the daemon lets agents start agents within limits. */
  delegation: boolean;
}

function rows(section: Section, kinds: readonly [string, RowKind][]): Row[] {
  return kinds.map(([row, kind]) => ({ key: rowKey(section, row), section, kind }));
}

export function settingRows(input: RowsInput): Row[] {
  const out: Row[] = [
    ...rows("general", [
      ["theme", "segmented"],
      ["session_lines", "segmented"],
      ["panels", "button"],
      ["colours", "select"],
      ["side_by_side", "select"],
      ...(input.sharedBuilds ? [["shared_builds", "switch"] as [string, RowKind]] : []),
      ["restart", "button"],
    ]),
    ...rows("notifications", [
      ["notifications", "switch"],
      ["tell.question", "switch"],
      ["tell.finish", "switch"],
      ["tell.error", "switch"],
      ["system_settings", "button"],
      ["notch", "switch"],
    ]),
    ...rows("agents", [
      ...input.agents.map((agent): [string, RowKind] => [`agent.${agent}`, "item"]),
      ["edit", "button"],
      ["read_again", "button"],
      ...(input.delegation ? [["delegation", "switch"] as [string, RowKind]] : []),
    ]),
    ...rows("naming", [
      ["names_by_agent", "switch"],
      ["named_by", "select"],
      ["naming_model", "select"],
    ]),
    ...rows("plugins", [
      ...input.plugins.map((plugin): [string, RowKind] => [`plugin.${plugin}`, "plugin"]),
      ["add_plugin", "button"],
    ]),
    ...rows("keys", [
      ["find", "field"],
      ...input.keys.map((name): [string, RowKind] => [`key.${name}`, "key"]),
    ]),
  ];
  for (const project of input.projects) {
    const section = projectSection(project.id);
    const own: [string, RowKind][] = project.loaded
      ? [
          ["tickets_from", "select"],
          ...project.fields.map((field): [string, RowKind] => [`field.${field}`, "field"]),
          ["only_mine", "switch"],
          ...(project.prefix ? [["muster_prefix", "switch"] as [string, RowKind]] : []),
        ]
      : [];
    out.push(...rows(section, [...own, ["remove", "button"]]));
  }
  return out;
}

/**
 * The row the selection is on: `wanted` while it exists; for a plugin gone,
 * the row that adds one; else its page's first row, else the first of all
 * (`keep_setting_selection`).
 */
export function keptRow(all: readonly Row[], wanted: string | undefined): Row | undefined {
  if (wanted) {
    const found = all.find((row) => row.key === wanted);
    if (found) return found;
    const section = sectionOf(wanted);
    if (rowName(wanted).startsWith("plugin.")) {
      const add = all.find((row) => row.key === rowKey("plugins", "add_plugin"));
      if (add) return add;
    }
    const first = all.find((row) => row.section === section);
    if (first) return first;
  }
  return all[0];
}

/** The row `by` rows on from `key`, through every page, no further than either end. */
export function movedRow(
  all: readonly Row[],
  key: string | undefined,
  by: number,
): Row | undefined {
  const at = all.findIndex((row) => row.key === key);
  if (at < 0) return all[0];
  return all[Math.min(all.length - 1, Math.max(0, at + by))];
}

/** A page's first row, where a click on its name goes. */
export function firstRow(all: readonly Row[], section: Section): Row | undefined {
  return all.find((row) => row.section === section);
}
