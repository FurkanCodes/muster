import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Select } from "./Select";

const options = [
  { value: "", label: "⎇ main", note: "the base" },
  { value: "w1", label: "⎇ muster/issue-214-desktop-app" },
  { value: "w2", label: "⎇ fix/resize-under-load" },
];

function Chosen() {
  const [value, setValue] = useState("");
  return (
    <>
      <Select label="Made on top of" options={options} value={value} onChange={setValue} />
      <output>{value || "base"}</output>
    </>
  );
}

describe("ARC-24 Select", () => {
  it("shows the value and its note, and ← → step through without opening", async () => {
    render(<Chosen />);
    const select = screen.getByRole("button", { name: "Made on top of" });
    expect(select).toHaveTextContent("⎇ mainthe base");
    select.focus();
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    expect(screen.getByRole("status")).toHaveTextContent("w2");
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("status")).toHaveTextContent("base");
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("status")).toHaveTextContent("w2");
    expect(select).toHaveAttribute("aria-expanded", "false");
  });
  it("opens a listbox on ⏎, ↑↓ move, ⏎ chooses and ⎋ closes", async () => {
    render(<Chosen />);
    screen.getByRole("button", { name: "Made on top of" }).focus();
    await userEvent.keyboard("{Enter}");
    const list = screen.getByRole("listbox", { name: "Made on top of" });
    expect(screen.getByRole("option", { name: /main/ })).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(list).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("w1");
    await userEvent.keyboard("{Enter}");
    await userEvent.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("w1");
    expect(screen.getByRole("button", { name: "Made on top of" })).toHaveFocus();
  });
});
