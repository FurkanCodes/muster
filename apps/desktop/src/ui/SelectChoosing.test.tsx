import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Select, type SelectOption } from "./Select";

// What Settings asks more of the shared Select: a row's size, a sound heard
// as the keys move to it, the TUI's own stepping, and a line between groups.

const SOUNDS: SelectOption[] = [
  { value: "none", label: "None" },
  { value: "default", label: "The Mac's default" },
  { value: "Basso", label: "Basso", apart: true },
  { value: "Blow", label: "Blow" },
];

function Sounds({
  onActive,
  onStep,
}: {
  onActive?: (v: string) => void;
  onStep?: (by: number) => void;
}) {
  const [value, setValue] = useState("Basso");
  return (
    <Select
      label="Sound"
      size="md"
      options={SOUNDS}
      value={value}
      onChange={setValue}
      onActive={onActive}
      onStep={onStep}
    />
  );
}

describe("Select, as Settings uses it", () => {
  it("is a row's size, and sets the Mac's own sounds apart from the others", async () => {
    render(<Sounds />);
    const button = screen.getByRole("button", { name: "Sound" });
    expect(button).not.toHaveClass("mu-select--lg");
    await userEvent.click(button);
    expect(screen.getAllByRole("separator")).toHaveLength(1);
    expect(screen.getByRole("option", { name: /Basso/ })).toHaveAttribute("aria-selected", "true");
  });

  it("says each option the keys move to before it is chosen", async () => {
    const onActive = vi.fn();
    render(<Sounds onActive={onActive} />);
    await userEvent.click(screen.getByRole("button", { name: "Sound" }));
    await userEvent.keyboard("{ArrowDown}");
    expect(onActive).toHaveBeenLastCalledWith("Blow");
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Sound" })).toHaveTextContent("Blow");
  });

  it("steps the caller's way on ← →", async () => {
    const onStep = vi.fn();
    render(<Sounds onStep={onStep} />);
    screen.getByRole("button", { name: "Sound" }).focus();
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    expect(onStep.mock.calls).toEqual([[-1], [1]]);
    expect(screen.getByRole("button", { name: "Sound" })).toHaveTextContent("Basso");
  });
});
