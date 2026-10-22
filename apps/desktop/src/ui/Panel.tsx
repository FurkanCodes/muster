// A live terminal shown in the detail pane beside others (design system:
// Panel): a framed `mu-panel` whose head says what it is and what runs in
// it, and is the handle that drags it; the frame and title are the accent's
// while it has the keys. Composed, not configured: the consumer puts what it
// needs in the head and the body.

import { IconButton, Spacer } from "./controls";
import { type PartProps, usePart } from "./part";

export function Panel({
  label,
  hasKeys,
  ...props
}: PartProps<"section"> & { label: string; hasKeys: boolean }) {
  return usePart("section", props, {
    className: hasKeys ? "mu-panel has-keys" : "mu-panel",
    "aria-label": label,
  });
}

/** The head: the panel's handle, its title in 600 and what runs in it in `meta`. */
export function PanelHead(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-panel-head" });
}

export function PanelTitle(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-panel-title" });
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
      <Spacer />
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
