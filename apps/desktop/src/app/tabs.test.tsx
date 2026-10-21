import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

// The terminal UI's one tab model (PRD §6, §6a): Tab and ⇧Tab go round
// Sessions, Needs you, Tickets and Settings; ⏎ alone gives a session the
// keys and ⇧Q takes them back; the status bar says where the keys are. The
// real terminal's part (its input out of Tab's way, its focus) is e2e/tabs.spec.ts.

const heading = (name: string) => screen.findByRole("heading", { name, level: 1 });
const statusBar = () => screen.getByRole("contentinfo");
const typingIn = () => statusBar().querySelector("[data-typing]");

async function opened() {
  const view = renderApp();
  await screen.findByRole("option", { name: "Sketch the Tauri shell" });
  return view;
}

describe("ARC-24 the tabs, as the terminal UI's", () => {
  it("Tab goes round Sessions, Needs you, Tickets, Settings; ⇧Tab goes back", async () => {
    await opened();
    await userEvent.keyboard("{Tab}");
    expect(await heading("Tickets")).toBeInTheDocument();
    await userEvent.keyboard("{Tab}");
    expect(await heading("Settings")).toBeInTheDocument();
    await userEvent.keyboard("{Tab}");
    expect(await heading("Sessions")).toBeInTheDocument();
    await userEvent.keyboard("{Tab}");
    expect(await heading("Needs you")).toBeInTheDocument();
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(await heading("Sessions")).toBeInTheDocument();
  });

  it("1 to 4 and ⌘1 to ⌘4 go to a tab", async () => {
    await opened();
    await userEvent.keyboard("1");
    expect(await heading("Sessions")).toBeInTheDocument();
    await userEvent.keyboard("3");
    expect(await heading("Tickets")).toBeInTheDocument();
    await userEvent.keyboard("{Meta>}2{/Meta}");
    expect(await heading("Needs you")).toBeInTheDocument();
    await userEvent.keyboard("{Meta>}4{/Meta}");
    expect(await heading("Settings")).toBeInTheDocument();
  });

  it("Tab never gives a terminal the keys: only ⏎ or a click does", async () => {
    await opened();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(await heading("Needs you")).toBeInTheDocument();
    expect(typingIn()).toBeNull();
  });
});

describe("ARC-24 where the keys go, said for as long as it is so", () => {
  it("says a terminal has the keys while it has them, and stops the moment ⇧Q gives them back", async () => {
    await opened();
    await userEvent.keyboard("{Enter}");
    expect(typingIn()).toHaveTextContent(
      "Typing in Sketch the Tauri shell. Every key goes to Codex.",
    );
    await userEvent.keyboard("Q");
    expect(typingIn()).toBeNull();
    expect(statusBar()).not.toHaveTextContent("Typing in");
    // Elsewhere too: it never follows the person to another tab.
    await userEvent.keyboard("{Tab}");
    expect(await heading("Tickets")).toBeInTheDocument();
    expect(statusBar()).not.toHaveTextContent("Typing in");
  });
});
