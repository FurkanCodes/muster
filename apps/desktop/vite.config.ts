import babel from "@rolldown/plugin-babel";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The webview is WebKit on a Mac: build for it, small and without maps. The
// React Compiler memoises every component, so a status change re-renders
// only what reads it (`ARC-25`).
export default defineConfig(({ mode }) => ({
  plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  define: {
    __MOCK__: JSON.stringify(mode === "mock" || mode === "test"),
    // The app as `pnpm e2e:app` builds it, for its end-to-end tests only.
    __E2E__: JSON.stringify(process.env.MUSTER_E2E_BUILD === "1"),
  },
  build: {
    target: "safari16",
    sourcemap: false,
    cssCodeSplit: true,
    reportCompressedSize: false,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
}));
