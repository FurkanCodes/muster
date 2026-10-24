import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAsked } from "./useAsked";

/** A promise answered by hand. */
function later<T>() {
  let answer!: (value: T) => void;
  let refuse!: (error: Error) => void;
  const promise = new Promise<T>((resolve, reject) => {
    answer = resolve;
    refuse = reject;
  });
  return { promise, answer, refuse };
}

describe("asking the daemon from a screen", () => {
  it("answers what is asked, and drops an answer to what is no longer asked", async () => {
    const asked = { a: later<string>(), b: later<string>() };
    const { result, rerender } = renderHook(
      ({ key }: { key: "a" | "b" }) => useAsked(() => asked[key].promise, key),
      { initialProps: { key: "a" as "a" | "b" } },
    );
    expect(result.current).toEqual({ state: "asking" });
    rerender({ key: "b" });
    // The first answer comes late: it is not what is asked now.
    await act(async () => asked.a.answer("for a"));
    expect(result.current).toEqual({ state: "asking" });
    await act(async () => asked.b.answer("for b"));
    expect(result.current).toEqual({ state: "answered", value: "for b" });
  });

  it("says why it failed, and asks nothing while there is nothing to ask", async () => {
    const asked = later<string>();
    let asks = 0;
    const { result, rerender } = renderHook(
      ({ on }: { on: boolean }) =>
        useAsked(
          on
            ? () => {
                asks += 1;
                return asked.promise;
              }
            : null,
          "it",
        ),
      { initialProps: { on: false } },
    );
    expect(asks).toBe(0);
    rerender({ on: true });
    expect(asks).toBe(1);
    await act(async () => asked.refuse(new Error("The daemon refused it.")));
    expect(result.current).toEqual({ state: "failed", error: "The daemon refused it." });
  });
});
