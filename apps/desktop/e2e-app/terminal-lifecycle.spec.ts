import { browser, expect } from "@wdio/globals";
import { type Launch, show, startAgent } from "./support/agents";
import { muster, session, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { bareId, goTo, press, row, terminalText } from "./support/window";

// A session's terminal kept between its showings, in the real app over the
// real daemon (src/terminal/kept.ts, ARC-25): switching sessions makes each
// terminal once and brings it back current from where it was, the daemon
// starting over for one that fell off its ring; a WebGL context lost is
// made again, never while the window is hidden; and switching fast through
// more sessions than WebKit keeps contexts leaves the shown one drawing.

const SESSIONS = 18;
const launches: Record<string, Launch> = {};
const ids: Record<string, string> = {};
const title = (n: number) => `Kept ${String(n).padStart(2, "0")}`;

const made = (): Promise<Record<string, number>> =>
  browser.execute(() => ({
    ...(window as unknown as { __musterXtermsMade?: Record<string, number> }).__musterXtermsMade,
  }));

async function open(name: string): Promise<void> {
  await row(name).click();
  await browser.waitUntil(async () => (await row(name).getAttribute("aria-selected")) === "true", {
    timeoutMsg: `${name} was not selected`,
  });
}

/** How the shown terminal draws: its WebGL canvas, whether its context still holds. */
const drawing = (id: string) =>
  browser.execute((session: string) => {
    const xterm = (
      window as unknown as { __musterXterms?: Record<string, { element?: HTMLElement }> }
    ).__musterXterms?.[session];
    const canvases = [...(xterm?.element?.querySelectorAll("canvas") ?? [])].filter(
      (canvas) => ![...canvas.classList].some((name) => name.endsWith("-layer")),
    );
    const gl = canvases[0]?.getContext("webgl2");
    return {
      gpu: canvases.length,
      lost: gl ? gl.isContextLost() : null,
      connected: Boolean(xterm?.element?.isConnected),
    };
  }, id);

/** Loses the shown terminal's WebGL context, as a sleep or memory pressure would. */
const loseContext = (id: string) =>
  browser.execute((session: string) => {
    const xterm = (
      window as unknown as { __musterXterms?: Record<string, { element?: HTMLElement }> }
    ).__musterXterms?.[session];
    for (const canvas of xterm?.element?.querySelectorAll("canvas") ?? []) {
      if ([...canvas.classList].some((name) => name.endsWith("-layer"))) continue;
      canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
      (canvas as HTMLCanvasElement & { lostByTest?: boolean }).lostByTest = true;
    }
  }, id);

/** Whether the shown terminal draws with a canvas made since `loseContext`. */
const drawsAfresh = (id: string) =>
  browser.execute((session: string) => {
    const xterm = (
      window as unknown as { __musterXterms?: Record<string, { element?: HTMLElement }> }
    ).__musterXterms?.[session];
    return [...(xterm?.element?.querySelectorAll("canvas") ?? [])].some(
      (canvas) =>
        ![...canvas.classList].some((name) => name.endsWith("-layer")) &&
        !(canvas as HTMLCanvasElement & { lostByTest?: boolean }).lostByTest &&
        canvas.getContext("webgl2")?.isContextLost() === false,
    );
  }, id);

/** The window seen or not, as the app is told on ⌘Tab away and back. */
const seen = (visible: boolean) =>
  browser.execute(
    (state: string) => {
      Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    },
    visible ? "visible" : "hidden",
  );

describe("A session's terminal kept between its showings", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    muster("project", "add", repository("app"));
    for (let n = 1; n <= SESSIONS; n += 1) {
      launches[title(n)] = await startAgent("app", title(n));
      ids[title(n)] = bareId(session(title(n))?.id ?? "");
    }
    await goTo("Sessions");
    // The list draws the lines in view: the first and second are.
    for (const n of [1, 2]) await row(title(n)).waitForExist({ timeout: 20_000 });
  });

  it("ARC-25 makes each terminal once, and brings it back current from where it was", async () => {
    const [a, b] = [title(1), title(2)];
    await open(a);
    show(launches[a] as Launch, "\r\nfirst-in-a\r\n");
    await until(
      async () => (await terminalText(ids[a] ?? "")).includes("first-in-a"),
      "a's output",
    );
    await open(b);
    show(launches[a] as Launch, "\r\nwhile-a-was-away\r\n");
    await open(a);
    await until(
      async () => (await terminalText(ids[a] ?? "")).includes("while-a-was-away"),
      "what came while a was away",
    );
    const text = await terminalText(ids[a] ?? "");
    expect(text.split("first-in-a")).toHaveLength(2);
    expect(text.split("while-a-was-away")).toHaveLength(2);
    const counts = await made();
    expect(counts[ids[a] ?? ""]).toBe(1);
    expect(counts[ids[b] ?? ""]).toBe(1);
  });

  it("ARC-25 starts over cleanly where the session's output fell off the daemon's ring while away", async () => {
    const [a, b] = [title(1), title(2)];
    await open(b);
    // More than the ring's 1 MiB: what a's terminal holds is no longer the daemon's.
    show(launches[a] as Launch, `\r\n${"x".repeat(1100 * 1024)}\r\nafter-the-overrun\r\n`);
    await open(a);
    await until(
      async () => (await terminalText(ids[a] ?? "")).includes("after-the-overrun"),
      "the output after the overrun",
      30_000,
    );
    // Started over: what it held before is gone, not kept above the new.
    expect(await terminalText(ids[a] ?? "")).not.toContain("first-in-a");
    expect((await made())[ids[a] ?? ""]).toBe(1);
  });

  it("ARC-25 makes a lost WebGL context again, and not while the window is hidden", async () => {
    const a = title(1);
    await open(a);
    const id = ids[a] ?? "";
    await until(async () => (await drawing(id)).gpu > 0, "the terminal on the GPU");
    await loseContext(id);
    // xterm waits for the context to come back before it says it is lost.
    await until(() => drawsAfresh(id), "a context made again", 15_000);

    await seen(false);
    await loseContext(id);
    await browser.pause(6_000);
    expect(await drawsAfresh(id)).toBe(false);
    await seen(true);
    await until(() => drawsAfresh(id), "a context made again once seen", 15_000);
  });

  it("ARC-25 switching fast through more sessions than WebKit keeps contexts leaves the shown one drawing", async () => {
    const lostOnScreen = await browser.execute(() => {
      const holder = window as unknown as { __lostOnScreen?: number };
      holder.__lostOnScreen = 0;
      window.addEventListener(
        "webglcontextlost",
        (event) => {
          if ((event.target as Element | null)?.isConnected)
            holder.__lostOnScreen = (holder.__lostOnScreen ?? 0) + 1;
        },
        true,
      );
      return holder.__lostOnScreen;
    });
    expect(lostOnScreen).toBe(0);
    // As a person goes down the list fast: each line selected shows its session.
    await open(title(1));
    for (let round = 0; round < 2; round += 1) {
      await press(...Array.from({ length: SESSIONS + 2 }, () => "↓"));
      await press(...Array.from({ length: SESSIONS + 2 }, () => "↑"));
    }
    // Past the sessions the arrows reach the project's line: one in view is shown last.
    await open(title(2));
    const shownNow = await browser.execute(() => {
      const xterms = (
        window as unknown as { __musterXterms?: Record<string, { element?: HTMLElement }> }
      ).__musterXterms;
      return Object.entries(xterms ?? {})
        .filter(([, xterm]) => xterm.element?.isConnected)
        .map(([id]) => id);
    });
    expect(shownNow).toHaveLength(1);
    const last = shownNow[0] ?? "";
    await until(async () => (await drawing(last)).gpu > 0, "the one shown on the GPU");
    const now = await drawing(last);
    expect(now.connected).toBe(true);
    expect(now.lost).toBe(false);
    expect(
      await browser.execute(
        () => (window as unknown as { __lostOnScreen?: number }).__lostOnScreen,
      ),
    ).toBe(0);
  });
});
