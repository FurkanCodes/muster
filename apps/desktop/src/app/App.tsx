// The window: title bar, sidebar, the screen shown, status bar.

import { Suspense, useEffect, useState } from "react";
import { Away, AwayBanner } from "../flows/Away";
import { SESSION_LISTS } from "../flows/actions";
import { Flows } from "../flows/Flows";
import { Welcome } from "../flows/Welcome";
import type { Bridge } from "../ipc/bridge";
import { screenKeys } from "../keys/keymap";
import { KEY_SCREENS, useKeymap } from "../keys/useKeymap";
import { installQueueHistory } from "../screens/queue/history";
import { createDaemon } from "../state/daemon";
import { createUi } from "../state/ui";
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
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";
import { SCREENS } from "./screens";
import { TitleBar } from "./TitleBar";
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
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      const root = document.documentElement;
      root.dataset.theme = dark ? "dark" : "light";
      void bridge.colours(terminalColours(root)).catch(() => {});
    };
    apply();
    if (theme !== "system") return;
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, bridge]);
}

/**
 * The window's own actions on the person's keys, beneath every screen's:
 * `fold_list` folds the sidebar, as ⌘\\ does (the terminal UI's ⌃\\ folds
 * its list).
 */
function WindowKeys() {
  const screen = useUi((state) => state.screen);
  const map = useKeymap();
  const actions = useActions();
  const handlers: Record<string, () => void> = {};
  for (const [dom, action] of screenKeys(map, KEY_SCREENS[screen])) {
    if (action === "fold_list") handlers[dom] = () => actions.foldSidebar();
  }
  useKeys(handlers, { beneath: true });
  return null;
}

function Screen() {
  const screen = useUi((state) => state.screen);
  const loaded = useDaemon((state) => state.loaded);
  const away = useDaemon((state) => state.link.state === "down");
  const empty = useDaemon((state) => Object.keys(state.projects).length === 0);
  const Shown = SCREENS[screen];
  // Out of reach, the screen stays as it was, still (src/flows: Away), or
  // Kep waits where nothing was known yet; with no projects, the lists of
  // sessions welcome a first one.
  return (
    <main className="app-screen" inert={away && loaded ? true : undefined}>
      <Suspense fallback={null}>
        {away && !loaded ? (
          <Away />
        ) : loaded && empty && SESSION_LISTS.includes(screen) ? (
          <Welcome />
        ) : (
          <Shown />
        )}
      </Suspense>
    </main>
  );
}

function Window() {
  useTheme();
  useApp();
  const fullScreen = useUi((state) => state.fullScreen !== null);
  return (
    <div className="mu-root app" data-full-screen={fullScreen}>
      <TitleBar />
      <AwayBanner />
      <div className="app-body">
        <Sidebar />
        <Screen />
      </div>
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
