// A sidebar's places (design system: Sidebar): each a line with its icon,
// its name and, at its end, a count or its key; quiet labels between groups.
//
//   <NavLabel>Projects</NavLabel>
//   <NavItem icon={<Swatch colour={1} />} current onClick={…}>
//     muster <NavCount>3</NavCount>
//   </NavItem>

import type { ReactNode } from "react";
import { Pressable } from "./controls";
import { type PartProps, usePart } from "./part";

export function NavItem({
  icon,
  current = false,
  disabled = false,
  onClick,
  children,
}: {
  /** At its start, in a column of its own so names line up; none leaves the room. */
  icon?: ReactNode;
  /** The place shown now. */
  current?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Pressable
      className="mu-nav-item"
      aria-current={current ? "page" : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="mu-nav-icon">{icon}</span>
      {children}
    </Pressable>
  );
}

/** How many there are, at the line's end; `waiting` where they wait on the person. */
export function NavCount({ tone, ...props }: PartProps<"span"> & { tone?: "waiting" }) {
  return usePart("span", props, {
    className: tone ? `mu-nav-count mu-nav-count--${tone}` : "mu-nav-count",
  });
}

/** A quiet word over a group of places. */
export function NavLabel(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-nav-label" });
}
