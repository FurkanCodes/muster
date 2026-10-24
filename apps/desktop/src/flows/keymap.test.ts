import { describe, expect, it } from "vitest";
import { defaultPreferences } from "../ipc/mock/scenario";
import { keysOf, labelOf } from "./keymap";

/** No keys set, and no plugin. */
const NOTHING_SET = { preferences: null, plugins: [] };

describe("ARC-24 the flows' keys are the person's, as the window's keymap reads them", () => {
  it("are Muster's where the person set none", () => {
    expect(keysOf(NOTHING_SET, "rename").map((key) => key.chord)).toEqual(["R"]);
    expect(labelOf(NOTHING_SET, "close")).toBe("x");
    expect(labelOf(NOTHING_SET, "help")).toBe("h ?");
  });

  it("are the person's where they set them, and none where they took them away", () => {
    const preferences = { ...defaultPreferences(), keys: { rename: ["ctrl-e"], help: [] } };
    expect(keysOf({ preferences, plugins: [] }, "rename").map((key) => key.chord)).toEqual(["⌃E"]);
    expect(labelOf({ preferences, plugins: [] }, "help")).toBe("none");
  });
});
