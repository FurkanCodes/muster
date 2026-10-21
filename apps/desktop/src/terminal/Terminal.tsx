// A session's live terminal: xterm.js on the GPU, fed the daemon's bytes as
// they come (`ARC-25`). It attaches while it is on screen and the window is
// seen, and detaches otherwise, picking up from the last byte it had. Keys
// go to the program while it has them; ⇧Q and ⌘ shortcuts stay Muster's.

import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XTerm } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import { useActions, useBridge, useUi } from "../app/context";
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
    selectionBackground: token("--surface-lit"),
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
  const visible = useUi((state) => state.visible);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const theme = useUi((state) => state.theme);
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

    let frame = 0;
    const resize = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        fit.fit();
        void bridge.resize(session, xterm.cols, xterm.rows).catch(() => {});
      });
    });
    resize.observe(element);
    term.current = xterm;
    const unexpose = exposeForTests(session, xterm);
    return () => {
      unexpose();
      cancelAnimationFrame(frame);
      resize.disconnect();
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

  useEffect(() => {
    actions.showTerminal(session);
    return () => actions.showTerminal(null);
  }, [actions, session]);

  // Follows the theme: tokens change with data-theme.
  useEffect(() => {
    const element = host.current;
    if (element && term.current && theme) term.current.options.theme = themeFromTokens(element);
  }, [theme]);

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
    />
  );
}
