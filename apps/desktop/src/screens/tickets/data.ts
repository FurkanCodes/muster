import { createStore } from "zustand/vanilla";
import type { Bridge } from "../../ipc/bridge";
import type { Issue } from "../../ipc/generated/Issue";
import type { Reply } from "../../ipc/generated/Reply";

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
  changing: Record<string, boolean>;
  lastAgent: string;
  lastProjects: Record<string, string>;
}
export function ticketId(project: string, key: string): string {
  return `${project}:${key}`;
}
export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function createTickets(bridge: Bridge) {
  const store = createStore<TicketData>(() => ({
    lists: {},
    texts: {},
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
  async function fetch(project: string, again = false) {
    const was = store.getState().lists[project];
    if (!again && was) return;
    const ask = (asked[project] ?? 0) + 1;
    asked[project] = ask;
    setList(
      project,
      was?.state === "loaded" ? { ...was, refreshing: true } : ({ state: "loading" } as const),
    );
    try {
      const reply = await bridge.command({ ListIssues: { project } });
      if (typeof reply !== "object" || !("Issues" in reply))
        throw new Error("The daemon did not return its tickets.");
      if (asked[project] === ask) setList(project, { state: "loaded", value: reply.Issues });
    } catch (error) {
      if (asked[project] === ask) setList(project, { state: "failed", error: message(error) });
    }
  }
  async function view(project: string, issue: Issue) {
    const id = ticketId(project, issue.key);
    if (store.getState().texts[id]) return;
    const loading = { state: "loading" } as const;
    setText(id, loading);
    try {
      const reply = await bridge.command({ ViewIssue: { project, key: issue.key } });
      if (typeof reply !== "object" || !("IssueViewed" in reply))
        throw new Error("The daemon did not return its text.");
      if (store.getState().texts[id] === loading)
        setText(id, { state: "loaded", value: reply.IssueViewed });
    } catch (error) {
      if (store.getState().texts[id] === loading)
        setText(id, { state: "failed", error: message(error) });
    }
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
    refresh(projects: readonly string[]) {
      store.setState({ texts: {} });
      for (const project of projects) void fetch(project, true);
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
          const reply = await bridge.command({
            ChangeSetting: { project, change: { OnlyMyTickets: mine } },
          });
          if (typeof reply !== "object" || !("ProjectSettings" in reply))
            throw new Error("The daemon did not return its settings.");
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
