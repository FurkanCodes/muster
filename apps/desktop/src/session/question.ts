// Text a program wrote, as the banner over a session's terminal shows what
// it waits for ("wants to run cargo build").

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
