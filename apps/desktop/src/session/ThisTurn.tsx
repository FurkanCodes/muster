// Under the terminal: what the workspace's newest turn changed, a click from
// its diff and from putting it back (design system: ThisTurn). The timeline
// is asked for once when shown and again only when the daemon says it has a
// newer entry (`workspace.timeline`), never polled, and not while the window
// is hidden (`ARC-25`). Its diff and its putting back are the changes view's
// (feature/desktop-changes), opened on the turn.

import { useEffect, useRef, useState } from "react";
import { useActions, useBridge, useDaemon, useUi } from "../app/context";
import type { Entry } from "../ipc/generated/Entry";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { splitPath } from "../lib/format";
import { standsBeside } from "../state/selectors";
import { Button } from "../ui/controls";
import { DiffStat } from "../ui/status";
import { type TurnFile, turnFiles, turnLabel } from "./turn";

type Read =
  | { state: "reading" }
  | { state: "failed"; message: string }
  | {
      state: "read";
      entry: Entry | null;
      label: string;
      files: TurnFile[];
      cut: boolean;
      filesFailed: string | null;
    };

/** "changed 3 files", or "changed no files" for a turn that changed none. */
function changed(entry: Entry): string {
  const files = entry.summary?.files ?? 0;
  if (files === 0) return "changed no files";
  return `changed ${files} ${files === 1 ? "file" : "files"}`;
}

/** Reads the newest entry and its files, as often as the daemon says it changed. */
function useNewestTurn(workspace: WorkspaceId): [Read, () => void] {
  const bridge = useBridge();
  // Asked once the workspace is known: before the daemon's snapshot there is nothing to ask of.
  const known = useDaemon((state) => state.workspaces[workspace] !== undefined);
  const newest = useDaemon((state) => state.workspaces[workspace]?.timeline ?? null);
  const visible = useUi((state) => state.visible);
  const [again, setAgain] = useState(0);
  const [read, setRead] = useState<Read>({ state: "reading" });
  const readFor = useRef<string | null>(null);
  const wanted = `${newest}:${again}`;

  useEffect(() => {
    // Hidden, nothing is asked; shown again, only what changed meanwhile.
    if (!known || !visible || readFor.current === wanted) return;
    readFor.current = wanted;
    let current = true;
    let answered = false;
    setRead({ state: "reading" });
    void (async () => {
      try {
        const reply = await bridge.command({ Timeline: { workspace } });
        if (!current) return;
        const timeline = typeof reply === "object" && "Timeline" in reply ? reply.Timeline : null;
        if (!timeline) throw new Error("Muster answered something other than the timeline.");
        const entry = timeline.entries[0] ?? null;
        const shown = {
          state: "read" as const,
          entry,
          label: entry ? turnLabel(timeline, entry) : "",
          files: [],
          cut: false,
          filesFailed: null,
        };
        setRead(shown);
        if (!entry?.summary?.files) return;
        try {
          const diff = await bridge.command({ EntryDiff: { workspace, number: entry.number } });
          if (!current) return;
          if (typeof diff !== "object" || !("FileDiff" in diff))
            throw new Error("Muster answered something other than the turn's diff.");
          setRead({ ...shown, files: turnFiles(diff.FileDiff.text), cut: diff.FileDiff.cut });
        } catch (error) {
          if (current) setRead({ ...shown, filesFailed: (error as Error).message });
        }
      } catch (error) {
        if (current) setRead({ state: "failed", message: (error as Error).message });
      } finally {
        answered = true;
      }
    })();
    return () => {
      current = false;
      // Left before the answer came: asked again the next time it is shown.
      if (!answered && readFor.current === wanted) readFor.current = null;
    };
  }, [bridge, workspace, wanted, visible, known]);

  return [read, () => setAgain((count) => count + 1)];
}

export function ThisTurn({ workspace }: { workspace: WorkspaceId }) {
  const actions = useActions();
  const [read, again] = useNewestTurn(workspace);
  // While an agent works there, its files are not to be put back (PRD-14).
  const busy = useDaemon((state) =>
    Object.values(state.sessions).some(
      (session) =>
        session.workspace === workspace &&
        session.status.state === "running" &&
        !standsBeside(session),
    ),
  );

  if (read.state !== "read") {
    return (
      <section className="mu-turn session-turn" aria-label="This turn">
        <div className="mu-turn-head">
          <span className="mu-turn-title">This turn</span>
          {read.state === "reading" ? (
            <span className="mu-meta" role="status">
              Reading the timeline…
            </span>
          ) : (
            <>
              <span className="mu-meta mu-cut" role="alert">
                The timeline could not be read: {read.message}
              </span>
              <span className="mu-spacer" />
              <Button size="sm" onClick={again}>
                Try again
              </Button>
            </>
          )}
        </div>
      </section>
    );
  }

  const { entry } = read;
  if (!entry) {
    return (
      <section className="mu-turn session-turn" aria-label="This turn">
        <div className="mu-turn-head">
          <span className="mu-turn-title">This turn</span>
          <span className="mu-meta">None kept yet: each agent's turn here is, from now on.</span>
        </div>
      </section>
    );
  }

  const diff = () => actions.showTurn(workspace, entry.number, "diff");
  return (
    <section className="mu-turn session-turn" aria-label="This turn">
      <div className="mu-turn-head">
        <span className="mu-turn-title">This turn</span>
        <span className="mu-meta mu-cut">
          {read.label}, {changed(entry)}
        </span>
        {entry.summary?.files ? (
          <DiffStat added={entry.summary.added} removed={entry.summary.removed} />
        ) : null}
        <span className="mu-spacer" />
        <Button
          size="sm"
          disabled={busy}
          title={busy ? "Wait for its turn to end" : "Put the files back as before this turn (z)"}
          onClick={() => actions.showTurn(workspace, entry.number, "put-back")}
        >
          Put back
        </Button>
        <Button size="sm" title="See the diff" onClick={diff}>
          See the diff
        </Button>
      </div>
      {read.files.length ? (
        <div className="mu-turn-files">
          {read.files.map((file) => (
            <button
              type="button"
              className="mu-pill"
              key={file.path}
              title={file.path}
              aria-label={`${file.path}, its diff`}
              onClick={diff}
            >
              <span className="mu-mono">{splitPath(file.path).name}</span>
              {file.lines ? (
                <DiffStat added={file.lines.added} removed={file.lines.removed} />
              ) : null}
            </button>
          ))}
        </div>
      ) : null}
      {read.cut ? (
        <span className="mu-meta">The diff was cut short; it changed more files than these.</span>
      ) : null}
      {read.filesFailed ? (
        <span className="mu-meta" role="alert">
          Its files could not be read: {read.filesFailed}
        </span>
      ) : null}
    </section>
  );
}
