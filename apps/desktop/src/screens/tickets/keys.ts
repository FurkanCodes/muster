// Adapt the daemon's written chords to the foundation's useKeys API. The
// Settings branch owns the eventual shared keymap; no defaults live here.
export function ticketChord(written: string): string {
  let rest = written.trim();
  let ctrl = false;
  let alt = false;
  let shift = false;
  while (/^(ctrl|alt|shift)-/i.test(rest)) {
    const dash = rest.indexOf("-");
    const mod = rest.slice(0, dash).toLowerCase();
    ctrl ||= mod === "ctrl";
    alt ||= mod === "alt";
    shift ||= mod === "shift";
    rest = rest.slice(dash + 1);
  }
  const names: Record<string, string> = {
    enter: "Enter",
    esc: "Escape",
    tab: "Tab",
    backspace: "Backspace",
    space: " ",
    up: "ArrowUp",
    down: "ArrowDown",
    left: "ArrowLeft",
    right: "ArrowRight",
    home: "Home",
    end: "End",
    pageup: "PageUp",
    pagedown: "PageDown",
  };
  const key = names[rest.toLowerCase()] ?? (/^f\d+$/i.test(rest) ? rest.toUpperCase() : rest);
  const upper = key.length === 1 && (ctrl || alt || shift) ? key.toUpperCase() : key;
  return `${ctrl ? "⌃" : ""}${alt ? "⌥" : ""}${shift && key.length !== 1 ? "⇧" : ""}${upper}`;
}

export function ticketKeyHint(written: string): string {
  return ticketChord(written)
    .replace("Enter", "⏎")
    .replace("Escape", "⎋")
    .replace("ArrowUp", "↑")
    .replace("ArrowDown", "↓")
    .replace("ArrowLeft", "←")
    .replace("ArrowRight", "→")
    .replace("Tab", "⇥");
}
