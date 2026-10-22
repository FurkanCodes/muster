// A list the keyboard moves through (design system: List): the queue, the
// sessions' tree, the tickets, a workspace's files and turns. One model for
// all of them, so the focus and the selection never part:
//
// - The list holds the focus and names its selected line
//   (`aria-activedescendant`); a line never takes the focus, by key or by
//   pointer (a press on a line gives the focus to its list).
// - Its screen moves the selection, by the window's keys, and says whether
//   the list has the keys: a selected line is lit while it does, tinted
//   while they are elsewhere (`resting`).
// - The selected line is kept in sight.
//
// A row with controls in it (a setting's switch, a field) is not a line of
// a List: its row or its control takes the focus (screens/settings/Row).
//
//   <List label="Needs you sessions" activeId={lineId(selected)} resting={keysIn !== "list"}>
//     <ListItem id={lineId(id)} selected={id === selected} label={title} onSelect={() => pick(id)}>
//       <StatusMark kind="waiting" />
//       <ListItemText>
//         <ListItemTitle>{title}</ListItemTitle>
//         <ListItemMeta>asks which cache folder to use</ListItemMeta>
//       </ListItemText>
//       <ListItemEnd>3m</ListItemEnd>
//     </ListItem>
//   </List>

import { createContext, type MouseEvent, type ReactNode, use, useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import { Pressable } from "./controls";
import { type PartProps, usePart } from "./part";

interface ListShape {
  /** A tree's lines are its items; a listbox's its options. */
  kind: "listbox" | "tree";
  resting: boolean;
  /** Who keeps the selected line in sight: the line itself, or the list (one that draws only what is in sight). */
  sight: "line" | "list";
}

const Shape = createContext<ListShape>({ kind: "listbox", resting: false, sight: "line" });

export function List({
  label,
  kind = "listbox",
  activeId,
  resting = false,
  sight = "line",
  ...props
}: PartProps<"div"> & {
  /** Its name, for a screen reader. */
  label: string;
  kind?: "listbox" | "tree";
  /** The selected line's id, where it is drawn. */
  activeId?: string;
  /** The keys are elsewhere (a terminal, a dialog, a page beside the list). */
  resting?: boolean;
  /** `list` where the list keeps its selected line in sight itself, drawing only what is in sight. */
  sight?: "line" | "list";
}) {
  const list = usePart("div", props, {
    className: "mu-list",
    role: kind,
    tabIndex: 0,
    "aria-label": label,
    "aria-activedescendant": activeId,
  });
  return <Shape value={{ kind, resting, sight }}>{list}</Shape>;
}

/** A press on a line: its list takes the focus, never the line; the click then selects. */
function keepFocusOnList(event: MouseEvent<HTMLElement>): void {
  event.preventDefault();
  // A right-click (or ⌃-click) leaves the focus to the menu it opens, which may open before this.
  if (event.button !== 0 || event.ctrlKey) return;
  event.currentTarget
    .closest<HTMLElement>('[role="listbox"], [role="tree"]')
    ?.focus({ preventScroll: true });
}

export type ListItemFlash = "waiting" | "done" | "news";

export function ListItem({
  id,
  selected,
  label,
  title,
  density = "comfortable",
  level,
  expanded,
  flash,
  className,
  onSelect,
  children,
}: {
  /** Unique in the window: the list names its selected line by it. */
  id: string;
  selected: boolean;
  /** What a screen reader says for it: its name, and how it stands. */
  label: string;
  /** All of it, for the tooltip, where the line cuts it. */
  title?: string;
  /** `comfortable` for a line a person reads (a session, a ticket); `compact` for one of many (a file, a turn, a tree's). */
  density?: "comfortable" | "compact";
  /** In a tree: its depth, from 1. */
  level?: number;
  /** In a tree: whether what is under it shows. */
  expanded?: boolean;
  /** Its status just changed in sight: it flashes once, in that status's colour. */
  flash?: ListItemFlash;
  /** Its own look, beyond a line's (a ticket's key column, a file's kind). */
  className?: string;
  onSelect: () => void;
  children: ReactNode;
}) {
  const { kind, resting, sight } = use(Shape);
  const line = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (selected && sight === "line") line.current?.scrollIntoView?.({ block: "nearest" });
  }, [selected, sight]);
  return (
    <Pressable
      ref={line}
      id={id}
      role={kind === "tree" ? "treeitem" : "option"}
      tabIndex={-1}
      aria-selected={selected}
      aria-label={label}
      title={title}
      aria-level={level}
      aria-expanded={expanded}
      className={cn(
        "mu-list-item",
        `mu-list-item--${density}`,
        selected && resting && "is-resting",
        flash && `mu-flash mu-flash--${flash}`,
        className,
      )}
      onMouseDown={keepFocusOnList}
      onClick={onSelect}
    >
      {children}
    </Pressable>
  );
}

/** Its name and, under it, what is said of it: the line's middle, taking the room left. */
export function ListItemText(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-list-item-text" });
}

/** Its name, cut where it runs out of room; all of it in the tooltip. */
export function ListItemTitle(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-list-item-title" });
}

/** What is said of it under its name: what it does, its branch, how its work stands. */
export function ListItemMeta(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-list-item-meta" });
}

/** At the line's end: its age, a count, a pull request. */
export function ListItemEnd(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-list-item-end" });
}

/**
 * A line that does one thing when pressed (opens its session), outside a
 * List: a button of its own, reached by Tab as any button, drawn as a line.
 */
export function RowButton({
  label,
  title,
  className,
  onClick,
  children,
}: {
  label?: string;
  title?: string;
  className?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Pressable
      className={cn("mu-list-item mu-list-item--comfortable", className)}
      aria-label={label}
      title={title}
      onClick={onClick}
    >
      {children}
    </Pressable>
  );
}
