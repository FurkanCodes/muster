// What the window shows and where the keys go: the webview's own state,
// never the daemon's. One store, so every screen reads the same answers.

import { createStore, type StoreApi } from "zustand/vanilla";
import type { Flow } from "../flows/types";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";

export type ScreenId = "needs-you" | "not-seen" | "working" | "all" | "tickets" | "settings";

/** Where typed keys go: the list (Muster's keys), a terminal (its program), or a dialog. */
export type KeysIn = "list" | "terminal" | "dialog";

export type Theme = "dark" | "light" | "system";

export type PaletteMode = "commands" | "keys";

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
  /** The project the queues and tree are narrowed to, if any. */
  project: ProjectId | null;
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
  sidebarFolded: boolean;
  theme: Theme;
  paletteOpen: boolean;
  /** What the palette shows: everything to go to or do, or every key of the screen (`h`). */
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
  select(screen: ScreenId, id: string | undefined): void;
  narrow(project: ProjectId | null): void;
  keysTo(keysIn: KeysIn): void;
  showTerminal(session: SessionId | null): void;
  showPane(mode: PaneMode): void;
  showTurn(workspace: WorkspaceId, number: number, intent: "diff" | "put-back"): void;
  foldSidebar(): void;
  setTheme(theme: Theme): void;
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
  try {
    const value = localStorage.getItem(THEME_KEY);
    return value === "light" || value === "system" ? value : "dark";
  } catch {
    return "dark";
  }
}

export function createUi(): { store: UiStore; actions: UiActions } {
  const store = createStore<UiState>(() => ({
    screen: "needs-you",
    project: null,
    selected: {},
    keysIn: "list",
    terminal: null,
    pane: "terminal",
    changesTurn: null,
    sidebarFolded: false,
    theme: storedTheme(),
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
    show: (screen) => set({ screen, keysIn: "list" }),
    select: (screen, id) => set((state) => ({ selected: { ...state.selected, [screen]: id } })),
    narrow: (project) => set((state) => ({ project: state.project === project ? null : project })),
    keysTo: (keysIn) => set({ keysIn }),
    showTerminal: (terminal) => set({ terminal }),
    showPane: (pane) => set({ pane, changesTurn: null }),
    showTurn: (workspace, number, intent) =>
      set((state) => ({
        pane: "changes",
        keysIn: "list",
        changesTurn: { workspace, number, intent, request: (state.changesTurn?.request ?? 0) + 1 },
      })),
    foldSidebar: () => set((state) => ({ sidebarFolded: !state.sidebarFolded })),
    setTheme: (theme) => {
      set({ theme });
      try {
        localStorage.setItem(THEME_KEY, theme);
      } catch {
        // Storage refused (a private window): the theme lasts as long as the window.
      }
    },
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
