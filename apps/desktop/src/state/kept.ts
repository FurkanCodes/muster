// The window's own state that outlives it (which lines of All sessions are
// folded, the theme): never the daemon's. In the app its Rust side keeps it
// (src-tauri/src/kept.rs), each change in a file before the call returns,
// so a ⌘Q or a crash loses none of it: the webview's own storage writes to
// disk when it likes. Read once as the window loads (`keepIn`), it is read
// here at once after. Mock builds and tests keep it in the page's storage.

/** Where the window's state is kept. */
export interface Keeper {
  get(key: string): string | null;
  /** Keeps `value` under `key`; null forgets it. */
  set(key: string, value: string | null): void;
}

/** The page's own storage; nothing kept where it is refused (a private window). */
const pageStorage: Keeper = {
  get(key) {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    } catch {
      // Storage refused: it lasts as long as the window.
    }
  },
};

let keeper: Keeper = pageStorage;

/** What the app's Rust side keeps (`TauriBridge`). */
export interface KeptInApp {
  kept(): Promise<Record<string, string>>;
  keep(key: string, value: string | null): Promise<void>;
}

/**
 * Keeps the window's state in the app from now on: what it holds is read
 * once, before the window draws, and each change is handed to it.
 */
export async function keepIn(app: KeptInApp): Promise<void> {
  const values = new Map(Object.entries(await app.kept()));
  keeper = {
    get: (key) => values.get(key) ?? null,
    set(key, value) {
      if (value === null) values.delete(key);
      else values.set(key, value);
      void app.keep(key, value).catch(() => {
        // Not written: it lasts as long as the window.
      });
    },
  };
}

/** What is kept under `key`, if anything. */
export function kept(key: string): string | null {
  return keeper.get(key);
}

/** Keeps `value` under `key`; null forgets it. */
export function keep(key: string, value: string | null): void {
  keeper.set(key, value);
}
