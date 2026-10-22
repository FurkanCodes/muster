// One folder or repository offered under a typed path (design system:
// FolderRow): an option of a listbox whose field keeps the keys and names
// the option it is on, so the option never takes the focus. A click chooses
// it; a double-click goes into it, or takes it.

import { type PartProps, usePart } from "./part";

export function FolderOption({
  id,
  selected,
  onChoose,
  onOpen,
  ...props
}: PartProps<"div"> & {
  /** Its id in the window, for the field's `aria-activedescendant`. */
  id: string;
  selected: boolean;
  onChoose: () => void;
  onOpen: () => void;
}) {
  // The field keeps the keys; ↑↓ there choose, as a click does here.
  return usePart("div", props, {
    id,
    role: "option",
    tabIndex: -1,
    "aria-selected": selected,
    className: "mu-folder",
    onClick: onChoose,
    onDoubleClick: onOpen,
  });
}
