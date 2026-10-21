// A live terminal shown in the detail pane beside others (design system:
// Panel): a framed `mu-panel` whose head says what it is and what runs in
// it, and is the handle that drags it; the frame and title are the accent's
// while it has the keys. Composed, not configured: the consumer puts what it
// needs in the head and the body.

import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import { IconButton } from "./controls";

export function Panel({
  label,
  hasKeys,
  className = "",
  style,
  children,
}: {
  label: string;
  hasKeys: boolean;
  className?: string;
  style?: CSSProperties;
  children: ReactNode;
}) {
  return (
    <section
      className={`mu-panel ${hasKeys ? "has-keys" : ""} ${className}`}
      aria-label={label}
      style={style}
    >
      {children}
    </section>
  );
}

/** The head: the panel's handle, its title in 600 and what runs in it in `meta`. */
export function PanelHead({ children, ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className="mu-panel-head" {...rest}>
      {children}
    </div>
  );
}

export function PanelTitle({ children }: { children: ReactNode }) {
  return <span className="mu-panel-title">{children}</span>;
}

/** The head's way to put the panel away, with its key in the tooltip. */
export function PanelClose({
  label,
  keyHint,
  onClose,
}: {
  label: string;
  keyHint: string | null;
  onClose: () => void;
}) {
  return (
    <>
      <span className="mu-spacer" />
      <IconButton
        icon="close"
        size="sm"
        label={label}
        title={keyHint ? `Put it away (${keyHint})` : "Put it away"}
        onClick={onClose}
        onPointerDown={(event) => event.stopPropagation()}
      />
    </>
  );
}
