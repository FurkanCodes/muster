// What a terminal shows, for end-to-end tests: xterm draws on the GPU, so
// its text and the background it draws on are not in the page. Only mock builds and the app built for its
// end-to-end tests have this; both flags are false in the app shipped, and
// the bundler drops it.

import type { Terminal } from "@xterm/xterm";

type Shown = Record<string, () => string>;

export function exposeForTests(session: string, xterm: Terminal): () => void {
  if (!__MOCK__ && !__E2E__) return () => {};
  const holder = window as unknown as { __musterTerminals?: Shown; __musterBackgrounds?: Shown };
  holder.__musterTerminals ??= {};
  holder.__musterBackgrounds ??= {};
  holder.__musterBackgrounds[session] = () => xterm.options.theme?.background ?? "";
  holder.__musterTerminals[session] = () => {
    const buffer = xterm.buffer.active;
    const lines: string[] = [];
    for (let row = 0; row < buffer.length; row += 1) {
      lines.push(buffer.getLine(row)?.translateToString(true) ?? "");
    }
    return lines.join("\n").trimEnd();
  };
  return () => {
    delete holder.__musterTerminals?.[session];
    delete holder.__musterBackgrounds?.[session];
  };
}
