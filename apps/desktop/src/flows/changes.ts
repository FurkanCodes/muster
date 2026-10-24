// What a worktree has changed, as git says it now (the daemon's
// WorkspaceChanges), for a dialog that counts commits or files before the
// person answers. Asked once, as the dialog opens; a refusal leaves the
// dialog saying less, never wrong.

import { useApp } from "../app/context";
import { ask } from "../ipc/ask";
import type { Changes } from "../ipc/generated/Changes";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { useAsked } from "../ipc/useAsked";

export function useChanges(workspace: WorkspaceId): Changes | null {
  const app = useApp();
  const asked = useAsked(
    () => ask(app.bridge, { WorkspaceChanges: { workspace } }, "Changes"),
    workspace,
  );
  return asked.state === "answered" ? asked.value : null;
}

/** `n thing`, or `n things`. */
export function count(n: number, thing: string): string {
  return `${n} ${thing}${n === 1 ? "" : "s"}`;
}
