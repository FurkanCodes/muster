import { cpSync } from "node:fs";
import { join, resolve } from "node:path";
import { sandbox } from "./harness";
import { muster } from "./muster";
import { git } from "./repos";

// Anyone's ticket plugin, the daemon's own test fixture
// (crates/muster/tests/fixtures/ticket-plugin): tickets from a file beside
// it, and a `source` it reports, so projects choosing the same board share
// their tickets, as GitHub's own issues never do.

const FIXTURE = resolve(
  import.meta.dirname,
  "../../../../crates/muster/tests/fixtures/ticket-plugin",
);

/** Installs the fixture plugin from a git repository of its own, as its author would publish it. */
export function installTicketPlugin(): void {
  const repo = join(sandbox().root, "plugins", "fixture");
  cpSync(FIXTURE, repo, { recursive: true });
  git(repo, "init", "--quiet", "--initial-branch", "main");
  git(repo, "add", ".");
  git(repo, "commit", "--quiet", "--message", "the fixture plugin");
  muster("plugin", "install", `file://${repo}`, "--yes");
}
