// Whether the person can see the window (`ARC-25`): when they cannot,
// nothing animates (CSS pauses on `data-visible="false"`), the clock stops,
// terminals detach, and the daemon is told nobody looks, so news stays news.

import type { App } from "./context";

export function installVisibility(app: App): () => void {
  let focused = document.hasFocus();
  let told = "";
  const tell = () => {
    const visible = document.visibilityState === "visible";
    if (app.ui.getState().visible !== visible) app.actions.setVisible(visible);
    document.documentElement.dataset.visible = String(visible);
    const looking = visible && focused;
    const terminal = looking ? app.ui.getState().terminal : null;
    const now = `${looking}:${terminal ?? ""}`;
    if (now === told) return;
    told = now;
    void app.bridge.looking(looking, terminal).catch(() => {
      told = "";
    });
  };
  const onFocus = () => {
    focused = true;
    tell();
  };
  const onBlur = () => {
    focused = false;
    tell();
  };
  document.addEventListener("visibilitychange", tell);
  window.addEventListener("focus", onFocus);
  window.addEventListener("blur", onBlur);
  // The terminal in focus changes as the person moves between sessions.
  const unsubscribe = app.ui.subscribe((state, previous) => {
    if (state.terminal !== previous.terminal) tell();
  });
  tell();
  return () => {
    document.removeEventListener("visibilitychange", tell);
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("blur", onBlur);
    unsubscribe();
  };
}
