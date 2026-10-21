import { describe, expect, it } from "vitest";
import { decodeFrame, encodeFrame, FRAME_EVENT, FRAME_OUTPUT, FRAME_REPLAY_RESET } from "./frames";

const bytes = (text: string) => new TextEncoder().encode(text);

describe("terminal frames", () => {
  it("carry output as bytes with its place in the stream", () => {
    const frame = decodeFrame(encodeFrame(FRAME_OUTPUT, 2 ** 40, bytes("hi")));
    if (frame.kind !== "output") throw new Error(`an output frame, not ${frame.kind}`);
    expect(frame.seq).toBe(2 ** 40);
    expect(new TextDecoder().decode(frame.data)).toBe("hi");
  });

  it("say when a replay must clear the screen", () => {
    const frame = decodeFrame(encodeFrame(FRAME_REPLAY_RESET, 0, bytes("x")));
    expect(frame.kind === "replay" && frame.reset).toBe(true);
  });

  it("carry events as JSON", () => {
    const event = { kind: "ended", reason: "exited" };
    const frame = decodeFrame(encodeFrame(FRAME_EVENT, 0, bytes(JSON.stringify(event))));
    expect(frame).toEqual({ kind: "event", event });
  });

  it("refuse a frame with no header", () => {
    expect(() => decodeFrame(new ArrayBuffer(3))).toThrow(/no header/);
  });
});
