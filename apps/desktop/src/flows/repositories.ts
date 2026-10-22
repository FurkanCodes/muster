// GitHub's repositories as the clone dialog offers them: narrowed by what is
// typed, those whose names hold it first, and a name typed taken as the
// daemon takes it (`muster_core::protocol::repositories::repository_name`).

import type { HostedRepository } from "../ipc/generated/HostedRepository";

const PART = /^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$/;

/** `owner/name`, from `owner/name` or a github.com address; null for anything else. */
export function repositoryName(typed: string): string | null {
  let bare = typed.trim();
  for (const prefix of ["https://github.com/", "http://github.com/", "git@github.com:"])
    if (bare.startsWith(prefix)) bare = bare.slice(prefix.length);
  bare = bare.replace(/\/+$/, "");
  if (bare.endsWith(".git")) bare = bare.slice(0, -4);
  const parts = bare.split("/");
  if (parts.length !== 2) return null;
  const [owner = "", name = ""] = parts;
  return PART.test(owner) && PART.test(name) ? `${owner}/${name}` : null;
}

/** The folder a clone of `owner/name` goes in. */
export function folderOf(name: string): string {
  return name.split("/").at(-1) ?? name;
}

/** Those whose names or descriptions hold `typed`, ignoring case, names first, each kept in GitHub's order. */
export function shownRepositories(
  listed: readonly HostedRepository[],
  typed: string,
): HostedRepository[] {
  const wanted = typed.trim().toLowerCase();
  const named = (one: HostedRepository) => one.name.toLowerCase().includes(wanted);
  const described = (one: HostedRepository) =>
    one.description?.toLowerCase().includes(wanted) ?? false;
  return [...listed.filter(named), ...listed.filter((one) => !named(one) && described(one))];
}
