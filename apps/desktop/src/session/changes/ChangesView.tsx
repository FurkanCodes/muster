// A workspace's changes in the session pane (PRODUCT.md §6, the Changes
// board): what is not committed, the branch's commits and files, and the
// timeline's turns, each with its diff, and putting a turn back. The keys
// are the TUI's (app::changes): ↑↓ through the list, ⏎ opens a diff, ⎋ in
// a diff goes back to the list and in the list back to the terminal; ⇥
// goes round the three views. Its actions answer the person's keys
// (`muster_core::keys`, "In the changes"): d or q back to the terminal, r
// asks git again, z puts a turn back, ? lists these keys.

import { type ReactNode, useEffect, useRef, useState } from "react";
import { useActions, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import type { Command } from "../../ipc/generated/Command";
import type { WorkspaceId } from "../../ipc/generated/WorkspaceId";
import { labelOf } from "../../keys/keymap";
import { useActionKeys, useKeymap } from "../../keys/useKeymap";
import { splitPath } from "../../lib/format";
import type { Hint } from "../../state/ui";
import { Button, Segmented } from "../../ui/controls";
import { fileKind } from "../../ui/FileRow";
import { DiffStat } from "../../ui/status";
import { ChangesList, type View } from "./ChangesList";
import { ClashBanners } from "./ClashBanners";
import { DiffPanel } from "./DiffPanel";
import { entryLabel, oneLine } from "./labels";
import { PutBack } from "./PutBack";
import { useWorkspaceChanges } from "./useWorkspaceChanges";
import "./changes.css";

const VIEWS: readonly View[] = ["uncommitted", "branch", "turns"];
const PART: Record<View, string> = {
  uncommitted: "Not committed",
  branch: "On the branch",
  turns: "Turns",
};
/** How far PgUp and PgDn move through the list. */
const PAGE = 10;

export default function ChangesView({ workspace }: { workspace: WorkspaceId }) {
  // This turn (the session pane's) opens the view on a turn, to see it or put it back.
  const target = useUi((state) =>
    state.changesTurn?.workspace === workspace ? state.changesTurn : null,
  );
  // Asked of the daemon only once the state names the workspace (a client's first patch).
  const known = useDaemon((state) => workspace in state.workspaces);
  if (!known) return null;
  return (
    <WorkspaceChanges
      key={`${workspace}:${target?.request ?? 0}`}
      workspace={workspace}
      target={target}
    />
  );
}

function WorkspaceChanges({
  workspace,
  target,
}: {
  workspace: WorkspaceId;
  target: { number: number; intent: "diff" | "put-back" } | null;
}) {
  const actions = useActions();
  const [again, setAgain] = useState(0);
  const [view, setView] = useState<View>(target ? "turns" : "uncommitted");
  const [selection, setSelection] = useState<string | null>(target ? String(target.number) : null);
  const [opened, setOpened] = useState<string | null>(
    target?.intent === "diff" ? String(target.number) : null,
  );
  const [keysInDiff, setKeysInDiff] = useState(target?.intent === "diff");
  const [undoing, setUndoing] = useState<number | null>(
    target?.intent === "put-back" ? target.number : null,
  );
  const rows = useRef<HTMLDivElement>(null);
  /** The entry an undo just made, to select and focus once the timeline lists it. */
  const newEntry = useRef<string | null>(null);
  /** The selected row takes the focus once it is drawn: a long list draws only what is in sight. */
  const focusing = useRef(false);
  // Moving to where the selection already is draws again, so the row takes the focus all the same.
  const [, setFollow] = useState(0);
  const focusSelected = () => {
    focusing.current = true;
    setFollow((count) => count + 1);
  };
  const { listing, timeline } = useWorkspaceChanges(workspace, again);
  const changes = listing.state === "ready" ? listing.value : null;
  const kept = timeline.state === "ready" ? timeline.value : { entries: [] };
  const files = view === "turns" ? [] : (changes?.[view] ?? []);
  const ids =
    view === "turns"
      ? kept.entries.map((entry) => String(entry.number))
      : files.map((file) => file.path);
  const selected = selection !== null && ids.includes(selection) ? selection : (ids[0] ?? null);
  const shown = opened !== null && ids.includes(opened) ? opened : null;
  const entry =
    view === "turns" ? kept.entries.find((found) => String(found.number) === shown) : undefined;
  const file = files.find((found) => found.path === shown);

  /** Gives the selected row the focus, where it is wanted and drawn. */
  const focusWanted = () => {
    if (!focusing.current) return;
    const row = rows.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!row) return;
    focusing.current = false;
    row.focus({ preventScroll: true });
    row.scrollIntoView({ block: "nearest" });
  };
  useEffect(() => {
    const made = newEntry.current;
    if (made !== null && ids.includes(made)) {
      newEntry.current = null;
      focusing.current = true;
    }
    focusWanted();
  });
  const changeView = (next: View) => {
    setView(next);
    setSelection(null);
    setOpened(null);
  };
  const toTerminal = () => actions.showPane("terminal");
  const open = (id: string | null) => {
    if (id === null) return;
    focusing.current = false;
    setSelection(id);
    setOpened(id);
    setKeysInDiff(true);
  };
  const focusList = () => {
    setKeysInDiff(false);
    focusSelected();
  };
  const putBack = () => {
    // The turn whose diff has the keys, else the one selected in the list.
    const number = view === "turns" ? Number((keysInDiff ? shown : null) ?? selected) : 0;
    if (number) setUndoing(number);
    else
      actions.tell("Choose a turn under Turns to put the files back as they were before it.", true);
  };
  const askAgain = () => setAgain((count) => count + 1);
  const move = (by: number) => {
    if (ids.length === 0) return;
    const at = Math.min(ids.length - 1, Math.max(0, ids.indexOf(selected ?? "") + by));
    setSelection(ids[at] ?? null);
    focusSelected();
  };
  const round = (by: 1 | 2) => changeView(VIEWS[(VIEWS.indexOf(view) + by) % VIEWS.length] ?? view);
  useKeys({
    Escape: toTerminal,
    h: () => setOpened(null),
    ArrowLeft: () => setOpened(null),
    Backspace: () => setOpened(null),
    ArrowDown: () => move(1),
    j: () => move(1),
    ArrowUp: () => move(-1),
    k: () => move(-1),
    PageDown: () => move(PAGE),
    PageUp: () => move(-PAGE),
    Home: () => move(-ids.length),
    End: () => move(ids.length),
    Enter: () => open(selected),
    ArrowRight: () => open(selected),
    l: () => open(selected),
    Tab: () => round(1),
    "⇧Tab": () => round(2),
  });
  useActionKeys("changes", {
    close_changes: toTerminal,
    leave_session: toTerminal,
    changes_again: askAgain,
    put_back: putBack,
    changes_help: () => actions.openPalette(true, "changes"),
    // Every screen's, the changes' too: they hold the keys from the window's own.
    fold_list: () => actions.foldSidebar(),
  });
  const map = useKeymap();
  const againKey = labelOf(map, "changes_again");
  const putBackKey = labelOf(map, "put_back");
  const closeKey = labelOf(map, "close_changes");
  const clashes = useDaemon((state) => state.workspaces[workspace]?.clashes.length ?? 0);
  useEffect(() => {
    const hints: Hint[] = [
      { key: "↑↓", does: view === "turns" ? "turns" : "files" },
      { key: "⏎", does: "diff" },
      ...(shown ? [{ key: "n p", does: "next and last hunk" }] : []),
      { key: "⇥", does: "not committed, branch, turns" },
      { key: againKey, does: "ask git again" },
      ...(clashes > 0 ? [{ key: clashes > 1 ? `1–${clashes}` : "1", does: "the other work" }] : []),
      ...(view === "turns" ? [{ key: putBackKey, does: "put back" }] : []),
      { key: closeKey, does: "back to the terminal" },
    ];
    // A key the person took from its action is no hint.
    actions.setHints(hints.filter((hint) => hint.key !== "none"));
    return () => actions.setHints([]);
  }, [actions, view, shown, clashes, againKey, putBackKey, closeKey]);

  const total = files.reduce(
    (sum, found) => ({
      added: sum.added + (found.lines?.added ?? 0),
      removed: sum.removed + (found.lines?.removed ?? 0),
    }),
    { added: 0, removed: 0 },
  );
  const command: Command | null =
    shown === null
      ? null
      : view === "turns"
        ? { EntryDiff: { workspace, number: Number(shown) } }
        : { FileDiff: { workspace, part: view, path: shown } };
  return (
    <section className="changes" aria-label="Changes">
      <div className="changes-toolbar">
        <Segmented<View>
          label="Which changes"
          segments={[
            { value: "uncommitted", label: `Not committed ${changes?.uncommitted.length ?? ""}` },
            { value: "branch", label: `On the branch ${changes?.branch.length ?? ""}` },
            {
              value: "turns",
              label: `Turns ${timeline.state === "ready" ? kept.entries.length : ""}`,
            },
          ]}
          value={view}
          onChange={changeView}
        />
        {view !== "turns" && files.length > 0 ? <DiffStat {...total} files={files.length} /> : null}
        {view === "turns" && selected ? (
          <Button
            size="sm"
            title={`Put back as before this turn (${putBackKey})`}
            onClick={putBack}
          >
            Put back
          </Button>
        ) : null}
      </div>
      <ClashBanners workspace={workspace} />
      <div className="changes-content">
        <div className="changes-list-column">
          <ChangesList
            view={view}
            listing={listing}
            timeline={timeline}
            rows={rows}
            selected={selected}
            onSelect={(id) => {
              // The dialog hands the focus back to the row it left; the new entry stays selected.
              if (newEntry.current !== null) return;
              setSelection(id);
              setKeysInDiff(false);
            }}
            onOpen={open}
            onAgain={askAgain}
            onDrawn={focusWanted}
          />
        </div>
        {command ? (
          <DiffPanel
            command={command}
            revision={String(again)}
            takeFocus={keysInDiff}
            onLeave={focusList}
            onFocus={() => setKeysInDiff(true)}
            head={
              entry ? (
                <>
                  <span className="mu-mono mu-cut changes-diff-path">
                    {entryLabel(kept, entry)}
                  </span>
                  {entry.summary ? (
                    <DiffStat added={entry.summary.added} removed={entry.summary.removed} />
                  ) : (
                    <span className="mu-quiet">changed no files</span>
                  )}
                </>
              ) : (
                <FileHead path={file?.path ?? shown ?? ""} kind={file?.kind} part={PART[view]}>
                  {file?.lines ? (
                    <DiffStat {...file.lines} />
                  ) : (
                    <span className="mu-quiet">binary</span>
                  )}
                </FileHead>
              )
            }
          />
        ) : (
          <div className="mu-diffview ui-diff changes-pick">
            <p className="mu-empty-text">
              {view === "turns"
                ? "Choose a turn to see what it changed."
                : "Choose a file to see its diff."}
            </p>
          </div>
        )}
      </div>
      {undoing !== null ? (
        <PutBack
          workspace={workspace}
          number={undoing}
          timeline={kept}
          onClose={() => setUndoing(null)}
          onDone={(made) => {
            newEntry.current = String(made);
            setUndoing(null);
            setOpened(null);
            setSelection(String(made));
            setAgain((count) => count + 1);
          }}
        />
      ) : null}
    </section>
  );
}

function FileHead({
  path,
  kind,
  part,
  children,
}: {
  path: string;
  kind: Parameters<typeof fileKind>[0] | undefined;
  part: string;
  children: ReactNode;
}) {
  const { dir, name } = splitPath(oneLine(path));
  const [letter, tone] = kind ? fileKind(kind) : ["", "M"];
  return (
    <>
      <span className={`mu-file-kind mu-file-kind--${tone} mu-mono`} aria-hidden="true">
        {letter}
      </span>
      <span className="mu-mono mu-cut changes-diff-path" title={path}>
        <span className="mu-quiet">{dir}</span>
        {name}
      </span>
      {children}
      <span className="mu-quiet">{part}</span>
    </>
  );
}
