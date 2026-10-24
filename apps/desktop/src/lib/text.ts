// Text as the window shows it.

/** `muster_core::text::one_line`: line breaks and tabs as spaces, other control characters dropped. */
export function oneLine(text: string): string {
  return text.replace(/[\n\r\t]/g, " ").replace(/\p{Cc}/gu, "");
}

/** What went wrong, for the person: an error's message, or what was thrown. */
export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
