// A line carried by the pointer from the sidebar: a session's, to be
// placed in a panel of its own where it is let go on the pane, or a
// project's, to go where it is let go on the list (`PRODUCT.md` §6a).
// Carried by pointer events, not the browser's drag and drop: the app's
// window takes every drag over it for a folder dropped from the Finder
// (`dragDropEnabled`), so a drag inside the page never reaches the page.

import { type PointerEvent, useEffect, useSyncExternalStore } from "react";
import type { SessionId } from "../ipc/generated/SessionId";

/** A session's line, or a project's by its id as the list has it. */
export type Carried = { kind: "session"; id: SessionId } | { kind: "project"; id: string };

/** What is carried, and where the pointer is with it, in the window's coordinates. */
export interface Carrying {
  what: Carried;
  x: number;
  y: number;
}

/** How far the pointer goes, pressed, before the line is taken up: less is a click. */
const TAKEN_UP_AT = 5;

let now: Carrying | null = null;
const watching = new Set<() => void>();
/** Where a line may be let go, each answering whether it took it. */
const takers = new Set<(carrying: Carrying) => boolean>();

function set(next: Carrying | null): void {
  now = next;
  for (const told of watching) told();
}

function watch(told: () => void): () => void {
  watching.add(told);
  return () => watching.delete(told);
}

/** Tells `told` what is carried each time it moves or is let go; the way to stop. */
export function onCarrying(told: (carrying: Carrying | null) => void): () => void {
  return watch(() => told(now));
}

/** What is being carried now, and where, if anything is. */
export function useCarrying(): Carrying | null {
  return useSyncExternalStore(watch, () => now);
}

/** Lets a carried line be let go here: `take` answers whether it took it. */
export function useTakesCarried(take: (carrying: Carrying) => boolean): void {
  useEffect(() => {
    takers.add(take);
    return () => {
      takers.delete(take);
    };
  }, [take]);
}

/** A click the carry ended in, where it is let go: no click on what is under it. */
function swallowClick(): void {
  const swallow = (event: MouseEvent) => {
    event.stopPropagation();
    event.preventDefault();
  };
  window.addEventListener("click", swallow, { capture: true, once: true });
  setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
}

/**
 * The pointer pressed on `what`'s line: carried once it moves, let go
 * where it is released, or put down with ⎋. A press that does not move is
 * the line's click, as before.
 */
export function pressToCarry(event: PointerEvent<HTMLElement>, what: Carried): void {
  if (event.button !== 0 || now) return;
  const line = event.currentTarget;
  const from = { x: event.clientX, y: event.clientY };
  let taken = false;
  const move = (moved: globalThis.PointerEvent) => {
    if (!taken && Math.hypot(moved.clientX - from.x, moved.clientY - from.y) < TAKEN_UP_AT) return;
    if (!taken) {
      taken = true;
      line.classList.add("is-carried");
      document.documentElement.classList.add("is-carrying");
    }
    set({ what, x: moved.clientX, y: moved.clientY });
  };
  const end = (at: { x: number; y: number } | null) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", putDown, { capture: true });
    if (!taken) return;
    line.classList.remove("is-carried");
    document.documentElement.classList.remove("is-carrying");
    set(null);
    if (!at) return;
    swallowClick();
    const carrying = { what, ...at };
    for (const take of takers) if (take(carrying)) return;
  };
  const up = (released: globalThis.PointerEvent) =>
    end({ x: released.clientX, y: released.clientY });
  const cancel = () => end(null);
  const putDown = (key: KeyboardEvent) => {
    if (key.key !== "Escape" || !taken) return;
    key.stopPropagation();
    key.preventDefault();
    end(null);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", putDown, { capture: true });
}
