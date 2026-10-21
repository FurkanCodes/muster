import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";
import "./dialog.css";

/**
 * A question Muster asks before it acts (design system: Dialog). A native
 * modal keeps the focus inside and the window behind it inert. ⏎ does the
 * acting answer, ⎋ the other; ⏎ on a focused answer does that one, so a
 * danger dialog, whose focus starts on the answer that does nothing, never
 * acts on a ⏎ meant for it. Keys pressed in it stay in it. The button that
 * has the focus first carries `data-dialog-initial`.
 */
export function Dialog({
  title,
  children,
  actions,
  onClose,
  onAccept,
  onKeyDown,
  width = "normal",
}: {
  title: string;
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
        // Keys in a modal are its own: the window's listener (app/keys.ts)
        // never sees them, so it neither acts on them nor clears a note an
        // answer has just put in the status bar.
        event.stopPropagation();
        onKeyDown?.(event);
        if (event.defaultPrevented || event.key !== "Enter") return;
        const answer =
          event.target instanceof HTMLElement && event.target.closest(".mu-dialog-actions button");
        if (answer) return;
        event.preventDefault();
        onAccept();
      }}
    >
      <h2 className="mu-dialog-title" id={id}>
        {title}
      </h2>
      {children}
      <div className="mu-dialog-actions">{actions}</div>
    </dialog>
  );
}
