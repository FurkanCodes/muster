import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";
import "./dialog.css";

/**
 * A question Muster asks before it acts (design system: Dialog). A native
 * modal keeps the focus inside and the window behind it inert. ⏎ does the
 * acting answer and ⎋ the other, but ⏎ on a focused answer does that one,
 * so a danger dialog, whose focus starts on the answer that does nothing,
 * never acts on a ⏎ meant for it; ← → go between the answers (⇥ reaches no
 * button in WebKit unless the person says so) and ⌘⏎ does the acting
 * answer wherever the focus is. The button that has the focus first
 * carries `data-dialog-initial`.
 */
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
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const dialog = ref.current;
    const previous = document.activeElement;
    dialog?.showModal();
    dialog?.querySelector<HTMLElement>("[data-dialog-initial]")?.focus();
    return () => {
      dialog?.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
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
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
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
