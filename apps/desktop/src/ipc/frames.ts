// A terminal stream's frames, as the Rust side sends them on the attach
// channel: `[kind: u8][seq: u64 LE][payload]` (`contract.rs`, `frame`).
// Output stays bytes from the daemon to xterm: never JSON.

import type { TermEvent } from "./generated/desktop/TermEvent";

export const FRAME_OUTPUT = 0;
export const FRAME_REPLAY = 1;
export const FRAME_REPLAY_RESET = 2;
export const FRAME_SCREEN = 3;
export const FRAME_EVENT = 9;
const HEADER = 9;

export type TermFrame =
  | { kind: "output"; seq: number; data: Uint8Array }
  | { kind: "replay"; seq: number; data: Uint8Array; reset: boolean }
  // The program's screen after the replay before it, drawn whole over what
  // that drew; it takes no place in the output.
  | { kind: "screen"; data: Uint8Array }
  | { kind: "event"; event: TermEvent };

const utf8 = new TextDecoder();

/** Reads one frame. Throws on a frame too short to hold its header. */
export function decodeFrame(buffer: ArrayBuffer): TermFrame {
  if (buffer.byteLength < HEADER) {
    throw new Error(`a terminal frame of ${buffer.byteLength} bytes has no header`);
  }
  const view = new DataView(buffer);
  const kind = view.getUint8(0);
  const seq = Number(view.getBigUint64(1, true));
  const data = new Uint8Array(buffer, HEADER);
  switch (kind) {
    case FRAME_OUTPUT:
      return { kind: "output", seq, data };
    case FRAME_REPLAY:
    case FRAME_REPLAY_RESET:
      return { kind: "replay", seq, data, reset: kind === FRAME_REPLAY_RESET };
    case FRAME_SCREEN:
      return { kind: "screen", data };
    case FRAME_EVENT:
      return { kind: "event", event: JSON.parse(utf8.decode(data)) as TermEvent };
    default:
      throw new Error(`a terminal frame of unknown kind ${kind}`);
  }
}

/** Writes one frame; the mock bridge and tests use it. */
export function encodeFrame(kind: number, seq: number, payload: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(HEADER + payload.byteLength);
  const view = new DataView(buffer);
  view.setUint8(0, kind);
  view.setBigUint64(1, BigInt(seq), true);
  new Uint8Array(buffer, HEADER).set(payload);
  return buffer;
}
