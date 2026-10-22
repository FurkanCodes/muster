import { Dialog as Base } from "@base-ui/react/dialog";
import { type KeyboardEvent, type ReactNode, useEffect, useRef } from "react";
import { useApp } from "../app/context";
import { listTakesEscape } from "./layers";
import { type PartProps, usePart } from "./part";
import { portalRoot } from "./portal";
import "./dialog.css";

/**
 * What floats over the window while it asks or offers (a Dialog, the
 * palette), on Base UI's dialog: modal, so the focus stays inside and the
 * window behind is inert, over the scrim, closed by ⎋ but never by a click
 * outside. The focus starts on what `initial` finds inside, and goes back
 * to what had it as it closes.
 */
export function Modal({
  className,
  label,
  initial,
  onClose,
  onKeyDown,
  children,
}: {
  className: string;
  /** Its name, where no title of its own names it. */
  label?: string;
  /** Where the focus starts, as a selector inside it. */
  initial: string;
  onClose: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
  children: ReactNode;
}) {
  const popup = useRef<HTMLDivElement | null>(null);
  // The focus goes in as it is drawn, not a frame later: a key pressed at
  // once is the Modal's. Once only, though its ref may be set again; Base
  // UI's own initial focus lands on the same.
  const focused = useRef(false);
  const drawn = (node: HTMLDivElement | null) => {
    popup.current = node;
    if (!node || focused.current) return;
    focused.current = true;
    node.querySelector<HTMLElement>(initial)?.focus();
  };
  useEffect(() => {
    const previous = document.activeElement;
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  return (
    <Base.Root
      open
      onOpenChange={(open, details) => {
        // ⎋ in a list that floats over it (a Select's) closes that list, not this.
        if (details.reason === "escape-key" && listTakesEscape()) {
          details.cancel();
          return;
        }
        if (!open) onClose();
      }}
      disablePointerDismissal
    >
      <Base.Portal container={portalRoot()}>
        <Base.Backdrop className="ui-modal-backdrop" />
        <Base.Popup
          ref={drawn}
          className={className}
          aria-label={label}
          initialFocus={() => popup.current?.querySelector<HTMLElement>(initial) ?? true}
          finalFocus={false}
          onKeyDown={onKeyDown}
        >
          {children}
        </Base.Popup>
      </Base.Portal>
    </Base.Root>
  );
}

/**
 * A question Muster asks before it acts (design system: Dialog), a Modal
 * that keeps the focus inside and the window behind it inert. ⏎ does the
 * acting answer and ⎋ the other, but ⏎ on a focused answer does that one,
 * so a danger dialog, whose focus starts on the answer that does nothing,
 * never acts on a ⏎ meant for it; ← → go between the answers (⇥ reaches no
 * button in WebKit unless the person says so) and ⌘⏎ does the acting
 * answer wherever the focus is; in a text area ⏎ is a new line. ⇥ past
 * the last field goes round to the first, ⇧⇥ the other way, so the keys
 * never leave it. The button that has the focus first carries
 * `data-dialog-initial`. While it is open the window's keys are the
 * dialog's, so no screen behind it acts on them, and a key pressed in it
 * never reaches the window's listener.
 */
const FOCUSABLE =
  "button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex='-1'])";

/** ⇥ past the last thing in the dialog goes round to the first, ⇧⇥ the other way. */
function keepTab(event: KeyboardEvent<HTMLDivElement>) {
  const inside = [...event.currentTarget.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => element.tabIndex >= 0,
  );
  const first = inside[0];
  const last = inside.at(-1);
  const at = document.activeElement;
  if (event.shiftKey && (at === first || at === event.currentTarget)) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && at === last) {
    event.preventDefault();
    first?.focus();
  }
}

export function Dialog({
  title,
  meta,
  children,
  actions,
  onClose,
  onAccept,
  onKeyDown,
  width = "normal",
}: {
  title: string;
  /** A line under the title, as what the question is about. */
  meta?: ReactNode;
  children?: ReactNode;
  actions: ReactNode;
  onClose: () => void;
  onAccept: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
  width?: "normal" | "wide";
}) {
  const app = useApp();
  useEffect(() => {
    const keysWere = app.ui.getState().keysIn;
    app.actions.keysTo("dialog");
    return () => {
      if (app.ui.getState().keysIn === "dialog") app.actions.keysTo(keysWere);
    };
  }, [app]);
  return (
    <Modal
      className={`mu-dialog ui-dialog ${width === "wide" ? "ui-dialog--wide" : ""}`}
      initial="[data-dialog-initial]"
      onClose={onClose}
      onKeyDown={(event) => {
        // Keys in a modal are its own: the window's listener (app/keys.ts)
        // never sees them, so it neither acts on them nor clears a note an
        // answer has just put in the status bar.
        event.stopPropagation();
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        if (event.key === "Tab") {
          keepTab(event);
          return;
        }
        const answer =
          event.target instanceof HTMLElement
            ? event.target.closest<HTMLButtonElement>(".mu-dialog-actions button")
            : null;
        if (answer && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
          event.preventDefault();
          const answers = [
            ...(answer
              .closest(".mu-dialog-actions")
              ?.querySelectorAll<HTMLButtonElement>("button:enabled") ?? []),
          ];
          const at = answers.indexOf(answer) + (event.key === "ArrowLeft" ? -1 : 1);
          answers[Math.max(0, Math.min(answers.length - 1, at))]?.focus();
          return;
        }
        if (event.key !== "Enter") return;
        // ⏎ on an answer gives that answer, as the browser's own press of a
        // button would: whatever made the key, a hand or a script.
        if (answer && !event.metaKey) {
          event.preventDefault();
          answer.click();
          return;
        }
        const area = event.target instanceof HTMLTextAreaElement;
        if (area && !event.metaKey && !event.ctrlKey) return;
        event.preventDefault();
        onAccept();
      }}
    >
      {meta ? (
        <div className="ui-dialog-head">
          <Base.Title className="mu-dialog-title">{title}</Base.Title>
          <span className="mu-meta">{meta}</span>
        </div>
      ) : (
        <Base.Title className="mu-dialog-title">{title}</Base.Title>
      )}
      {children}
      <div className="mu-dialog-actions">{actions}</div>
    </Modal>
  );
}

/** A dialog's running text: what it asks, what follows from it; `busy`, what it is waiting for. */
export function DialogText({ busy = false, ...props }: PartProps<"p"> & { busy?: boolean }) {
  return usePart("p", props, {
    className: "mu-dialog-text",
    role: busy ? "status" : undefined,
    "aria-busy": busy || undefined,
  });
}
