import { act, fireEvent } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithApp } from "../test/render";
import { installKeys, KeysHeld, pressKey, useKeys } from "./keys";

let removeKeys: (() => void) | undefined;
afterEach(() => removeKeys?.());

const list = { x: vi.fn(), j: vi.fn() };
const flows = { n: vi.fn() };
const changes = { j: vi.fn(), "1": vi.fn() };

function List() {
  useKeys({ x: list.x, j: list.j });
  return null;
}

function FlowKeys() {
  useKeys({ n: flows.n }, { beneath: true });
  return null;
}

/** A banner inside the changes: its keys are added before its parent's. */
function Banner() {
  useKeys({ "1": changes["1"] });
  return null;
}

function Changes() {
  useKeys({ j: changes.j });
  return <Banner />;
}

function Pane() {
  const [open, setOpen] = useState(true);
  return (
    <>
      <List />
      <FlowKeys />
      {open ? (
        <KeysHeld>
          <Changes />
        </KeysHeld>
      ) : null}
      <button type="button" onClick={() => setOpen(false)}>
        Close
      </button>
    </>
  );
}

describe("ARC-24 a part holding the keys", () => {
  it("answers its own keys, and none reaches the list or the window beneath it", () => {
    const { app, getByRole } = renderWithApp(<Pane />);
    removeKeys = installKeys(app);
    for (const key of ["x", "j", "1", "n"]) fireEvent.keyDown(window, { key });
    expect(list.x).not.toHaveBeenCalled();
    expect(list.j).not.toHaveBeenCalled();
    expect(flows.n).not.toHaveBeenCalled();
    expect(changes.j).toHaveBeenCalledTimes(1);
    expect(changes["1"]).toHaveBeenCalledTimes(1);
    // The palette runs a key as it is pressed: held the same.
    expect(pressKey("x")).toBe(false);

    act(() => getByRole("button", { name: "Close" }).click());
    fireEvent.keyDown(window, { key: "x" });
    fireEvent.keyDown(window, { key: "n" });
    expect(list.x).toHaveBeenCalledTimes(1);
    expect(flows.n).toHaveBeenCalledTimes(1);
  });
});
