import { describe, expect, it } from "vitest";
import { defaultPreferences } from "../ipc/mock/scenario";
import { keysOf, labelOf, parseKey } from "./keymap";

describe("ARC-24 the person's keys, as [keys] writes them", () => {
  it("names each as the key listener does, and shows it as a key cap", () => {
    expect(parseKey("R")).toEqual({ chord: "R", label: "R" });
    expect(parseKey("shift-r")).toEqual({ chord: "R", label: "R" });
    expect(parseKey("ctrl-e")).toEqual({ chord: "⌃E", label: "⌃e" });
    expect(parseKey("alt-x")).toEqual({ chord: "⌥X", label: "⌥x" });
    expect(parseKey("shift-tab")).toEqual({ chord: "⇧Tab", label: "⇧⇥" });
    expect(parseKey("esc")).toEqual({ chord: "Escape", label: "⎋" });
    expect(parseKey("backspace")).toEqual({ chord: "Backspace", label: "⌫" });
    expect(parseKey("enter")).toEqual({ chord: "Enter", label: "⏎" });
    expect(parseKey("space")).toEqual({ chord: " ", label: "space" });
    expect(parseKey("f5")).toEqual({ chord: "F5", label: "F5" });
    expect(parseKey("?")).toEqual({ chord: "?", label: "?" });
    expect(parseKey("not a key")).toBeNull();
    expect(parseKey("f25")).toBeNull();
  });

  it("are the person's where they set them, Muster's where not, and none where taken away", () => {
    const preferences = { ...defaultPreferences(), keys: { rename: ["ctrl-e"], help: [] } };
    expect(keysOf(preferences, "rename").map((key) => key.chord)).toEqual(["⌃E"]);
    expect(labelOf(preferences, "close")).toBe("x");
    expect(labelOf(null, "help")).toBe("h ?");
    expect(labelOf(preferences, "help")).toBe("none");
  });
});
