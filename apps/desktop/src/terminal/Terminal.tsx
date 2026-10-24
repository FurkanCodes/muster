// A session's live terminal: xterm.js on the GPU, fed the daemon's bytes as
// they come (`ARC-25`). The xterm is the session's kept one (kept.ts), put
// in this pane while it shows and taken out as it goes, so moving the
// session's terminal never makes it afresh nor replays its output. It
// attaches while it is on screen and the window is seen, and detaches
// otherwise, picking up from the last byte it had. Keys go to the program
// while it has them; ⇧Q and ⌘ shortcuts stay Muster's.

import type { Terminal as XTerm } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { useActions, useApp, useBridge, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { type KeptTerminal, keptTerminals } from "./kept";

/**
 * One showing of a session's kept terminal in this pane: a new one each
 * time it is taken, so what follows a showing (its fit, its stream) is
 * that showing's alone, never one gone or another session's.
 */
interface Showing {
  readonly terminal: KeptTerminal;
  readonly session: SessionId;
}

/** The colours of the theme in use, read from the design tokens. */
function themeFromTokens(element: Element) {
  const css = getComputedStyle(element);
  const token = (name: string) => css.getPropertyValue(name).trim();
  return {
    background: token("--surface-term"),
    foreground: token("--text"),
    cursor: token("--text"),
    cursorAccent: token("--surface-term"),
    selectionBackground: token("--text-selection"),
  };
}

/** Draws `xterm` in the colours the window's tokens give `element` now. */
function drawIn(xterm: XTerm, element: Element) {
  xterm.options.theme = themeFromTokens(element);
}

export function Terminal({ session, compact = false }: { session: SessionId; compact?: boolean }) {
  const app = useApp();
  const bridge = useBridge();
  const actions = useActions();
  const { ui } = app;
  const visible = useUi((state) => state.visible);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const drawnIn = useUi((state) => state.drawnIn);
  const host = useRef<HTMLElement>(null);
  const [showing, setShowing] = useState<Showing | null>(null);
  // The showing in this pane now, as the effects that follow it check: a
  // render's own may be gone already.
  const current = useRef<Showing | null>(null);
  // The showing fitted to this pane at least once: only then is it fed.
  // Bytes written to xterm's own 80 by 24 and fitted after are wrapped
  // again by xterm, and the program's screen drawn into it is scrambled.
  const [fitted, setFitted] = useState<Showing | null>(null);
  const kept = showing?.terminal ?? null;

  // The session's kept terminal, in this pane while it shows.
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const keeper = keptTerminals(app);
    const lease = keeper.take(session);
    const terminal = lease.item;
    const xterm = terminal.xterm;
    const shown: Showing = { terminal, session };
    current.current = shown;
    terminal.showIn(element, compact, themeFromTokens(element));
    // Seen or not before it may draw on the GPU, so a hidden window makes no context.
    terminal.gpu.see(ui.getState().visible);

    // The program is told the size its terminal is drawn at, and only that:
    // a size measured on a pane laid out with a character cell to measure,
    // once per change. A pane put out of sight (0 by 0), or a font not yet
    // measured, says nothing, never xterm's own 80 by 24, which the program
    // would draw its screen at and leave behind once told the real size.
    // Out of sight, the pane's terminal also streams nothing and holds no
    // GPU context (`ARC-25`): a panel the layout has no room for is laid
    // out at 0 by 0. Both come back with its next size.
    let frame = 0;
    const measure = () => {
      if (element.clientWidth === 0 || element.clientHeight === 0) {
        terminal.gpu.want(false);
        setFitted(null);
        return;
      }
      const wanted = terminal.fit.proposeDimensions();
      if (!wanted || !Number.isFinite(wanted.cols) || !Number.isFinite(wanted.rows)) return;
      terminal.fit.fit();
      terminal.gpu.want(true);
      setFitted(shown);
      const now = `${xterm.cols}x${xterm.rows}`;
      if (now === terminal.told) return;
      terminal.told = now;
      void bridge.resize(session, xterm.cols, xterm.rows).catch(() => {});
    };
    const size = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    // At once where the pane is laid out already, so the first bytes find it fitted.
    measure();
    const resize = new ResizeObserver(size);
    resize.observe(element);
    // The terminal's face loads with the first terminal shown: its cell is
    // measured again once it has, and the columns that fit with it.
    const fonts = document.fonts;
    fonts?.addEventListener?.("loadingdone", size);
    setShowing(shown);
    return () => {
      if (current.current === shown) current.current = null;
      cancelAnimationFrame(frame);
      resize.disconnect();
      fonts?.removeEventListener?.("loadingdone", size);
      terminal.putAway();
      keeper.give(lease);
      setShowing(null);
      setFitted(null);
    };
  }, [app, bridge, ui, session, compact]);

  // Streams while the window is seen, once fitted; detaches while it is not
  // (`ARC-25`). A frame of a stream let go never reaches the terminal.
  // Only the showing in the pane now, of this session, once fitted: a
  // render's showing may be another session's, gone in this same commit.
  useEffect(() => {
    if (!showing || fitted !== showing || !visible) return;
    if (current.current !== showing || showing.session !== session) return;
    const terminal = showing.terminal;
    const feed = terminal.startFeed();
    void bridge.attach(session, terminal.nextSeq, true, feed.onFrame).catch(() => {});
    return () => {
      terminal.stopFeed(feed);
      void bridge.detach(session).catch(() => {});
    };
  }, [bridge, session, showing, fitted, visible]);

  // A lost WebGL context is made again only while the window is seen.
  useEffect(() => {
    kept?.gpu.see(visible);
  }, [kept, visible]);

  // The terminal takes the keys when told to, and tells the daemon it is in focus.
  useEffect(() => {
    if (!kept) return;
    if (hasKeys) kept.xterm.focus();
    else kept.xterm.blur();
  }, [kept, hasKeys]);

  // With panels beside it, the terminal in focus is the one that had it, or
  // this one where none did; each lets go only of its own.
  useEffect(() => {
    if (ui.getState().terminal === null) actions.showTerminal(session);
    return () => {
      if (ui.getState().terminal === session) actions.showTerminal(null);
    };
  }, [actions, ui, session]);

  // Follows the theme the window is drawn in, the Mac's too where it follows
  // the Mac: the tokens change with data-theme, set before `drawnIn` is.
  useEffect(() => {
    const element = host.current;
    if (element && kept && drawnIn) drawIn(kept.xterm, element);
  }, [kept, drawnIn]);

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the keyboard reaches the terminal through xterm's textarea.
    <section
      ref={host}
      className={`mu-term app-terminal ${compact ? "mu-term--compact" : ""} ${hasKeys ? "has-keys" : ""}`}
      aria-label="Terminal"
      onClick={() => {
        actions.showTerminal(session);
        actions.keysTo("terminal");
      }}
      // Wherever its input got the focus from, the window says the terminal
      // has the keys, so what the window says and where keys go never part.
      onFocus={() => {
        const state = ui.getState();
        if (state.keysIn === "terminal" && state.terminal === session) return;
        if (state.keysIn === "dialog") return;
        actions.showTerminal(session);
        actions.keysTo("terminal");
      }}
    />
  );
}
