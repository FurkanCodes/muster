// What every part of the window reaches: the bridge, the daemon's state, the
// window's own state and the clock. Components read state through selectors
// (`useDaemon(s => s.sessions[id])`), so each re-renders only for its slice.

import { createContext, use } from "react";
import { useStore } from "zustand";
import type { Bridge } from "../ipc/bridge";
import type { Daemon, DaemonState } from "../state/daemon";
import type { UiActions, UiState, UiStore } from "../state/ui";

export interface App {
  bridge: Bridge;
  daemon: Daemon;
  ui: UiStore;
  actions: UiActions;
}

export const AppContext = createContext<App | null>(null);

export function useApp(): App {
  const app = use(AppContext);
  if (!app) throw new Error("useApp() outside <App>");
  return app;
}

/** A slice of the daemon's state; re-renders only when the slice changes. */
export function useDaemon<T>(select: (state: DaemonState) => T): T {
  return useStore(useApp().daemon.store, select);
}

/** A slice of the window's own state. */
export function useUi<T>(select: (state: UiState) => T): T {
  return useStore(useApp().ui, select);
}

export function useActions(): UiActions {
  return useApp().actions;
}

export function useBridge(): Bridge {
  return useApp().bridge;
}
