// The window: the sidebar and its list, the session or the page shown
// beside it, the status bar.

import { type ReactNode, Suspense, useEffect, useState } from "react";
import { Away, AwayBanner } from "../flows/Away";
import { SESSION_LISTS } from "../flows/actions";
import { Flows } from "../flows/Flows";
import { Welcome } from "../flows/Welcome";
import type { Bridge } from "../ipc/bridge";
import { screenKeys } from "../keys/keymap";
import { KEY_SCREENS, useKeymap } from "../keys/useKeymap";
import { installQueueHistory } from "../screens/queue/history";
import { createDaemon } from "../state/daemon";
import { createUi, isList, type UiActions } from "../state/ui";
import { terminalColours } from "../terminal/colours";
import { runClock } from "./clock";
import {
  AppContext,
  type App as AppValue,
  useActions,
  useApp,
  useBridge,
  useDaemon,
  useUi,
} from "./context";
import { installKeys, useKeys } from "./keys";
import { StatusBar } from "./StatusBar";
import { LISTS, PAGES } from "./screens";
import { installVisibility } from "./visibility";

/** The app's stores, made once for the window's life (advanced-init-once). */
function useAppValue(bridge: Bridge): AppValue {
  const [app] = useState<AppValue>(() => {
    const { store, actions } = createUi();
    return { bridge, daemon: createDaemon(), ui: store, actions };
  });
  return app;
}

/** Wires the window to the world once: the link, keys, visibility, the clock, the theme. */
function useWiring(app: AppValue): void {
  useEffect(() => {
    const keys = installKeys(app);
    const visibility = installVisibility(app);
    const history = installQueueHistory(app.daemon);
    const revealed = app.daemon.onReveal((session) => {
      app.actions.show("needs-you");
      app.actions.select("needs-you", session);
      app.actions.keysTo("terminal");
    });
    const clock = app.ui.subscribe((state, previous) => {
      if (state.visible !== previous.visible) runClock(state.visible);
    });
    runClock(app.ui.getState().visible);
    void app.bridge
      .open((message) => app.daemon.apply(message))
      .catch(() => {
        // The link says itself how it stands; a failure to open shows as out of reach.
      });
    return () => {
      keys();
      visibility();
      history();
      revealed();
      clock();
      runClock(false);
    };
  }, [app]);
}

/**
 * Draws the window in the person's theme, the system's where they chose
 * it, and tells the daemon the colours its terminals are drawn in, so a
 * program asking its terminal's is told them (terminal/colours.ts).
 */
function useTheme(): void {
  const theme = useUi((state) => state.theme);
  const bridge = useBridge();
  const actions = useActions();
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      const root = document.documentElement;
      root.dataset.theme = dark ? "dark" : "light";
      // The terminals redraw in it: the theme chosen stays "system" as the Mac's changes.
      actions.drawIn(dark ? "dark" : "light");
      void bridge.colours(terminalColours(root)).catch(() => {});
    };
    apply();
    if (theme !== "system") return;
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, bridge, actions]);
}

/** What the tab keys do, as the terminal UI's: Tab and ⇧Tab go round, 1–4 go to one. */
const TAB_KEYS: Readonly<Record<string, (actions: UiActions) => void>> = {
  next_tab: (actions) => actions.cycle(1),
  previous_tab: (actions) => actions.cycle(-1),
  sessions_tab: (actions) => actions.show("all"),
  needs_you_tab: (actions) => actions.show("needs-you"),
  tickets_tab: (actions) => actions.show("tickets"),
  settings_tab: (actions) => actions.show("settings"),
};

/**
 * The window's own actions on the person's keys, beneath every screen's:
 * the tab keys, and `fold_list`, which folds the sidebar as ⌘\\ does (the
 * terminal UI's ⌃\\ folds its list).
 */
function WindowKeys() {
  const screen = useUi((state) => state.screen);
  const map = useKeymap();
  const actions = useActions();
  const handlers: Record<string, () => void> = {};
  for (const [dom, action] of screenKeys(map, KEY_SCREENS[screen])) {
    if (action === "fold_list") handlers[dom] = () => actions.foldSidebar();
    const tab = TAB_KEYS[action];
    if (tab) handlers[dom] = () => tab(actions);
  }
  useKeys(handlers, { beneath: true });
  return null;
}

/**
 * The sidebar's list and what is beside it. The list's screen draws both,
 * the list into the sidebar and its session beside it, and stays drawn
 * while Tickets or Settings is shown (`active` false: its keys are theirs).
 */
function Body() {
  const screen = useUi((state) => state.screen);
  const list = useUi((state) => state.list);
  const loaded = useDaemon((state) => state.loaded);
  const away = useDaemon((state) => state.link.state === "down");
  const empty = useDaemon((state) => Object.keys(state.projects).length === 0);
  const List = LISTS[list];
  // Out of reach, the window stays as it was, still (src/flows: Away), or
  // Kep waits where nothing was known yet; with no projects, the lists of
  // sessions welcome a first one.
  let page: ReactNode = null;
  if (away && !loaded) page = <Away />;
  else if (loaded && empty && SESSION_LISTS.includes(screen)) page = <Welcome />;
  else if (!isList(screen)) {
    const Shown = PAGES[screen];
    page = <Shown />;
  }
  return (
    <div className="app-body" inert={away && loaded ? true : undefined}>
      {/* Apart, so a page loading for the first time never takes the list away meanwhile. */}
      <Suspense fallback={null}>
        <List active={page === null} />
      </Suspense>
      {page ? (
        <main className="app-screen">
          <Suspense fallback={null}>{page}</Suspense>
        </main>
      ) : null}
    </div>
  );
}

function Window() {
  useTheme();
  useApp();
  const fullScreen = useUi((state) => state.fullScreen !== null);
  return (
    <div className="mu-root app" data-full-screen={fullScreen}>
      <Body />
      <AwayBanner />
      <StatusBar />
      <Flows />
      <WindowKeys />
    </div>
  );
}

export function App({ bridge }: { bridge: Bridge }) {
  const app = useAppValue(bridge);
  useWiring(app);
  return (
    <AppContext value={app}>
      <Window />
    </AppContext>
  );
}
