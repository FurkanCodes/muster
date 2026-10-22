import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installKeys } from "../../app/keys";
import type { FileChange } from "../../ipc/generated/FileChange";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import ChangesView from "./ChangesView";
import { inSight, ROW } from "./FileList";

/** The list holds the focus and names `line` as its active one (ui/List: lines never take the focus). */
function expectActive(list: HTMLElement, line: HTMLElement) {
  expect(list).toHaveFocus();
  expect(line).toHaveAttribute("aria-selected", "true");
  expect(list).toHaveAttribute("aria-activedescendant", line.id);
}

const workspace = ulid(11);
let removeKeys: (() => void) | undefined;
beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.scrollTo = vi.fn();
});
afterEach(() => {
  removeKeys?.();
  vi.unstubAllGlobals();
});

/** `count` files not committed, each a line longer than the last. */
function manyFiles(count: number): FileChange[] {
  return Array.from({ length: count }, (_, n) => ({
    path: `src/file${String(n).padStart(4, "0")}.rs`,
    from: null,
    kind: "modified",
    lines: { added: n, removed: 1 },
  }));
}

function setup(files: FileChange[]) {
  const bridge = new MockBridge(workingDay());
  const changes = bridge.world.changes?.[workspace];
  if (changes) changes.uncommitted = files;
  const result = renderWithApp(<ChangesView workspace={workspace} />, bridge);
  removeKeys = installKeys(result.app);
  act(() => result.app.actions.showPane("changes"));
  return result;
}

describe("ARC-25 a long list of files", () => {
  it("draws a few dozen of 5,000 rows, each where it would be, and all of them counted", async () => {
    setup(manyFiles(5000));
    const files = await screen.findByRole("listbox", { name: "Files not committed" });
    const drawn = within(files).getAllByRole("option");
    expect(drawn.length).toBeLessThan(60);
    expect(drawn[0]).toHaveAccessibleName("modified: src/file0000.rs");
    // The spacer under the rows drawn holds the place of the rest.
    const under = files.lastElementChild as HTMLElement;
    expect(under.style.height).toBe(`${(5000 - drawn.length) * ROW}px`);
    expect(screen.getByRole("button", { name: "Not committed 5000" })).toBeVisible();
    expect(screen.getByText("in 5000 files").parentElement).toHaveTextContent(
      `+${(4999 * 5000) / 2}−5000in 5000 files`,
    );
  });

  it("End, Home and the pages move the selection anywhere, drawn and focused", async () => {
    setup(manyFiles(5000));
    const files = await screen.findByRole("listbox", { name: "Files not committed" });
    await userEvent.keyboard("{End}");
    const last = within(files).getByRole("option", { name: "modified: src/file4999.rs" });
    expectActive(files, last);
    expect(within(files).getAllByRole("option").length).toBeLessThan(60);
    expect(files.scrollTop).toBe(inSight(0, 640, 4999));
    await userEvent.keyboard("{PageUp}{PageUp}k");
    expectActive(files, within(files).getByRole("option", { name: "modified: src/file4978.rs" }));
    await userEvent.keyboard("{Home}");
    expectActive(files, within(files).getByRole("option", { name: "modified: src/file0000.rs" }));
    expect(files.scrollTop).toBe(0);
    await userEvent.keyboard("{PageDown}j{Enter}");
    expect(await screen.findByRole("region", { name: "Diff" })).toHaveFocus();
    expect(screen.getByText("src/").parentElement).toHaveTextContent("src/file0011.rs");
  });

  it("keeps a row in sight: scrolls up to it, down just far enough, and not at all when it is", () => {
    expect(inSight(0, 260, 3)).toBe(0);
    expect(inSight(0, 260, 10)).toBe(11 * ROW - 260);
    expect(inSight(20 * ROW, 260, 5)).toBe(5 * ROW);
    expect(inSight(20 * ROW, 260, -1)).toBe(20 * ROW);
  });
});
