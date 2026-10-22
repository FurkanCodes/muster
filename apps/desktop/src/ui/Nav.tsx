// A sidebar's places (design system: Sidebar): each a line with its icon,
// its name and, at its end, a count or its key; quiet labels between groups.
//
//   <NavLabel>Projects</NavLabel>
//   <NavItem icon={<Swatch colour={1} />} current onClick={…}>
//     muster <NavCount>3</NavCount>
//   </NavItem>

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

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
    <button
      type="button"
      className="mu-nav-item"
      aria-current={current ? "page" : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="mu-nav-icon">{icon}</span>
      {children}
    </button>
  );
}

/** How many there are, at the line's end; `waiting` where they wait on the person. */
export function NavCount({ tone, children }: { tone?: "waiting"; children: ReactNode }) {
  return <span className={cn("mu-nav-count", tone && `mu-nav-count--${tone}`)}>{children}</span>;
}

/** A quiet word over a group of places. */
export function NavLabel({ children }: { children: ReactNode }) {
  return <div className="mu-nav-label">{children}</div>;
}
