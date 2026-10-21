import { expect, test } from "@playwright/test";
import tauri from "../src-tauri/tauri.conf.json" with { type: "json" };

// The window under the app's own security settings (tauri.conf.json): its
// content security policy, and a frozen Object.prototype when the app asks
// for one. A setting the page cannot live with leaves the real window blank,
// which no other test would see (ARC-24).

const security: { csp: Record<string, string>; freezePrototype?: boolean } = tauri.app.security;
const policy = Object.entries(security.csp)
  .map(([directive, sources]) => `${directive} ${sources}`)
  .join("; ");

test("the window draws under the app's own security settings", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/*", async (route) => {
    if (route.request().resourceType() !== "document") return route.continue();
    const response = await route.fetch();
    await route.fulfill({
      response,
      headers: { ...response.headers(), "content-security-policy": policy },
    });
  });
  if (security.freezePrototype) await page.addInitScript(() => Object.freeze(Object.prototype));

  await page.goto("/");
  await expect(page.getByRole("navigation", { name: "Screens" })).toBeVisible();
  await expect(page.getByLabel("Terminal").first()).toBeVisible();
  expect(errors).toEqual([]);
});
