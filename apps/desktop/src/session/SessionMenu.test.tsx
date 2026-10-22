import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { type MenuItem, SessionMenu } from "./SessionMenu";

const ITEMS: MenuItem[] = [
  { key: "f", label: "Full screen", does: vi.fn() },
  { key: "x", label: "Close", does: vi.fn(), danger: true },
];

/** A list that holds the keys, and the menu opened from it. */
function Opened({ at }: { at?: { x: number; y: number } }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <div role="listbox" tabIndex={0} aria-label="Needs you sessions" />
      <button type="button" onClick={() => setOpen(true)}>
        Open
      </button>
      <button type="button">Elsewhere</button>
      {open ? (
        <SessionMenu
          label="Sketch the Tauri shell"
          items={ITEMS}
          at={at}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  );
}

async function openFromList(at?: { x: number; y: number }) {
  render(
    <StrictMode>
      <Opened at={at} />
    </StrictMode>,
  );
  screen.getByRole("listbox").focus();
  // Opened as a key or a right-click would, the list keeping the keys meanwhile.
  screen.getByRole("button", { name: "Open" }).click();
  screen.getByRole("listbox").focus();
}

describe("SessionMenu", () => {
  it("ARC-24 stays open with the focus on its first item, though React mounts it twice", async () => {
    render(
      <StrictMode>
        <Opened />
      </StrictMode>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /^Full screen/ })).toHaveFocus();
  });

  it("ARC-24 gives the focus back to what had it when ⎋ closes it", async () => {
    render(
      <StrictMode>
        <Opened />
      </StrictMode>,
    );
    const list = screen.getByRole("listbox");
    list.focus();
    await userEvent.keyboard("{Tab}{Enter}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open" })).toHaveFocus();
  });

  it("ARC-24 closed by a click elsewhere, leaves the focus where the click put it", async () => {
    await openFromList();
    await userEvent.click(screen.getByRole("button", { name: "Elsewhere" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Elsewhere" })).toHaveFocus();
  });

  it("ARC-24 opened at a point, stands there inside the window", async () => {
    await openFromList({ x: 120, y: 80 });
    const menu = await screen.findByRole("menu");
    expect(menu).toHaveClass("session-menu--at");
    expect(menu.style.left).toBe("120px");
    expect(menu.style.top).toBe("80px");
  });
});
