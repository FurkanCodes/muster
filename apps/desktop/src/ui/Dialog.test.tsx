import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { Button } from "./controls";
import { Dialog } from "./Dialog";

beforeAll(() => {
  // jsdom has no modal dialogs; WebKit's own is tested end to end (e2e/).
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

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
    render(<Close />);
    expect(screen.getByRole("dialog", { name: "Close #205 on GitHub?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep it open" })).toHaveFocus();
  });
  it("says what the question is about under its title", () => {
    render(
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
    render(<Close onClose={onClose} onAccept={onAccept} />);
    await userEvent.keyboard("{Enter}");
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onAccept).not.toHaveBeenCalled();
  });
  it("← → go between the answers and ⌘⏎ does the acting one from any", async () => {
    const onAccept = vi.fn();
    render(<Close onAccept={onAccept} />);
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
    render(<Close onClose={onClose} onAccept={onAccept} />);
    const text = screen.getByText("Everyone watching it is told.");
    fireEvent.keyDown(text, { key: "Enter" });
    expect(onAccept).toHaveBeenCalledTimes(1);
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
