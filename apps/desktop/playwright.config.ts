import { createHash } from "node:crypto";
import { defineConfig, devices } from "@playwright/test";

// Each worktree gets a port of its own, so agents testing branches side by
// side never reach each other's build. E2E_PORT overrides it.
const port = Number(
  process.env.E2E_PORT ??
    1421 +
      (Number.parseInt(createHash("sha1").update(process.cwd()).digest("hex").slice(0, 4), 16) %
        500),
);

// The window, end to end, in WebKit (the engine of the Mac's webview), over a
// mock build: the pretend daemon in the page, driven through
// `window.__muster` (src/ipc/mock/bridge.ts). The real app with a real daemon
// is `pnpm e2e:app` (WebdriverIO).
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://localhost:${port}`,
    trace: "retain-on-failure",
    colorScheme: "dark",
  },
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 1440, height: 900 } },
    },
  ],
  webServer: {
    command: `pnpm build:mock && pnpm exec vite preview --mode mock --outDir dist-mock --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
