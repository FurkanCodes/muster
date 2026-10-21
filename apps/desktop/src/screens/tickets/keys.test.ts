import { expect, it } from "vitest";
import { ticketChord, ticketKeyHint } from "./keys";

it("ARC-24 uses the person's daemon key bindings in useKeys and hints", () => {
  expect(ticketChord("ctrl-alt-r")).toBe("⌃⌥R");
  expect(ticketChord("shift-x")).toBe("X");
  expect(ticketChord("shift-tab")).toBe("⇧Tab");
  expect(ticketChord("alt-enter")).toBe("⌥Enter");
  expect(ticketKeyHint("ctrl-enter")).toBe("⌃⏎");
});
