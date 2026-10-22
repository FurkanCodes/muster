// A session's live terminal: xterm.js on the GPU, fed the daemon's bytes as
// they come (`ARC-25`). It attaches while it is on screen and the window is
// seen, and detaches otherwise, picking up from the last byte it had. Keys
// go to the program while it has them; ⇧Q and ⌘ shortcuts stay Muster's.

import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XTerm } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import { useActions, useApp, useBridge, useUi } from "../app/context";
import type { TermFrame } from "../ipc/frames";
import type { SessionId } from "../ipc/generated/SessionId";
import type { TerminalModes } from "../ipc/generated/TerminalModes";
import { exposeForTests } from "./testing";

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

/** The modes a program set, written to xterm itself so its keys and pastes match. */
function modeSequences(modes: TerminalModes): string {
  const set = (on: boolean, mode: number) => `\x1b[?${mode}${on ? "h" : "l"}`;
  return set(modes.application_cursor, 1) + set(modes.bracketed_paste, 2004);
}

export function Terminal({ session, compact = false }: { session: SessionId; compact?: boolean }) {
  const bridge = useBridge();
  const actions = useActions();
  const { ui } = useApp();
  const visible = useUi((state) => state.visible);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const drawnIn = useUi((state) => state.drawnIn);
  const host = useRef<HTMLElement>(null);
  const term = useRef<XTerm | null>(null);
  // The position after the last byte written, so a reattach asks only for what is new.
  const nextSeq = useRef<number | null>(null);

  // One xterm for the component's life.
  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const xterm = new XTerm({
      fontFamily: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
      fontSize: compact ? 12 : 13,
      lineHeight: compact ? 1.5 : 1.54,
      cursorBlink: false,
      scrollback: 5000,
      allowProposedApi: false,
      macOptionIsMeta: false,
      theme: themeFromTokens(element),
    });
    const fit = new FitAddon();
    xterm.loadAddon(fit);
    xterm.open(element);
    // Out of Tab's way: Tab goes round the tabs, as in the terminal UI, and
    // a terminal takes the keys only on ⏎ or a click, never by Tab.
    xterm.textarea?.setAttribute("tabindex", "-1");
    try {
      const webgl = new WebglAddon();
      // Lost after a sleep or under memory pressure: fall back to the DOM renderer.
      webgl.onContextLoss(() => webgl.dispose());
      xterm.loadAddon(webgl);
    } catch {
      // No WebGL here: xterm draws with the DOM.
    }
    const typed = xterm.onData((text) => void bridge.input(session, text).catch(() => {}));
    xterm.attachCustomKeyEventHandler((event) => {
      // ⌘ shortcuts and ⇧Q (leave the terminal) are Muster's, never the program's.
      if (event.metaKey) return false;
      if (event.type === "keydown" && event.shiftKey && event.key === "Q") return false;
      return true;
    });

    // The program is told the size its terminal is drawn at, and only that:
    // a size measured on a pane laid out with a character cell to measure,
    // once per change. A pane put out of sight (0 by 0), or a font not yet
    // measured, says nothing, never xterm's own 80 by 24, which the program
    // would draw its screen at and leave behind once told the real size.
    let frame = 0;
    let told = "";
    const size = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (element.clientWidth === 0 || element.clientHeight === 0) return;
        const wanted = fit.proposeDimensions();
        if (!wanted || !Number.isFinite(wanted.cols) || !Number.isFinite(wanted.rows)) return;
        fit.fit();
        const now = `${xterm.cols}x${xterm.rows}`;
        if (now === told) return;
        told = now;
        void bridge.resize(session, xterm.cols, xterm.rows).catch(() => {});
      });
    };
    const resize = new ResizeObserver(size);
    resize.observe(element);
    // The terminal's face loads with the first terminal shown: its cell is
    // measured again once it has, and the columns that fit with it.
    const fonts = document.fonts;
    fonts?.addEventListener?.("loadingdone", size);
    term.current = xterm;
    const unexpose = exposeForTests(session, xterm);
    return () => {
      unexpose();
      cancelAnimationFrame(frame);
      resize.disconnect();
      fonts?.removeEventListener?.("loadingdone", size);
      typed.dispose();
      xterm.dispose();
      term.current = null;
      nextSeq.current = null;
    };
  }, [bridge, session, compact]);

  // Streams while the window is seen; detaches while it is not (`ARC-25`).
  useEffect(() => {
    if (!visible) return;
    const onFrame = (frame: TermFrame) => {
      const xterm = term.current;
      if (!xterm) return;
      switch (frame.kind) {
        case "replay":
          if (frame.reset || nextSeq.current === null) xterm.reset();
          xterm.write(frame.data);
          nextSeq.current = frame.seq + frame.data.byteLength;
          break;
        case "output":
          xterm.write(frame.data);
          nextSeq.current = frame.seq + frame.data.byteLength;
          break;
        // After a replay starting over, whose bytes were cut from the
        // middle of the output: the program's screen, drawn whole.
        case "screen":
          xterm.write(frame.data);
          break;
        case "event":
          if (frame.event.kind === "modes") xterm.write(modeSequences(frame.event.modes));
          break;
      }
    };
    void bridge.attach(session, nextSeq.current, true, onFrame).catch(() => {});
    return () => {
      void bridge.detach(session).catch(() => {});
    };
  }, [bridge, session, visible]);

  // The terminal takes the keys when told to, and tells the daemon it is in focus.
  useEffect(() => {
    if (hasKeys) term.current?.focus();
    else term.current?.blur();
  }, [hasKeys]);

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
    if (element && term.current && drawnIn) term.current.options.theme = themeFromTokens(element);
  }, [drawnIn]);

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
