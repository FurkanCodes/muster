import type { Bridge } from "./bridge";
import { TauriBridge } from "./tauri";

/**
 * The real bridge in the app; the pretend daemon in mock builds and tests.
 * The mock is imported only when it is used, so the app never carries it
 * (bundle-conditional).
 */
export async function createBridge(): Promise<Bridge> {
  if (!__MOCK__) return new TauriBridge();
  const [{ MockBridge }, { workingDay }] = await Promise.all([
    import("./mock/bridge"),
    import("./mock/scenario"),
  ]);
  const bridge = new MockBridge(workingDay());
  // Tests, and a person poking at a mock build, drive the pretend daemon from here.
  (window as unknown as { __muster: typeof bridge }).__muster = bridge;
  return bridge;
}

export type { Bridge } from "./bridge";
export { BridgeFailure } from "./bridge";
