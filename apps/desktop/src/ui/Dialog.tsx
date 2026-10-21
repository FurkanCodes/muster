import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";
import { useApp } from "../app/context";
import "./dialog.css";

/**
 * A question Muster asks before it acts (design system: Dialog). A native
 * modal keeps the focus inside and the window behind it inert. ⏎ does the
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
function keepTab(event: KeyboardEvent<HTMLDialogElement>) {
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
  onKeyDown?: (event: KeyboardEvent<HTMLDialogElement>) => void;
  width?: "normal" | "wide";
}) {
  const app = useApp();
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const dialog = ref.current;
    const previous = document.activeElement;
    const keysWere = app.ui.getState().keysIn;
    app.actions.keysTo("dialog");
    dialog?.showModal();
    dialog?.querySelector<HTMLElement>("[data-dialog-initial]")?.focus();
    return () => {
      dialog?.close();
      if (app.ui.getState().keysIn === "dialog") app.actions.keysTo(keysWere);
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, [app]);
  return (
    <dialog
      ref={ref}
      className={`mu-dialog ui-dialog ${width === "wide" ? "ui-dialog--wide" : ""}`}
      aria-labelledby={id}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
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
        if (event.key !== "Enter" || (answer && !event.metaKey)) return;
        const area = event.target instanceof HTMLTextAreaElement;
        if (area && !event.metaKey && !event.ctrlKey) return;
        event.preventDefault();
        onAccept();
      }}
    >
      {meta ? (
        <div className="ui-dialog-head">
          <h2 className="mu-dialog-title" id={id}>
            {title}
          </h2>
          <span className="mu-meta">{meta}</span>
        </div>
      ) : (
        <h2 className="mu-dialog-title" id={id}>
          {title}
        </h2>
      )}
      {children}
      <div className="mu-dialog-actions">{actions}</div>
    </dialog>
  );
}
