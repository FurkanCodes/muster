/// <reference types="vite/client" />

/** True in mock builds (`--mode mock`) and tests: the pretend daemon is used. */
declare const __MOCK__: boolean;

/** True in the app built for its end-to-end tests (`pnpm e2e:app`), never in one shipped. */
declare const __E2E__: boolean;
