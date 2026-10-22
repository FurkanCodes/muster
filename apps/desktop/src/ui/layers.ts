// Lists that float over a Modal (a Select's): ⎋ closes the one on top,
// never the Modal under it too, whichever of them hears the key first.

/** How many are open now. */
let open = 0;
/** When one was last closed by ⎋. */
let escapedAt = Number.NEGATIVE_INFINITY;

/** A floating list opened. */
export function listOpened(): void {
  open += 1;
}

/** A floating list closed; `byEscape` where ⎋ closed it. */
export function listClosed(byEscape: boolean): void {
  open = Math.max(0, open - 1);
  if (byEscape) escapedAt = performance.now();
}

/** Whether ⎋ now is a floating list's: one is open, or one has just closed on it. */
export function listTakesEscape(): boolean {
  return open > 0 || performance.now() - escapedAt < 100;
}
