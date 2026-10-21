// What typing narrows a list to (design system: CommandPalette, FolderRow):
// what holds the letters typed, ignoring case, those where they start a word
// first; and where the letters are, to light them.

/** How well `text` matches `typed`: 0 at a word's start, 1 elsewhere, null not at all. */
export function rank(text: string, typed: string): 0 | 1 | null {
  const at = found(text, typed);
  if (at === null) return null;
  return at === 0 || !/[\p{L}\p{N}]/u.test(text[at - 1] ?? "") ? 0 : 1;
}

/** Where the letters typed are in `text`, at a word's start where they are there too. */
export function found(text: string, typed: string): number | null {
  const wanted = typed.trim().toLowerCase();
  if (!wanted) return 0;
  const lower = text.toLowerCase();
  let first: number | null = null;
  for (let at = lower.indexOf(wanted); at >= 0; at = lower.indexOf(wanted, at + 1)) {
    if (at === 0 || !/[\p{L}\p{N}]/u.test(lower[at - 1] ?? "")) return at;
    first ??= at;
  }
  return first;
}

/** `items` that match, those at a word's start first, each kind keeping its order. */
export function narrowed<T>(items: readonly T[], typed: string, text: (item: T) => string): T[] {
  const ranked = items
    .map((item) => ({ item, rank: rank(text(item), typed) }))
    .filter((entry) => entry.rank !== null);
  return ranked.toSorted((a, b) => (a.rank ?? 0) - (b.rank ?? 0)).map((entry) => entry.item);
}
