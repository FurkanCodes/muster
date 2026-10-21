// What the window shows and where the keys go: the webview's own state,
// never the daemon's. One store, so every screen reads the same answers.

import { createStore, type StoreApi } from "zustand/vanilla";
import type { Flow } from "../flows/types";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { keep, kept } from "./kept";

export type ScreenId = "all" | "needs-you" | "tickets" | "settings";

/** The tabs in the terminal UI's order, as Tab and ⇧Tab go round them (`next_tab`). */
export const TABS: readonly ScreenId[] = ["all", "needs-you", "tickets", "settings"];

/** The tabs the sidebar's list shows: every session by project, or what needs the person. */
export type ListId = "all" | "needs-you";

export function isList(screen: ScreenId): screen is ListId {
  return screen === "all" || screen === "needs-you";
}

/** Where typed keys go: the list (Muster's keys), a terminal (its program), or a dialog. */
export type KeysIn = "list" | "terminal" | "dialog";

export type Theme = "dark" | "light" | "system";

/** The palette: everything to go to or do, every key of the screen, or every key in the changes. */
export type PaletteMode = "commands" | "keys" | "changes";

/** What the session pane shows: the terminal, or the workspace's changes. */
export type PaneMode = "terminal" | "changes";

/** A key and what it does, for the status bar and menus. */
export interface Hint {
  key: string;
  does: string;
}

export interface Note {
  text: string;
  error: boolean;
}

export interface UiState {
  screen: ScreenId;
  /** What the sidebar lists: the list shown, or the one shown last beside Tickets and Settings. */
  list: ListId;
  /** Each screen's selected item (a session, ticket or setting id). */
  selected: Partial<Record<ScreenId, string>>;
  keysIn: KeysIn;
  /** The session whose terminal is on screen, told to the daemon as in focus. */
  terminal: SessionId | null;
  /** What the session pane shows. */
  pane: PaneMode;
  /** This turn opens the changes pane at a specific entry. */
  changesTurn: {
    workspace: WorkspaceId;
    number: number;
    intent: "diff" | "put-back";
    request: number;
  } | null;
  /** The shell put beside the session (`T`); it runs on when put away. */
  shell: SessionId | null;
  /** The session filling the window (`f`), its panels with it. */
  fullScreen: SessionId | null;
  sidebarFolded: boolean;
  theme: Theme;
  /** The theme the window is drawn in now: the system's, where the theme follows it. */
  drawnIn: "dark" | "light";
  paletteOpen: boolean;
  /** What the palette shows: everything to go to or do, or every key of the screen (`h`) or of the changes (`?` there). */
  paletteMode: PaletteMode;
  /** The dialog open over the window, if any (src/flows). */
  flow: Flow | null;
  /** The agent the person started last, offered first next time. */
  lastAgent: string;
  /** The screen's keys, as the status bar lists them. */
  hints: readonly Hint[];
  note: Note | null;
  /** Whether the person can see the window: animations and streams stop when not. */
  visible: boolean;
}

export interface UiActions {
  show(screen: ScreenId): void;
  /** Goes `by` tabs on round {@link TABS}, as Tab (1) and ⇧Tab (-1) do. */
  cycle(by: number): void;
  select(screen: ScreenId, id: string | undefined): void;
  keysTo(keysIn: KeysIn): void;
  showTerminal(session: SessionId | null): void;
  showPane(mode: PaneMode): void;
  showTurn(workspace: WorkspaceId, number: number, intent: "diff" | "put-back"): void;
  showShell(shell: SessionId | null): void;
  fillWindow(session: SessionId | null): void;
  foldSidebar(): void;
  setTheme(theme: Theme): void;
  /** Says which theme the window is drawn in, as the system's changes too. */
  drawIn(drawnIn: "dark" | "light"): void;
  openPalette(open: boolean, mode?: PaletteMode): void;
  /** Opens a dialog of src/flows over the window, or closes it with null. */
  openFlow(flow: Flow | null): void;
  choseAgent(agent: string): void;
  setHints(hints: readonly Hint[]): void;
  /** Says what just happened in the status bar; it goes after five seconds, an error at the next key. */
  tell(text: string, error?: boolean): void;
  clearNote(): void;
  setVisible(visible: boolean): void;
}

export type UiStore = StoreApi<UiState>;

const THEME_KEY = "muster.theme";

function storedTheme(): Theme {
  const value = kept(THEME_KEY);
  return value === "light" || value === "system" ? value : "dark";
}

/** The window on `screen`: the keys Muster's, nothing filling it, the sidebar listing a list shown. */
function shown(screen: ScreenId): Partial<UiState> {
  return isList(screen)
    ? { screen, list: screen, keysIn: "list", fullScreen: null }
    : { screen, keysIn: "list", fullScreen: null };
}

export function createUi(): { store: UiStore; actions: UiActions } {
  const store = createStore<UiState>(() => ({
    screen: "needs-you",
    list: "needs-you",
    selected: {},
    keysIn: "list",
    terminal: null,
    pane: "terminal",
    changesTurn: null,
    shell: null,
    fullScreen: null,
    sidebarFolded: false,
    theme: storedTheme(),
    drawnIn: "dark",
    paletteOpen: false,
    paletteMode: "commands",
    flow: null,
    lastAgent: "claude",
    hints: [],
    note: null,
    visible: true,
  }));
  const set = store.setState;
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  const actions: UiActions = {
    show: (screen) => set(shown(screen)),
    select: (screen, id) => set((state) => ({ selected: { ...state.selected, [screen]: id } })),
    cycle: (by) =>
      set((state) => {
        const at = Math.max(0, TABS.indexOf(state.screen));
        return shown(TABS[(at + by + TABS.length) % TABS.length] ?? state.screen);
      }),
    keysTo: (keysIn) => set({ keysIn }),
    showTerminal: (terminal) => set({ terminal }),
    showPane: (pane) => set({ pane, changesTurn: null }),
    showTurn: (workspace, number, intent) =>
      set((state) => ({
        pane: "changes",
        keysIn: "list",
        changesTurn: { workspace, number, intent, request: (state.changesTurn?.request ?? 0) + 1 },
      })),
    showShell: (shell) => set({ shell }),
    fillWindow: (fullScreen) => set({ fullScreen }),
    foldSidebar: () => set((state) => ({ sidebarFolded: !state.sidebarFolded })),
    setTheme: (theme) => {
      set({ theme });
      keep(THEME_KEY, theme);
    },
    drawIn: (drawnIn) => set({ drawnIn }),
    openPalette: (paletteOpen, paletteMode = "commands") =>
      set(paletteOpen ? { paletteOpen, paletteMode, flow: null } : { paletteOpen }),
    openFlow: (flow) => set({ flow, paletteOpen: false }),
    choseAgent: (lastAgent) => set({ lastAgent }),
    setHints: (hints) => set({ hints }),
    tell: (text, error = false) => {
      clearTimeout(noteTimer);
      set({ note: { text, error } });
      if (!error) noteTimer = setTimeout(() => set({ note: null }), 5000);
    },
    clearNote: () => {
      clearTimeout(noteTimer);
      set({ note: null });
    },
    setVisible: (visible) => set({ visible }),
  };
  return { store, actions };
}
