import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AgentChoice } from "./AgentChoice";

const agents = [
  { id: "codex", name: "Codex" },
  { id: "claude", name: "Claude Code" },
];

describe("ARC-24 AgentChoice", () => {
  it("is a radio group, the chosen one checked and the last chosen noted", () => {
    render(<AgentChoice agents={agents} value="codex" last="codex" onChange={() => {}} />);
    expect(screen.getByRole("radiogroup", { name: "Agent" })).toBeInTheDocument();
    const codex = screen.getByRole("radio", { name: /Codex/ });
    expect(codex).toHaveAttribute("aria-checked", "true");
    expect(codex).toHaveTextContent("last chosen");
    expect(screen.getByRole("radio", { name: "Claude Code" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });
  it("chooses an agent on a click", async () => {
    const onChange = vi.fn();
    render(<AgentChoice agents={agents} value="codex" last="codex" onChange={onChange} />);
    await userEvent.click(screen.getByRole("radio", { name: "Claude Code" }));
    expect(onChange).toHaveBeenCalledWith("claude");
  });
});
