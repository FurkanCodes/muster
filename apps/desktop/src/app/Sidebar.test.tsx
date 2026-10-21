import { act, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

// The agents at work in the working day: Claude Code twice, OpenCode, Gemini CLI.
const WORKING = [100, 103, 201, 400].map(ulid);

describe("ARC-25 the sidebar", () => {
  it("spins Working's mark only while an agent works", async () => {
    const { bridge } = renderApp();
    const working = await screen.findByRole("button", { name: /Working/ });
    const spinner = () => working.querySelector(".mu-spin");
    expect(spinner()).not.toHaveClass("mu-spin--still");
    act(() => {
      for (const session of WORKING) bridge.setStatus(session, { state: "finished" });
    });
    expect(spinner()).toHaveClass("mu-spin--still");
    act(() => bridge.setStatus(WORKING[0] ?? "", { state: "running" }));
    expect(spinner()).not.toHaveClass("mu-spin--still");
  });
});
