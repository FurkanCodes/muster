import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { IconButton } from "./controls";
import { ActionMenu, type MenuItem } from "./Menu";

function items(): MenuItem[] {
  return [
    { key: "f", label: "Full screen", does: vi.fn() },
    { key: "⇧R", chords: ["R"], label: "Rename", does: vi.fn() },
    { key: "x", label: "Close", does: vi.fn(), danger: true },
  ];
}

function Opened({ list }: { list: MenuItem[] }) {
  return (
    <StrictMode>
      <div role="listbox" tabIndex={0} aria-label="Needs you sessions" />
      <ActionMenu
        label="Sketch the Tauri shell"
        items={list}
        trigger={<IconButton icon="more" label="More" />}
      />
      <button type="button">Elsewhere</button>
    </StrictMode>
  );
}

describe("ActionMenu", () => {
  it("ARC-24 opens with the focus on its first item, though React mounts it twice", async () => {
    render(<Opened list={items()} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    const menu = await screen.findByRole("menu", { name: "Sketch the Tauri shell" });
    await waitFor(() => expect(menu.contains(document.activeElement)).toBe(true));
  });

  it("ARC-24 does an item on its own key, and keeps the menu's keys from the screen behind", async () => {
    const list = items();
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    render(<Opened list={list} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await screen.findByRole("menu");
    await userEvent.keyboard("R");
    expect(list[1]?.does).toHaveBeenCalledOnce();
    expect(behind).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    window.removeEventListener("keydown", behind);
  });

  it("ARC-24 puts the destructive item last, after a separator", async () => {
    render(<Opened list={items()} />);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    const names = (await screen.findAllByRole("menuitem")).map((item) => item.textContent);
    expect(names).toEqual(["Full screenf", "Rename⇧R", "Closex"]);
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("ARC-24 closed by ⎋, gives the focus back to its button", async () => {
    render(<Opened list={items()} />);
    const more = screen.getByRole("button", { name: "More" });
    await userEvent.click(more);
    await screen.findByRole("menu");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(more).toHaveFocus();
  });

  it("ARC-24 opened at a point, hangs there, and gives the focus back to what had it", async () => {
    function AtAPoint() {
      const [at, setAt] = useState<{ x: number; y: number } | null>(null);
      return (
        <>
          <button type="button" onClick={() => setAt({ x: 120, y: 80 })}>
            Right-click
          </button>
          <div role="listbox" tabIndex={0} aria-label="Needs you sessions" />
          {at ? (
            <ActionMenu
              label="Sketch the Tauri shell"
              items={items()}
              at={at}
              onClose={() => setAt(null)}
            />
          ) : null}
        </>
      );
    }
    render(<AtAPoint />);
    const list = screen.getByRole("listbox");
    // The sidebar gives its list the focus as a right-click opens a line's menu.
    screen.getByRole("button", { name: "Right-click" }).click();
    list.focus();
    await screen.findByRole("menu", { name: "Sketch the Tauri shell" });
    const anchor = document.querySelector<HTMLElement>(".ui-menu-anchor");
    expect(anchor?.style.left).toBe("120px");
    expect(anchor?.style.top).toBe("80px");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(list).toHaveFocus();
  });
});
