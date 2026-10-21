// The daemon's state as the webview holds it: whole entities by id, and what
// the Rust side derives (`Derived`). A patch replaces only the entities it
// names and keeps every other object as it was, so a component that selects
// one session re-renders only when that session changes (`ARC-25`).

import { createStore, type StoreApi } from "zustand/vanilla";
import type { Derived } from "../ipc/generated/desktop/Derived";
import type { LinkState } from "../ipc/generated/desktop/LinkState";
import type { Patch } from "../ipc/generated/desktop/Patch";
import type { StateMessage } from "../ipc/generated/desktop/StateMessage";
import type { PluginInfo } from "../ipc/generated/PluginInfo";
import type { Preferences } from "../ipc/generated/Preferences";
import type { Project } from "../ipc/generated/Project";
import type { ResourceReport } from "../ipc/generated/ResourceReport";
import type { Session } from "../ipc/generated/Session";
import type { SessionId } from "../ipc/generated/SessionId";
import type { Workspace } from "../ipc/generated/Workspace";

export interface DaemonState {
  link: LinkState;
  /** Whether the first patch came. */
  loaded: boolean;
  projects: Readonly<Record<string, Project>>;
  workspaces: Readonly<Record<string, Workspace>>;
  sessions: Readonly<Record<string, Session>>;
  preferences: Preferences | null;
  plugins: readonly PluginInfo[];
  resources: ResourceReport | null;
  notifier: boolean;
  derived: Derived;
}

export type DaemonStore = StoreApi<DaemonState>;

export function emptyDerived(): Derived {
  return {
    needs_you: [],
    working: [],
    rows: [],
    counts: { working: 0, waiting: 0, done: 0 },
    questions: {},
  };
}

function byId<T extends { id: string }>(items: readonly T[]): Record<string, T> {
  const out: Record<string, T> = {};
  for (const item of items) out[item.id] = item;
  return out;
}

/** `items` merged into `held`, `gone` taken out; `held` itself when nothing changed. */
function merged<T extends { id: string }>(
  held: Readonly<Record<string, T>>,
  items: readonly T[],
  gone: readonly string[],
): Readonly<Record<string, T>> {
  if (items.length === 0 && gone.length === 0) return held;
  const next = { ...held };
  for (const item of items) next[item.id] = item;
  for (const id of gone) delete next[id];
  return next;
}

function sameList<T>(a: readonly T[], b: readonly T[], same: (x: T, y: T) => boolean): boolean {
  return a.length === b.length && a.every((item, at) => same(item, b[at] as T));
}

function sameQuestions(a: Derived["questions"], b: Derived["questions"]): boolean {
  const asked = Object.keys(a);
  return (
    asked.length === Object.keys(b).length &&
    asked.every((session) => {
      const lines = b[session];
      return lines !== undefined && sameList(a[session] ?? [], lines, Object.is);
    })
  );
}

/** The new derivations, keeping each part that did not change as the same object. */
function keptDerived(held: Derived, next: Derived): Derived {
  const counts =
    held.counts.working === next.counts.working &&
    held.counts.waiting === next.counts.waiting &&
    held.counts.done === next.counts.done
      ? held.counts
      : next.counts;
  const needs_you = sameList(held.needs_you, next.needs_you, Object.is)
    ? held.needs_you
    : next.needs_you;
  const working = sameList(held.working, next.working, Object.is) ? held.working : next.working;
  const rows = sameList(held.rows, next.rows, (a, b) => a.kind === b.kind && a.id === b.id)
    ? held.rows
    : next.rows;
  const questions = sameQuestions(held.questions, next.questions) ? held.questions : next.questions;
  return counts === held.counts &&
    needs_you === held.needs_you &&
    working === held.working &&
    rows === held.rows &&
    questions === held.questions
    ? held
    : { counts, needs_you, working, rows, questions };
}

function patched(state: DaemonState, patch: Patch): Partial<DaemonState> {
  const next: Partial<DaemonState> = {
    loaded: true,
    derived: keptDerived(state.derived, patch.derived),
  };
  if (patch.reset) {
    next.projects = byId(patch.projects);
    next.workspaces = byId(patch.workspaces);
    next.sessions = byId(patch.sessions);
  } else {
    next.projects = merged(state.projects, patch.projects, patch.gone.projects);
    next.workspaces = merged(state.workspaces, patch.workspaces, patch.gone.workspaces);
    next.sessions = merged(state.sessions, patch.sessions, patch.gone.sessions);
  }
  if (patch.preferences) next.preferences = patch.preferences;
  if (patch.plugins) next.plugins = patch.plugins;
  if (patch.resources) next.resources = patch.resources;
  if (patch.notifier !== null) next.notifier = patch.notifier;
  return next;
}

export interface Daemon {
  readonly store: DaemonStore;
  apply(message: StateMessage): void;
  /** Called when the person picked a session elsewhere (the notch). */
  onReveal(listener: (session: SessionId) => void): () => void;
}

export function createDaemon(): Daemon {
  const store = createStore<DaemonState>(() => ({
    link: { state: "connecting" },
    loaded: false,
    projects: {},
    workspaces: {},
    sessions: {},
    preferences: null,
    plugins: [],
    resources: null,
    notifier: false,
    derived: emptyDerived(),
  }));
  const reveals = new Set<(session: SessionId) => void>();

  return {
    store,
    apply(message) {
      switch (message.kind) {
        case "link":
          store.setState({ link: message.link });
          break;
        case "patch":
          store.setState((state) => patched(state, message.patch));
          break;
        case "reveal":
          for (const listener of reveals) listener(message.session);
          break;
      }
    },
    onReveal(listener) {
      reveals.add(listener);
      return () => reveals.delete(listener);
    },
  };
}
