// One clock for the whole window: it ticks every 15 seconds while the
// window is seen, and not at all while it is hidden (`ARC-25`). Components
// that show ages read it; nothing else keeps a timer.

import { useSyncExternalStore } from "react";
import { NOW } from "../ipc/mock/now";

const listeners = new Set<() => void>();
let now = __MOCK__ ? NOW : Date.now();
let timer: ReturnType<typeof setInterval> | undefined;

function tick(): void {
  now = Date.now();
  for (const listener of listeners) listener();
}

/** Starts or stops the clock with the window's visibility. The mock's clock stands still. */
export function runClock(visible: boolean): void {
  clearInterval(timer);
  timer = undefined;
  if (__MOCK__ || !visible) return;
  tick();
  timer = setInterval(tick, 15_000);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The wall clock in milliseconds, as of its last tick. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, () => now);
}
