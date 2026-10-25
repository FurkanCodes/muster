// The panels' layout, as the terminal UI keeps it (`muster_core::layout`,
// `ARCHITECTURE.md` §5a Panels): a small tree of Muster's own panels (the
// list, the preview, the changes and a plugin's panel) and every terminal the
// person placed in a panel of its own, an agent's or a shell's
// (`session:<its id>`), kept as one line of Muster's own settings
// (`preferences.panels`):
//
//   session:ses_01J… | list | session 60% | (changes / plugin)
//
// `|` puts two side by side and `/` one above the other; a run of one of
// them nests to the right, and brackets mix them. A part may say its size
// within the two it is one of: a share (`60%`) or cells (`30`). One without
// a size shares evenly. A layout lacking a panel, as one written before that
// panel was Muster's, gets it where the default puts it; one naming the
// retired `shell` panel, whose place went to shells placed as any terminal
// is, reads without it. Two panels swap places, as one dropped on another's
// middle does. The daemon checks
// what it is sent the same way; this reads, moves and sizes it in the window
// so the two UIs write the same line.

/** One of Muster's own panels: `session` is the preview. */
export type OwnPanel = "list" | "session" | "changes" | "plugin";
/** A session placed in a panel of its own, by its id. */
export type PlacedPanelName = `session:${string}`;
export type PanelName = OwnPanel | PlacedPanelName;
export type Split = "across" | "down";
export type Size = { share: number } | { cells: number };
export type Edge = "left" | "right" | "top" | "bottom";
export type Branch = "first" | "second";
export type Place =
  | { window: Edge }
  | { beside: PanelName; edge: Edge }
  /** In another panel's place, which takes the place left. */
  | { over: PanelName };

export type Part =
  | { kind: "panel"; panel: PanelName }
  | { kind: "split"; split: Split; first: LayoutNode; second: LayoutNode };

export interface LayoutNode {
  part: Part;
  size: Size | null;
}

/** Muster's own panels, in the order the default layout names them; every layout has each once. */
export const PANELS: readonly OwnPanel[] = ["list", "session", "changes", "plugin"];

/** The word of the panel Muster no longer has, which a layout written before still names. */
const RETIRED = "shell";

const PLACED = "session:";
/** How a session's id is printed in the layout's text (`SessionId`'s `Display`); the wire has the bare ULID. */
const PRINTED = "ses_";

/** The panel of `session` (its id as the wire has it), placed. */
export function placedPanel(session: string): PlacedPanelName {
  return `${PLACED}${PRINTED}${session}`;
}

/** The session placed in `panel`, by its id as the wire has it, if it is a placed session's panel. */
export function placedSession(panel: PanelName): string | null {
  return panel.startsWith(PLACED + PRINTED) ? panel.slice(PLACED.length + PRINTED.length) : null;
}

/** What the person sees first. */
export const DEFAULT_LAYOUT = "list | session 60% | (changes / plugin)";

/** Why a layout does not read, said for the person as the daemon says it. */
export class LayoutError extends Error {
  override name = "LayoutError";
}

const SIGNS: Record<Split, string> = { across: "|", down: "/" };

type Token =
  | { kind: "word"; word: string }
  | { kind: "size"; size: Size }
  | { kind: "sign"; split: Split }
  | { kind: "open" }
  | { kind: "close" };

function isPanel(word: string): word is PanelName {
  if (word.startsWith(PLACED))
    return /^ses_[0-9A-HJKMNP-TV-Z]{26}$/i.test(word.slice(PLACED.length));
  return (PANELS as readonly string[]).includes(word);
}

function sizeOf(word: string): Size {
  const not = () =>
    new LayoutError(`"${word}" is not a size: a size is a share like 60% or a width like 30`);
  if (word.endsWith("%")) {
    const digits = word.slice(0, -1);
    if (!/^\d+$/.test(digits)) throw not();
    const share = Number(digits);
    if (share < 1 || share > 99) throw not();
    return { share };
  }
  if (!/^\d+$/.test(word)) throw not();
  const cells = Number(word);
  if (cells < 1 || cells > 65_535) throw not();
  return { cells };
}

function tokens(text: string): Token[] {
  const out: Token[] = [];
  let at = 0;
  while (at < text.length) {
    const c = text[at] as string;
    at += 1;
    if (c === " " || c === "\t") continue;
    if (c === "|") out.push({ kind: "sign", split: "across" });
    else if (c === "/") out.push({ kind: "sign", split: "down" });
    else if (c === "(") out.push({ kind: "open" });
    else if (c === ")") out.push({ kind: "close" });
    else {
      let word = c;
      while (at < text.length && !" \t|/()".includes(text[at] as string)) {
        word += text[at];
        at += 1;
      }
      out.push(/^\d/.test(word) ? { kind: "size", size: sizeOf(word) } : { kind: "word", word });
    }
  }
  return out;
}

class Reader {
  at = 0;
  constructor(readonly tokens: Token[]) {}

  peek(): Token | undefined {
    return this.tokens[this.at];
  }

  /** A run of terms joined by one sign, nested to the right; a retired panel's left out, null where nothing else is. */
  node(): LayoutNode | null {
    const first = this.term();
    const sign = this.peek();
    if (sign?.kind !== "sign") return first;
    const terms = [first];
    for (let next = this.peek(); next?.kind === "sign"; next = this.peek()) {
      if (next.split !== sign.split)
        throw new LayoutError("| and / are mixed without brackets around one of them");
      this.at += 1;
      terms.push(this.term());
    }
    const kept = terms.filter((term): term is LayoutNode => term !== null);
    let nested = kept.pop();
    if (!nested) return null;
    // The one term left takes the place the run had.
    if (kept.length === 0) return { part: nested.part, size: null };
    for (let term = kept.pop(); term; term = kept.pop()) {
      nested = {
        part: { kind: "split", split: sign.split, first: term, second: nested },
        size: null,
      };
    }
    return nested;
  }

  /** A panel or a bracket, with its size; null for a retired panel, or a bracket of nothing else. */
  term(): LayoutNode | null {
    const token = this.peek();
    let part: Part | null;
    if (token?.kind === "word") {
      this.at += 1;
      const word = token.word.startsWith(PLACED) ? token.word : token.word.toLowerCase();
      if (word === RETIRED) part = null;
      else if (!isPanel(word))
        throw new LayoutError(
          `"${word}" is not a panel: the panels are list, session, changes and plugin, and a session placed, session:<its id>`,
        );
      else part = { kind: "panel", panel: word };
    } else if (token?.kind === "open") {
      this.at += 1;
      const inner = this.node();
      if (this.peek()?.kind !== "close")
        throw new LayoutError("a bracket is not closed, or one is closed that was not opened");
      this.at += 1;
      part = inner?.part ?? null;
    } else if (token?.kind === "close") {
      throw new LayoutError("a bracket is not closed, or one is closed that was not opened");
    } else {
      throw new LayoutError("something is missing before or after a | or a /");
    }
    const after = this.peek();
    const size = after?.kind === "size" ? after.size : null;
    if (size) this.at += 1;
    return part ? { part, size } : null;
  }
}

/** The tree `text` writes, whatever panels it has, without a retired one; null where it names that alone. */
function tree(text: string): LayoutNode | null {
  const reader = new Reader(tokens(text));
  const root = reader.node();
  const left = reader.peek();
  if (left)
    throw new LayoutError(
      left.kind === "close"
        ? "a bracket is not closed, or one is closed that was not opened"
        : "something is missing before or after a | or a /",
    );
  return root;
}

/** The panels in `node`, first to last. */
export function panelsIn(node: LayoutNode): PanelName[] {
  return node.part.kind === "panel"
    ? [node.part.panel]
    : [...panelsIn(node.part.first), ...panelsIn(node.part.second)];
}

export function contains(node: LayoutNode, panel: PanelName): boolean {
  return node.part.kind === "panel"
    ? node.part.panel === panel
    : contains(node.part.first, panel) || contains(node.part.second, panel);
}

const isFirst = (edge: Edge) => edge === "left" || edge === "top";
const splitOf = (edge: Edge): Split => (edge === "left" || edge === "right" ? "across" : "down");

/** `moving` and `staying` sharing `staying`'s place, `moving` at `edge` of it. */
function joined(
  moving: LayoutNode,
  staying: LayoutNode,
  edge: Edge,
  size: Size | null,
): LayoutNode {
  const kept = { ...staying, size: null };
  const [first, second] = isFirst(edge) ? [moving, kept] : [kept, moving];
  return { part: { kind: "split", split: splitOf(edge), first, second }, size };
}

/** `node` with `moving` put at `edge` of `target`. */
function beside(node: LayoutNode, target: PanelName, moving: LayoutNode, edge: Edge): LayoutNode {
  const part = node.part;
  if (part.kind === "panel")
    return part.panel === target ? joined(moving, node, edge, node.size) : node;
  const [first, second] = contains(part.first, target)
    ? [beside(part.first, target, moving, edge), part.second]
    : [part.first, beside(part.second, target, moving, edge)];
  return { part: { ...part, first, second }, size: node.size };
}

/** `node` without `panel`, and the size `panel` had; null when `node` is `panel` alone, or lacks it. */
function without(node: LayoutNode, panel: PanelName): [LayoutNode, Size | null] | null {
  const part = node.part;
  if (part.kind !== "split") return null;
  for (const [self, other, firstIsSelf] of [
    [part.first, part.second, true],
    [part.second, part.first, false],
  ] as const) {
    if (self.part.kind === "panel" && self.part.panel === panel)
      return [{ part: other.part, size: node.size }, self.size];
    const found = without(self, panel);
    if (found) {
      const [rest, size] = found;
      const [first, second] = firstIsSelf ? [rest, other] : [other, rest];
      return [{ part: { ...part, first, second }, size: node.size }, size];
    }
  }
  return null;
}

/** Where `node` puts `panel`: at an edge of the panel nearest it that `present` says is there. */
function placeIn(
  node: LayoutNode,
  panel: PanelName,
  present: (panel: PanelName) => boolean,
): [PanelName, Edge] | null {
  const part = node.part;
  if (part.kind !== "split") return null;
  let self: LayoutNode;
  let other: LayoutNode;
  let after: boolean;
  if (contains(part.first, panel)) [self, other, after] = [part.first, part.second, false];
  else if (contains(part.second, panel)) [self, other, after] = [part.second, part.first, true];
  else return null;
  const inner = placeIn(self, panel, present);
  if (inner) return inner;
  const panels = panelsIn(other);
  const nearest = (after ? panels.reverse() : panels).find(present);
  if (!nearest) return null;
  const edge: Edge =
    part.split === "across" ? (after ? "right" : "left") : after ? "bottom" : "top";
  return [nearest, edge];
}

/** `root` with `panel`, which it lacks, where the default layout puts it. */
function completed(root: LayoutNode, panel: PanelName): LayoutNode {
  const moving: LayoutNode = { part: { kind: "panel", panel }, size: null };
  const place = placeIn(tree(DEFAULT_LAYOUT) as LayoutNode, panel, (other) =>
    contains(root, other),
  );
  return place ? beside(root, place[0], moving, place[1]) : joined(moving, root, "right", null);
}

function kept(node: LayoutNode, shown: (panel: PanelName) => boolean): LayoutNode | null {
  const part = node.part;
  if (part.kind === "panel") return shown(part.panel) ? node : null;
  const first = kept(part.first, shown);
  const second = kept(part.second, shown);
  if (first && second) return { part: { ...part, first, second }, size: node.size };
  const only = first ?? second;
  return only ? { part: only.part, size: node.size } : null;
}

function atPath(node: LayoutNode, path: readonly Branch[]): LayoutNode | null {
  let at: LayoutNode = node;
  for (const step of path) {
    if (at.part.kind !== "split") return null;
    at = step === "first" ? at.part.first : at.part.second;
  }
  return at;
}

/** The size `panel` is given where it is in `node`, null for none; undefined where it is not there. */
function sizeIn(node: LayoutNode, panel: PanelName): Size | null | undefined {
  const part = node.part;
  if (part.kind === "panel") return part.panel === panel ? node.size : undefined;
  const first = sizeIn(part.first, panel);
  return first !== undefined ? first : sizeIn(part.second, panel);
}

function cloneNode(node: LayoutNode): LayoutNode {
  const part = node.part;
  return part.kind === "panel"
    ? { part, size: node.size }
    : {
        part: { ...part, first: cloneNode(part.first), second: cloneNode(part.second) },
        size: node.size,
      };
}

function sizeText(size: Size | null): string {
  if (!size) return "";
  return "share" in size ? ` ${size.share}%` : ` ${size.cells}`;
}

function write(node: LayoutNode, within: Split | null): string {
  const part = node.part;
  let text: string;
  if (part.kind === "panel") text = part.panel;
  else {
    const inner = `${write(part.first, part.split)} ${SIGNS[part.split]} ${writeRest(part.second, part.split)}`;
    text = within ? `(${inner})` : inner;
  }
  return text + sizeText(node.size);
}

/** The rest of a run of `split`'s sign, without brackets where it runs on. */
function writeRest(node: LayoutNode, split: Split): string {
  const part = node.part;
  if (part.kind === "split" && part.split === split && node.size === null)
    return `${write(part.first, split)} ${SIGNS[split]} ${writeRest(part.second, split)}`;
  return write(node, split);
}

/** The panels' layout. Every panel is in it once, shown or not. */
export class Layout {
  private constructor(readonly root: LayoutNode) {}

  /** Reads a layout as `[layout] panels` writes it, completing what it lacks. */
  static parse(text: string): Layout {
    const read = tree(text);
    if (!read) return Layout.default();
    let root = read;
    const seen = panelsIn(root);
    for (const [at, panel] of seen.entries()) {
      if (seen.indexOf(panel) !== at) throw new LayoutError(`${panel} is in the layout twice`);
    }
    root = { part: root.part, size: null };
    for (const panel of PANELS) {
      if (!contains(root, panel)) root = completed(root, panel);
    }
    return new Layout(root);
  }

  /** The layout `text` writes, or the default where it does not read. */
  static read(text: string | null | undefined): Layout {
    try {
      return Layout.parse(text ?? DEFAULT_LAYOUT);
    } catch {
      return Layout.parse(DEFAULT_LAYOUT);
    }
  }

  static default(): Layout {
    return Layout.parse(DEFAULT_LAYOUT);
  }

  toString(): string {
    return write(this.root, null);
  }

  equals(other: Layout): boolean {
    return this.toString() === other.toString();
  }

  /** The sessions placed, in the layout's order. */
  placed(): string[] {
    return panelsIn(this.root).flatMap((panel) => placedSession(panel) ?? []);
  }

  /** The layout with `session` placed at `place`, or moved there where it is placed already. */
  placing(session: string, place: Place): Layout {
    const panel = placedPanel(session);
    if (contains(this.root, panel)) return this.moved(panel, place);
    const moving: LayoutNode = { part: { kind: "panel", panel }, size: null };
    // A session not placed yet has no place to give: over another, it goes beside.
    const target = "beside" in place ? place.beside : "over" in place ? place.over : null;
    const edge = "beside" in place ? place.edge : "right";
    const root =
      "window" in place
        ? joined(moving, this.root, place.window, null)
        : target && contains(this.root, target)
          ? beside(this.root, target, moving, edge)
          : joined(moving, this.root, "right", null);
    return new Layout({ part: root.part, size: null });
  }

  /** The layout with `session` no longer placed: its place goes to the other of its two. */
  unplaced(session: string): Layout {
    return this.without(placedPanel(session));
  }

  /**
   * The layout with `one` and `other` in each other's places, as a panel
   * dropped on another's middle swaps with it. A share stays with its place;
   * a size in cells goes with its panel. As it was where either is not in
   * it, or the two are one.
   */
  swapped(one: PanelName, other: PanelName): Layout {
    const oneSize = sizeIn(this.root, one);
    const otherSize = sizeIn(this.root, other);
    if (oneSize === undefined || otherSize === undefined || one === other) return this;
    const taken = (leaving: Size | null, coming: Size | null): Size | null =>
      coming && "cells" in coming ? coming : leaving && "cells" in leaving ? null : leaving;
    const exchange = (node: LayoutNode): LayoutNode => {
      const part = node.part;
      if (part.kind === "panel") {
        if (part.panel === one)
          return { part: { kind: "panel", panel: other }, size: taken(oneSize, otherSize) };
        if (part.panel === other)
          return { part: { kind: "panel", panel: one }, size: taken(otherSize, oneSize) };
        return node;
      }
      return {
        part: { ...part, first: exchange(part.first), second: exchange(part.second) },
        size: node.size,
      };
    };
    return new Layout(exchange(this.root));
  }

  /** The layout without `panel`, its place gone to the other of its two. */
  without(panel: PanelName): Layout {
    const found = without(this.root, panel);
    return found ? new Layout({ part: found[0].part, size: null }) : this;
  }

  /**
   * The layout with `panel` moved to `place`. A panel moved beside itself
   * stays. It keeps a size in cells it had; a share it had was of the
   * place it left.
   */
  moved(panel: PanelName, place: Place): Layout {
    if ("beside" in place && place.beside === panel) return this;
    if ("over" in place) return this.swapped(panel, place.over);
    const found = without(this.root, panel);
    if (!found) return this;
    const [rest, size] = found;
    const moving: LayoutNode = {
      part: { kind: "panel", panel },
      size: size && "cells" in size ? size : null,
    };
    const root =
      "window" in place
        ? joined(moving, rest, place.window, null)
        : beside(rest, place.beside, moving, place.edge);
    return new Layout({ part: root.part, size: null });
  }

  /**
   * The layout with `panel` moved to `edge` of the smallest part that holds
   * all of `around` (the panels of a region of the window, such as the
   * session's pane): the window's edge, for a window that is that region.
   */
  movedToEdgeOf(panel: PanelName, around: readonly PanelName[], edge: Edge): Layout {
    const found = without(this.root, panel);
    if (!found) return this;
    const [rest, size] = found;
    const moving: LayoutNode = {
      part: { kind: "panel", panel },
      size: size && "cells" in size ? size : null,
    };
    const others = around.filter((other) => other !== panel);
    const swap = (node: LayoutNode): LayoutNode => {
      const part = node.part;
      if (part.kind === "split") {
        if (others.every((other) => contains(part.first, other)))
          return { part: { ...part, first: swap(part.first) }, size: node.size };
        if (others.every((other) => contains(part.second, other)))
          return { part: { ...part, second: swap(part.second) }, size: node.size };
      }
      return joined(moving, node, edge, node.size);
    };
    const root = swap(rest);
    return new Layout({ part: root.part, size: null });
  }

  /**
   * The layout with the split at `path` sharing out its place by `size`,
   * given to its `branch`; the other of the two takes what is left.
   */
  sized(path: readonly Branch[], branch: Branch, size: Size): Layout {
    const root = cloneNode(this.root);
    const node = atPath(root, path);
    if (node?.part.kind !== "split") return this;
    const [given, other] =
      branch === "first"
        ? [node.part.first, node.part.second]
        : [node.part.second, node.part.first];
    given.size = size;
    other.size = null;
    return new Layout(root);
  }

  /**
   * The path to the split that shares out a place between `first`'s panels
   * and `second`'s; null if none does. Panels not shown may be on either side.
   */
  splitBetween(first: readonly PanelName[], second: readonly PanelName[]): Branch[] | null {
    const path: Branch[] = [];
    let node = this.root;
    const holds = (at: LayoutNode, panels: readonly PanelName[]) =>
      panels.every((panel) => contains(at, panel));
    for (;;) {
      const part = node.part;
      if (part.kind !== "split") return null;
      if (holds(part.first, first) && holds(part.second, second)) return path;
      const all = [...first, ...second];
      if (holds(part.first, all)) {
        path.push("first");
        node = part.first;
      } else if (holds(part.second, all)) {
        path.push("second");
        node = part.second;
      } else return null;
    }
  }

  /** The panels shown, as a tree: each not shown leaves its place to the other of its two. */
  shown(shown: (panel: PanelName) => boolean): LayoutNode | null {
    return kept(this.root, shown);
  }
}
