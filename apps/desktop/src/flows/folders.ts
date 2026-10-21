// The folder browser under the path field (design system: FolderRow; the
// TUI's crates/muster-tui/src/app/picker.rs): the folders in the folder
// typed so far, narrowed by the name after its last `/`, ignoring case,
// those starting with it first; hidden ones only once a `.` is typed.

import type { Folder } from "../ipc/generated/desktop/Folder";

/** A typed path's folder, up to its last `/`, and the name after it. */
export function split(typed: string): [string, string] {
  const at = typed.lastIndexOf("/");
  return at < 0 ? ["", typed] : [typed.slice(0, at + 1), typed.slice(at + 1)];
}

/** The folders shown for `name`, what follows the last `/`. */
export function shown(folders: readonly Folder[], name: string): Folder[] {
  const wanted = name.toLowerCase();
  const starts = (folder: Folder) => (folder.name.toLowerCase().startsWith(wanted) ? 0 : 1);
  return folders
    .filter((folder) => wanted.startsWith(".") || !folder.name.startsWith("."))
    .filter((folder) => folder.name.toLowerCase().includes(wanted))
    .toSorted((a, b) => starts(a) - starts(b));
}

/** The choice `by` from `at` among `count`; up from the first is back to the path as typed. */
export function step(at: number | null, by: number, count: number): number | null {
  if (count === 0) return null;
  if (at === null) return by > 0 ? Math.min(by - 1, count - 1) : null;
  if (at + by < 0) return null;
  return Math.min(at + by, count - 1);
}

/** The folder above `folder` (as typed, ending in `/`), given where it was listed. */
export function up(folder: string, listed: string | null): string {
  const bare = folder.replace(/\/+$/, "");
  if ((bare === "~" || bare === "") && listed) return parentOf(listed);
  return parentOf(bare);
}

function parentOf(path: string): string {
  const bare = path.replace(/\/+$/, "");
  const at = bare.lastIndexOf("/");
  return at <= 0 ? "/" : bare.slice(0, at + 1);
}

/** The whole path to add: the chosen folder, or the path as typed, `~` as listed. */
export function root(typed: string, listed: string | null, chosen: Folder | null): string {
  if (chosen) return chosen.path;
  const [folder, name] = split(typed.trim());
  const base = folder.startsWith("~") && listed ? `${listed.replace(/\/+$/, "")}/` : folder;
  const path = `${base}${name}`.replace(/\/+$/, "");
  return path || "/";
}
