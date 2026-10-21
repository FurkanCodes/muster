import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { BridgeFailure } from "./bridge";
import { TauriBridge } from "./tauri";

afterEach(() => clearMocks());

describe("the real bridge, over Tauri's IPC", () => {
  it("asks the Rust side to play a sound by its name, and passes its refusal on", async () => {
    const asked: unknown[] = [];
    mockIPC((command, args) => {
      asked.push([command, args]);
      if ((args as { sound?: string }).sound === "../x") {
        throw { kind: "invalid", message: '"../x" is not a sound' };
      }
      return null;
    });
    const bridge = new TauriBridge();
    await bridge.hear("Glass");
    expect(asked).toEqual([["hear", { sound: "Glass" }]]);
    await expect(bridge.hear("../x")).rejects.toEqual(
      new BridgeFailure("invalid", '"../x" is not a sound'),
    );
  });

  it("says whether the window is seen as it says whether it is looked at", async () => {
    const asked: unknown[] = [];
    mockIPC((command, args) => {
      asked.push([command, args]);
      return null;
    });
    await new TauriBridge().looking(false, null, false);
    expect(asked).toEqual([["looking", { looking: false, focused: null, seen: false }]]);
  });
});
