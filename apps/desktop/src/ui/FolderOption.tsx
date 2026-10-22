// One folder or repository offered under a typed path (design system:
// FolderRow): an option of a listbox whose field keeps the keys and names
// the option it is on, so the option never takes the focus. A click chooses
// it; a double-click goes into it, or takes it.

import type { ReactNode } from "react";

export function FolderOption({
  id,
  selected,
  className,
  onChoose,
  onOpen,
  children,
}: {
  /** Its id in the window, for the field's `aria-activedescendant`. */
  id: string;
  selected: boolean;
  className?: string;
  onChoose: () => void;
  onOpen: () => void;
  children: ReactNode;
}) {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the field keeps the keys; ↑↓ there choose, as a click does here.
    <div
      id={id}
      role="option"
      tabIndex={-1}
      aria-selected={selected}
      className={className ? `mu-folder ${className}` : "mu-folder"}
      onClick={onChoose}
      onDoubleClick={onOpen}
    >
      {children}
    </div>
  );
}
