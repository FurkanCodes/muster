import { expect, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// ARC-25: a window with nothing at work draws nothing. Over the mock daemon
// (window.__muster).

declare global {
  interface Window {
    __muster: MockBridge;
  }
}

/** The animations running in the page now. */
const running = () =>
  document.getAnimations().filter((animation) => animation.playState === "running").length;

test("ARC-25 with no agent at work, nothing in the window moves", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("img", { name: /4 working/ })).toBeVisible();
  expect(await page.evaluate(running)).toBeGreaterThan(0);
  await page.evaluate(() => {
    for (const n of [100, 103, 201, 400]) {
      window.__muster.setStatus(`01JD0000000000000000000${n}`, { state: "finished" });
    }
  });
  await expect(page.getByRole("img", { name: /0 working/ })).toBeVisible();
  // What a finish flashes plays once; then nothing runs.
  await expect.poll(() => page.evaluate(running), { timeout: 5_000 }).toBe(0);
});
