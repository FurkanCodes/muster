import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { createElement } from "react";
import { afterEach, vi } from "vitest";

afterEach(() => cleanup());

// jsdom has no matchMedia; the app asks it for the system's theme.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

// xterm needs a canvas, which jsdom lacks: unit tests see a stand-in, and the
// real terminal is tested end to end in WebKit (e2e/).
vi.mock("../terminal/Terminal", () => ({
  Terminal: ({ session }: { session: string }) =>
    createElement("div", { "aria-label": "Terminal", "data-session": session }),
}));

// jsdom has no modal dialogs; WebKit's own is tested end to end (e2e/).
HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
  this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
  this.removeAttribute("open");
};
