// What a session that asks the person something asks, as its queue item and
// banner show it: the agent's own last lines, as the Rust side reads them off
// its screen while the window is seen (`Derived.questions`); until they come,
// what the agent's hooks say it waits for ("wants to run cargo build").

import type { Session } from "../ipc/generated/Session";
import { doing } from "../lib/status";

/** The most lines a question shows (design system: QueueItem). */
export const QUESTION_LINES = 4;

/** Text a program wrote, made safe to show: no escapes, no control characters. */
export function plainText(text: string): string {
  return (
    text
      // biome-ignore lint/suspicious/noControlCharactersInRegex: an OSC sequence (a title, a link) is taken out whole.
      .replace(/\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)/g, "")
      // biome-ignore lint/suspicious/noControlCharactersInRegex: a CSI sequence (a colour, a move) is taken out whole.
      .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
      // biome-ignore lint/suspicious/noControlCharactersInRegex: the control characters left, but for line breaks.
      .replace(/[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/g, "")
  );
}

/** The question's last lines, at most four; the first is the one that asks. */
export function questionOf(
  session: Pick<Session, "status" | "activity">,
  screen: readonly string[] = [],
): string[] {
  const lines = (text: string) =>
    plainText(text)
      .split("\n")
      .filter((line) => line.trim() !== "")
      .slice(-QUESTION_LINES);
  const own = lines(screen.join("\n"));
  return own.length > 0 ? dedented(own) : lines(doing(session));
}

/** Lines without the indent they all share, as a screen indents a dialog. */
function dedented(lines: string[]): string[] {
  const indent = Math.min(...lines.map((line) => line.length - line.trimStart().length));
  return lines.map((line) => line.slice(indent));
}
