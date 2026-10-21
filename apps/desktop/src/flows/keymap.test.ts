import { describe, expect, it } from "vitest";
import { defaultPreferences } from "../ipc/mock/scenario";
import { keysOf, labelOf } from "./keymap";

describe("ARC-24 the flows' keys are the person's, as the window's keymap reads them", () => {
  it("are Muster's where the person set none", () => {
    expect(keysOf(null, "rename").map((key) => key.chord)).toEqual(["R"]);
    expect(labelOf(null, "close")).toBe("x");
    expect(labelOf(null, "help")).toBe("h ?");
  });

  it("are the person's where they set them, and none where they took them away", () => {
    const preferences = { ...defaultPreferences(), keys: { rename: ["ctrl-e"], help: [] } };
    expect(keysOf(preferences, "rename").map((key) => key.chord)).toEqual(["⌃E"]);
    expect(labelOf(preferences, "help")).toBe("none");
  });
});
