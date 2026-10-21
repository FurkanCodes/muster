import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useMediaQuery } from "./useMediaQuery";

function Narrow() {
  return <span>{useMediaQuery("(max-width: 1100px)") ? "narrow" : "wide"}</span>;
}

describe("ARC-25 useMediaQuery", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("follows the breakpoint as it is crossed, and stops listening when gone", () => {
    let matches = false;
    const listeners = new Set<() => void>();
    vi.stubGlobal("matchMedia", (query: string) => ({
      media: query,
      get matches() {
        return matches;
      },
      addEventListener: (_: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
    }));
    const view = render(<Narrow />);
    expect(screen.getByText("wide")).toBeInTheDocument();
    act(() => {
      matches = true;
      for (const listener of listeners) listener();
    });
    expect(screen.getByText("narrow")).toBeInTheDocument();
    view.unmount();
    expect(listeners.size).toBe(0);
  });
});
