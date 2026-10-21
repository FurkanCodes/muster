import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { TextField } from "./TextField";

function Named({ error = null }: { error?: string | null }) {
  const [name, setName] = useState("Sketch");
  return (
    <TextField
      label="Session name"
      value={name}
      onChange={setName}
      hint="⏎ renames it"
      error={error}
    />
  );
}

describe("ARC-24 TextField", () => {
  it("is named by its label and says its hint", async () => {
    render(<Named />);
    const field = screen.getByRole("textbox", { name: "Session name" });
    expect(field).toHaveAccessibleDescription("⏎ renames it");
    await userEvent.type(field, " the shell");
    expect(field).toHaveValue("Sketch the shell");
  });

  it("says what went wrong in place of the hint", () => {
    render(<Named error="a session's name cannot be empty" />);
    expect(screen.getByRole("textbox")).toBeInvalid();
    expect(screen.getByRole("alert")).toHaveTextContent("a session's name cannot be empty");
  });
});
