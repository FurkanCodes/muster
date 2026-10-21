// The colours the window's terminals draw in, as the daemon takes them
// (`ClientFrame::Colours`): a program in a session asking its terminal's
// colours (OSC 10, 11 and 4) is told the theme's, as the terminal UI tells
// the person's terminal's. The text and background are the design tokens
// the terminal reads (Terminal.tsx); the palette is xterm.js's own, which
// the theme leaves as it is.

import type { Rgb } from "../ipc/generated/Rgb";
import type { TerminalColours } from "../ipc/generated/TerminalColours";

/** xterm.js's sixteen colours: black, red, green, yellow, blue, magenta, cyan, white, then bright. */
const PALETTE = [
  "#2e3436",
  "#cc0000",
  "#4e9a06",
  "#c4a000",
  "#3465a4",
  "#75507b",
  "#06989a",
  "#d3d7cf",
  "#555753",
  "#ef2929",
  "#8ae234",
  "#fce94f",
  "#729fcf",
  "#ad7fa8",
  "#34e2e2",
  "#eeeeec",
] as const;

/** `#rrggbb` (or `#rgb`) in sixteen bits a channel, as X11 scales it; null for anything else. */
export function rgbOf(colour: string): Rgb | null {
  const hex = colour.trim().replace(/^#/, "");
  const full = /^[0-9a-f]{3}$/i.test(hex) ? hex.replace(/./g, "$&$&") : hex;
  if (!/^[0-9a-f]{6}$/i.test(full)) return null;
  const channel = (at: number) => Number.parseInt(full.slice(at, at + 2), 16) * 257;
  return { red: channel(0), green: channel(2), blue: channel(4) };
}

/** The colours of the theme `element` is drawn in. */
export function terminalColours(element: Element): TerminalColours {
  const css = getComputedStyle(element);
  return {
    foreground: rgbOf(css.getPropertyValue("--text")),
    background: rgbOf(css.getPropertyValue("--surface-term")),
    palette: PALETTE.map(rgbOf),
  };
}
