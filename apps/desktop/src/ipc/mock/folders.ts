// A small pretend file system for the folder browser: no test or mock
// build ever reads the person's own folders. Home is /Users/you.

import type { Folder } from "../generated/desktop/Folder";

export const HOME = "/Users/you";

/** Each folder's folders, by its whole path; a git repository marked. */
export function folderTree(): Record<string, Folder[]> {
  const tree: Record<string, Folder[]> = {};
  const add = (parent: string, name: string, git = false) => {
    const path = parent === "/" ? `/${name}` : `${parent}/${name}`;
    tree[parent] = [...(tree[parent] ?? []), { name, path, git }];
    tree[path] ??= [];
  };
  tree["/"] = [];
  add("/", "Users");
  add("/Users", "you");
  for (const name of [".config", "code", "Desktop", "Documents", "Downloads"]) add(HOME, name);
  add(HOME, "muster", true);
  add(`${HOME}/code`, ".cache");
  add(`${HOME}/code`, "kep-api", true);
  add(`${HOME}/code`, "kep-site", true);
  add(`${HOME}/code`, "labs");
  add(`${HOME}/code`, "lamphaus", true);
  add(`${HOME}/code`, "lamphaus-api", true);
  add(`${HOME}/code/labs`, "notes");
  return tree;
}

/** A path as the app's list_folders takes it: `~` is home; no trailing `/`. */
export function expandPath(typed: string): string {
  const trimmed = typed.trim();
  const path = /^~(\/|$)/.test(trimmed) ? `${HOME}${trimmed.slice(1)}` : trimmed;
  return path.length > 1 ? path.replace(/\/+$/, "") : path;
}
