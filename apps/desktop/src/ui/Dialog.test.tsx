import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { renderWithApp } from "../test/render";
import { Button } from "./controls";
import { Dialog } from "./Dialog";

function Close({ onClose = vi.fn(), onAccept = vi.fn() }) {
  return (
    <Dialog
      title="Close #205 on GitHub?"
      onClose={onClose}
      onAccept={onAccept}
      actions={
        <>
          <Button data-dialog-initial onClick={onClose}>
            Keep it open
          </Button>
          <Button variant="danger" onClick={onAccept}>
            Close ticket
          </Button>
        </>
      }
    >
      <p className="mu-dialog-text">Everyone watching it is told.</p>
    </Dialog>
  );
}

describe("ARC-24 Dialog", () => {
  it("is named by its question and starts on the answer marked first", () => {
    renderWithApp(<Close />);
    expect(screen.getByRole("dialog", { name: "Close #205 on GitHub?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep it open" })).toHaveFocus();
  });
  it("says what the question is about under its title", () => {
    renderWithApp(
      <Dialog
        title="Assign an agent to #217"
        meta="Tickets from YouTrack"
        onClose={() => {}}
        onAccept={() => {}}
        actions={null}
      />,
    );
    expect(screen.getByRole("dialog", { name: "Assign an agent to #217" })).toHaveTextContent(
      "Tickets from YouTrack",
    );
  });
  it("PRD-01 ⏎ on a focused answer does that answer, never the acting one", async () => {
    const onClose = vi.fn();
    const onAccept = vi.fn();
    renderWithApp(<Close onClose={onClose} onAccept={onAccept} />);
    await userEvent.keyboard("{Enter}");
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onAccept).not.toHaveBeenCalled();
  });
  it("← → go between the answers and ⌘⏎ does the acting one from any", async () => {
    const onAccept = vi.fn();
    renderWithApp(<Close onAccept={onAccept} />);
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("button", { name: "Close ticket" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}{ArrowLeft}");
    expect(screen.getByRole("button", { name: "Keep it open" })).toHaveFocus();
    await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
    expect(onAccept).toHaveBeenCalledTimes(1);
  });
  it("⏎ elsewhere in it does the acting answer and ⎋ the other", () => {
    const onClose = vi.fn();
    const onAccept = vi.fn();
    renderWithApp(<Close onClose={onClose} onAccept={onAccept} />);
    const text = screen.getByText("Everyone watching it is told.");
    fireEvent.keyDown(text, { key: "Enter" });
    expect(onAccept).toHaveBeenCalledTimes(1);
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
  it("ARC-24 has the window's keys while open, and gives them back as it closes", () => {
    function Asked() {
      const [open, setOpen] = useState(true);
      return open ? <Close onClose={vi.fn(() => setOpen(false))} /> : null;
    }
    const { app } = renderWithApp(<Asked />);
    expect(app.ui.getState().keysIn).toBe("dialog");
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(app.ui.getState().keysIn).toBe("list");
  });
  it("PRD-03 ⏎ in a text area is a new line; ⌘⏎ there does the acting answer", () => {
    const onAccept = vi.fn();
    renderWithApp(
      <Dialog
        title="New session"
        onClose={vi.fn()}
        onAccept={onAccept}
        actions={<Button>Start</Button>}
      >
        <textarea aria-label="First prompt" />
      </Dialog>,
    );
    const prompt = screen.getByRole("textbox", { name: "First prompt" });
    fireEvent.keyDown(prompt, { key: "Enter" });
    expect(onAccept).not.toHaveBeenCalled();
    fireEvent.keyDown(prompt, { key: "Enter", metaKey: true });
    expect(onAccept).toHaveBeenCalledTimes(1);
  });
  it("⇥ past the last goes round to the first, and ⇧⇥ the other way", () => {
    renderWithApp(<Close />);
    const keep = screen.getByRole("button", { name: "Keep it open" });
    const close = screen.getByRole("button", { name: "Close ticket" });
    close.focus();
    fireEvent.keyDown(close, { key: "Tab" });
    expect(keep).toHaveFocus();
    fireEvent.keyDown(keep, { key: "Tab", shiftKey: true });
    expect(close).toHaveFocus();
  });
});
