import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";
import "./Dialog.css";

/** Native modal focus containment and focus return, with the design system's skin. */
export function Dialog({
  title,
  children,
  onCancel,
  onKeyDown,
  focusKey,
}: {
  title: string;
  children: ReactNode;
  onCancel(): void;
  onKeyDown?(event: KeyboardEvent<HTMLDialogElement>): void;
  focusKey?: number;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    dialog.showModal();
    dialog.querySelector<HTMLElement>("[data-initial-focus]")?.focus();
    return () => dialog.close();
  }, []);
  useEffect(() => {
    if (focusKey !== undefined)
      ref.current?.querySelector<HTMLElement>("[data-initial-focus]")?.focus();
  }, [focusKey]);
  return (
    <dialog
      ref={ref}
      aria-labelledby={id}
      aria-modal="true"
      className="mu-dialog app-dialog"
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
      onKeyDown={onKeyDown}
    >
      <h2 className="mu-dialog-title" id={id}>
        {title}
      </h2>
      {children}
    </dialog>
  );
}
