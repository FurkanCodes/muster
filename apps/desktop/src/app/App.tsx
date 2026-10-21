// The window: title bar, sidebar, the screen shown, status bar.

import { Suspense, useEffect, useState } from "react";
import type { Bridge } from "../ipc/bridge";
import { installQueueHistory } from "../screens/queue/history";
import { createDaemon } from "../state/daemon";
import { createUi } from "../state/ui";
import { runClock } from "./clock";
import { AppContext, type App as AppValue, useApp, useUi } from "./context";
import { installKeys } from "./keys";
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

function useTheme(): void {
  const theme = useUi((state) => state.theme);
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    if (theme !== "system") return;
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

function Screen() {
  const screen = useUi((state) => state.screen);
  const Shown = SCREENS[screen];
  return (
    <main className="app-screen">
      <Suspense fallback={null}>
        <Shown />
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
      <div className="app-body">
        <Sidebar />
        <Screen />
      </div>
      <StatusBar />
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
