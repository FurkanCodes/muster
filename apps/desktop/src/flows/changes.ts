// What a worktree has changed, as git says it now (the daemon's
// WorkspaceChanges), for a dialog that counts commits or files before the
// person answers. Asked once, as the dialog opens; a refusal leaves the
// dialog saying less, never wrong.

import { useEffect, useState } from "react";
import { useApp } from "../app/context";
import { replyAs } from "../ipc/ask";
import type { Changes } from "../ipc/generated/Changes";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";

export function useChanges(workspace: WorkspaceId): Changes | null {
  const app = useApp();
  const [changes, setChanges] = useState<Changes | null>(null);
  useEffect(() => {
    let current = true;
    app.bridge.command({ WorkspaceChanges: { workspace } }).then(
      (reply) => {
        const changes = replyAs(reply, "Changes");
        if (current && changes) setChanges(changes);
      },
      () => {},
    );
    return () => {
      current = false;
    };
  }, [app, workspace]);
  return changes;
}

/** `n thing`, or `n things`. */
export function count(n: number, thing: string): string {
  return `${n} ${thing}${n === 1 ? "" : "s"}`;
}
