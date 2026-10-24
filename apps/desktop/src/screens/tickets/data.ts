import { createStore } from "zustand/vanilla";
import { ask } from "../../ipc/ask";
import type { Bridge } from "../../ipc/bridge";
import type { Issue } from "../../ipc/generated/Issue";
import type { PluginInfo } from "../../ipc/generated/PluginInfo";
import type { Reply } from "../../ipc/generated/Reply";
import type { TicketBrief } from "../../ipc/generated/TicketBrief";
import { errorText } from "../../lib/text";

export type Issues = Extract<Reply, { Issues: unknown }>["Issues"];
export type Viewed = Extract<Reply, { IssueViewed: unknown }>["IssueViewed"];
export type Load<T> =
  | { state: "loading" }
  | { state: "failed"; error: string }
  /** `refreshing` while it is fetched again: shown as it was until the new one comes. */
  | { state: "loaded"; value: T; refreshing?: boolean };
export interface TicketData {
  lists: Record<string, Load<Issues>>;
  texts: Record<string, Load<Viewed>>;
  /** What of each ticket an agent is given, as the person chose; its text alone until then. */
  briefs: Record<string, TicketBrief>;
  /** The part of each ticket the keys are on: 0 its text, 1 its first comment… */
  parts: Record<string, number>;
  changing: Record<string, boolean>;
  lastAgent: string;
  lastProjects: Record<string, string>;
}
export function ticketId(project: string, key: string): string {
  return `${project}:${key}`;
}
/** Its text alone: what an agent is given until the person chooses otherwise. */
export const TEXT_ALONE: TicketBrief = { text: true, comments: [] };

/** Whether `brief` gives part `at` (0 its text, 1 its first comment…). */
export function gives(brief: TicketBrief, at: number): boolean {
  return at === 0 ? brief.text : brief.comments.includes(at - 1);
}

/** `brief` giving part `at` or not, as the terminal UI's `TicketBrief::toggle`; null where nothing would be left. */
export function toggled(brief: TicketBrief, at: number): TicketBrief | null {
  const next =
    at === 0
      ? { ...brief, text: !brief.text }
      : {
          ...brief,
          comments: gives(brief, at)
            ? brief.comments.filter((c) => c !== at - 1)
            : [...brief.comments, at - 1].sort((a, b) => a - b),
        };
  return next.text || next.comments.length ? next : null;
}

/** What `brief` gives, in words: `the ticket's text and 2 comments on it`. */
export function givenWords(brief: TicketBrief): string {
  const comments = brief.comments.length === 1 ? "1 comment" : `${brief.comments.length} comments`;
  if (!brief.comments.length) return "the ticket's text";
  return brief.text ? `the ticket's text and ${comments} on it` : `${comments} on the ticket`;
}

/**
 * What of the daemon's `plugins` decides where tickets come from: each ticket
 * plugin, whether it is on, whether it can run, and the manifest approved, as
 * the terminal UI's `ticket_sources` (`PRD-15`).
 */
export function ticketSources(plugins: readonly PluginInfo[]): string {
  return plugins
    .filter((plugin) => plugin.details?.tickets)
    .map((plugin) =>
      [plugin.id, plugin.enabled, plugin.problem === null, plugin.details?.sha256].join(" "),
    )
    .join("\n");
}

function createTickets(bridge: Bridge) {
  const store = createStore<TicketData>(() => ({
    lists: {},
    texts: {},
    briefs: {},
    parts: {},
    changing: {},
    lastAgent: "claude",
    lastProjects: {},
  }));
  const setList = (id: string, value: Load<Issues>) =>
    store.setState((s) => ({ lists: { ...s.lists, [id]: value } }));
  const setText = (id: string, value: Load<Viewed>) =>
    store.setState((s) => ({ texts: { ...s.texts, [id]: value } }));
  // Fetched again (r, Mine and All), a list already shown stays shown,
  // marked refreshing, until the new one comes: it never gives way to
  // "Fetching…", so nothing on it moves and its selected ticket stays.
  // Only the newest fetch of a project lands.
  const asked: Record<string, number> = {};
  // The ticket plugins the lists were fetched under (`ticketSources`).
  let sources: string | null = null;
  async function fetch(project: string, again = false) {
    const was = store.getState().lists[project];
    if (!again && was) return;
    const turn = (asked[project] ?? 0) + 1;
    asked[project] = turn;
    setList(
      project,
      was?.state === "loaded" ? { ...was, refreshing: true } : ({ state: "loading" } as const),
    );
    try {
      const reply = await ask(
        bridge,
        { ListIssues: { project } },
        "Issues",
        "The daemon did not return its tickets.",
      );
      if (asked[project] === turn) setList(project, { state: "loaded", value: reply });
    } catch (error) {
      if (asked[project] === turn) setList(project, { state: "failed", error: errorText(error) });
    }
  }
  async function view(project: string, issue: Issue) {
    const id = ticketId(project, issue.key);
    if (store.getState().texts[id]) return;
    const loading = { state: "loading" } as const;
    setText(id, loading);
    try {
      const reply = await ask(
        bridge,
        { ViewIssue: { project, key: issue.key } },
        "IssueViewed",
        "The daemon did not return its text.",
      );
      if (store.getState().texts[id] === loading) setText(id, { state: "loaded", value: reply });
    } catch (error) {
      if (store.getState().texts[id] === loading)
        setText(id, { state: "failed", error: errorText(error) });
    }
  }
  function refresh(projects: readonly string[]) {
    // Comments chosen may have moved: they are chosen again.
    store.setState({ texts: {}, briefs: {}, parts: {} });
    for (const project of projects) void fetch(project, true);
  }
  return {
    store,
    fetch,
    view,
    prune(projects: readonly string[]) {
      store.setState((s) => ({
        lists: Object.fromEntries(Object.entries(s.lists).filter(([id]) => projects.includes(id))),
        texts: Object.fromEntries(
          Object.entries(s.texts).filter(([id]) => projects.includes(id.split(":")[0] ?? "")),
        ),
        changing: Object.fromEntries(
          Object.entries(s.changing).filter(([id]) => projects.includes(id)),
        ),
        lastProjects: Object.fromEntries(
          Object.entries(s.lastProjects).filter(([, id]) => projects.includes(id)),
        ),
      }));
    },
    forgetFailed() {
      store.setState((s) => ({
        lists: Object.fromEntries(Object.entries(s.lists).filter(([, v]) => v.state !== "failed")),
        texts: Object.fromEntries(Object.entries(s.texts).filter(([, v]) => v.state !== "failed")),
      }));
    },
    /** Gives the agent part `at` of ticket `id`, or stops; false where nothing would be left. */
    choose(id: string, at: number): boolean {
      const next = toggled(store.getState().briefs[id] ?? TEXT_ALONE, at);
      if (!next) return false;
      store.setState((s) => ({ briefs: { ...s.briefs, [id]: next } }));
      return true;
    },
    /** Puts the keys on part `at` of ticket `id`. */
    reach(id: string, at: number) {
      store.setState((s) => ({ parts: { ...s.parts, [id]: at } }));
    },
    refresh,
    /**
     * The daemon's ticket plugins as they are now: one turned on or off,
     * added, gone or changed since the lists were fetched has them fetched
     * again, as tickets may come from elsewhere now, or from nowhere.
     */
    follow(now: string, projects: readonly string[]) {
      const was = sources;
      sources = now;
      if (was !== null && was !== now) refresh(projects);
    },
    /**
     * Only my tickets, for a project or the projects sharing its tickets:
     * each is changed, so they go on listing the same ones, and their
     * tickets are fetched again however the daemon answered.
     */
    async mine(projects: readonly string[], mine: boolean) {
      const lead = projects[0] ?? "";
      if (store.getState().changing[lead]) return;
      store.setState((s) => ({ changing: { ...s.changing, [lead]: true } }));
      try {
        for (const project of projects) {
          await ask(
            bridge,
            {
              ChangeSetting: { project, change: { OnlyMyTickets: mine } },
            },
            "ProjectSettings",
            "The daemon did not return its settings.",
          );
        }
      } finally {
        await Promise.all(projects.map((project) => fetch(project, true)));
        store.setState((s) => ({ changing: { ...s.changing, [lead]: false } }));
      }
    },
    closed(projects: readonly string[], key: string) {
      store.setState((s) => ({
        texts: Object.fromEntries(
          Object.entries(s.texts).filter(
            ([id]) => !projects.some((project) => id === ticketId(project, key)),
          ),
        ),
        lists: Object.fromEntries(
          Object.entries(s.lists).map(([id, list]) => [
            id,
            projects.includes(id) && list.state === "loaded"
              ? {
                  ...list,
                  value: { ...list.value, issues: list.value.issues.filter((i) => i.key !== key) },
                }
              : list,
          ]),
        ),
      }));
    },
  };
}
export type TicketsModel = ReturnType<typeof createTickets>;
const held = new WeakMap<Bridge, TicketsModel>();
export function ticketsFor(bridge: Bridge): TicketsModel {
  let model = held.get(bridge);
  if (!model) {
    model = createTickets(bridge);
    held.set(bridge, model);
  }
  return model;
}

export function groupProjects(projects: readonly string[], lists: TicketData["lists"]): string[][] {
  const groups: string[][] = [];
  const sources = new Map<string, string[]>();
  for (const project of projects) {
    const list = lists[project];
    const source = list?.state === "loaded" ? list.value.source : null;
    const found = source !== null ? sources.get(source) : undefined;
    if (found) found.push(project);
    else {
      const group = [project];
      groups.push(group);
      if (source !== null) sources.set(source, group);
    }
  }
  return groups;
}
