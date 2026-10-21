import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";
import "./dialog.css";

/**
 * A question Muster asks before it acts (design system: Dialog). A native
 * modal keeps the focus inside and the window behind it inert. ⏎ does the
 * acting answer, ⎋ the other; ⏎ on a focused answer does that one, so a
 * danger dialog, whose focus starts on the answer that does nothing, never
 * acts on a ⏎ meant for it; in a text area ⏎ is a new line and ⌘⏎ the
 * acting answer. ⇥ and ⇧⇥ go round what is in it and never leave it. The
 * button that has the focus first carries `data-dialog-initial`.
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
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        if (event.key === "Tab") {
          keepTab(event);
          return;
        }
        if (event.key !== "Enter") return;
        const target = event.target instanceof HTMLElement ? event.target : null;
        if (target?.closest(".mu-dialog-actions button")) return;
        if (target instanceof HTMLTextAreaElement && !event.metaKey && !event.ctrlKey) return;
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
