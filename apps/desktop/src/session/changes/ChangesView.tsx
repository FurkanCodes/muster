import { useEffect, useRef, useState } from "react";
import { useActions, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import type { Changes } from "../../ipc/generated/Changes";
import type { Timeline } from "../../ipc/generated/Timeline";
import type { WorkspaceId } from "../../ipc/generated/WorkspaceId";
import { Button, Segmented } from "../../ui/controls";
import { FileRow } from "../../ui/FileRow";
import { DiffStat } from "../../ui/status";
import { TurnRow } from "../../ui/TurnRow";
import { ClashBanners } from "./ClashBanners";
import { DiffPanel } from "./DiffPanel";
import { entryLabel } from "./labels";
import { PutBack } from "./PutBack";
import { useReply } from "./useReply";
import "./changes.css";

type View = "uncommitted" | "branch" | "turns";
const VIEWS: View[] = ["uncommitted", "branch", "turns"];
const EMPTY_TIMELINE: Timeline = { entries: [] };
const EMPTY_CHANGES: Changes = {
  base: null,
  uncommitted: [],
  branch: [],
  commits: [],
  more_commits: 0,
};

export default function ChangesView({ workspace }: { workspace: WorkspaceId }) {
  const target = useUi((state) =>
    state.changesTurn?.workspace === workspace ? state.changesTurn : null,
  );
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
  const summary = useDaemon((state) => state.workspaces[workspace]?.changes);
  const newest = useDaemon((state) => state.workspaces[workspace]?.timeline ?? null);
  const [again, setAgain] = useState(0);
  const [view, setView] = useState<View>(target ? "turns" : "uncommitted");
  const [selection, setSelection] = useState<string | null>(target ? String(target.number) : null);
  const [opened, setOpened] = useState<string | null>(
    target?.intent === "diff" ? String(target.number) : null,
  );
  const [undoTarget, setUndoTarget] = useState<number | null>(
    target?.intent === "put-back" ? target.number : null,
  );
  const list = useRef<HTMLDivElement>(null);
  const revision = `${JSON.stringify(summary)}:${newest}:${again}`;
  const listing = useReply({ WorkspaceChanges: { workspace } }, revision);
  const turns = useReply({ Timeline: { workspace } }, `${newest}:${again}`);
  const changes =
    listing.state === "ready" && typeof listing.value !== "string" && "Changes" in listing.value
      ? listing.value.Changes
      : EMPTY_CHANGES;
  const timeline =
    turns.state === "ready" && typeof turns.value !== "string" && "Timeline" in turns.value
      ? turns.value.Timeline
      : EMPTY_TIMELINE;
  const answer = view === "turns" ? turns : listing;
  const files = view === "turns" ? [] : changes[view];
  const ids =
    view === "turns"
      ? timeline.entries.map((entry) => String(entry.number))
      : files.map((file) => file.path);
  const selected = selection && ids.includes(selection) ? selection : (ids[0] ?? null);
  const shown = opened && ids.includes(opened) ? opened : null;
  const entry =
    view === "turns"
      ? timeline.entries.find((entry) => String(entry.number) === (shown ?? selected))
      : undefined;
  const file = files.find((file) => file.path === shown);
  const changeView = (next: View) => {
    setView(next);
    setSelection(null);
    setOpened(null);
  };
  const back = () => actions.showPane("terminal");
  const putBack = () => {
    const number = view === "turns" ? Number(shown ?? selected) : null;
    if (number) setUndoTarget(number);
    else
      actions.tell("Choose a turn under Turns to put the files back as they were before it.", true);
  };
  const move = (by: number) => {
    const next = Math.min(ids.length - 1, Math.max(0, ids.indexOf(selected ?? "") + by));
    const id = ids[next];
    if (!id) return;
    setSelection(id);
    const button = list.current?.querySelectorAll<HTMLButtonElement>('[role="option"]')[next];
    button?.focus();
    button?.scrollIntoView({ block: "nearest" });
  };
  useKeys({
    Escape: back,
    c: back,
    d: back,
    Backspace: () => setOpened(null),
    ArrowLeft: () => setOpened(null),
    h: () => setOpened(null),
    ArrowDown: () => move(1),
    j: () => move(1),
    ArrowUp: () => move(-1),
    k: () => move(-1),
    PageDown: () => move(10),
    PageUp: () => move(-10),
    Home: () => move(-ids.length),
    End: () => move(ids.length),
    Enter: () => setOpened(selected),
    ArrowRight: () => setOpened(selected),
    l: () => setOpened(selected),
    r: () => setAgain((count) => count + 1),
    z: putBack,
    u: putBack,
    Tab: () => changeView(VIEWS[(VIEWS.indexOf(view) + 1) % VIEWS.length] ?? "uncommitted"),
    "⇧Tab": () => changeView(VIEWS[(VIEWS.indexOf(view) + 2) % VIEWS.length] ?? "uncommitted"),
  });
  useEffect(() => {
    actions.setHints([
      { key: "↑↓", does: view === "turns" ? "turns" : "files" },
      { key: "⏎", does: "diff" },
      { key: "n p", does: "hunks" },
      { key: "⇥", does: "not committed, branch, turns" },
      { key: "r", does: "ask git again" },
      ...(view === "turns" ? [{ key: "z", does: "put back" }] : []),
      { key: "c", does: "terminal" },
    ]);
    return () => actions.setHints([]);
  }, [actions, view]);
  const total = files.reduce(
    (sum, file) => ({
      added: sum.added + (file.lines?.added ?? 0),
      removed: sum.removed + (file.lines?.removed ?? 0),
    }),
    { added: 0, removed: 0 },
  );
  return (
    <section className="changes" aria-label="Changes">
      <div className="changes-toolbar">
        <Segmented<View>
          label="Which changes"
          segments={[
            { value: "uncommitted", label: `Not committed ${changes.uncommitted.length}` },
            { value: "branch", label: `On the branch ${changes.branch.length}` },
            { value: "turns", label: `Turns ${timeline.entries.length}` },
          ]}
          value={view}
          onChange={changeView}
        />
        {view !== "turns" ? <DiffStat {...total} files={files.length} /> : null}
        {view === "turns" && selected ? (
          <Button title="Put back before this turn (z, u)" onClick={putBack}>
            Put back
          </Button>
        ) : null}
      </div>
      <ClashBanners workspace={workspace} />
      <div className="changes-content">
        <div className="changes-list-column">
          {answer.state === "loading" ? (
            <p className="mu-meta" role="status">
              {view === "turns" ? "Reading the turns…" : "Asking git…"}
            </p>
          ) : answer.state === "error" ? (
            <div role="alert">
              <p>
                {view === "turns" ? "The timeline could not be read: " : ""}
                {answer.error}
              </p>
              <Button title="Ask again (r)" onClick={() => setAgain((count) => count + 1)}>
                Ask again
              </Button>
            </div>
          ) : (
            <>
              {view === "branch" ? (
                <div className="mu-meta">
                  {changes.base === null ? (
                    "No branch to measure its commits against: set base_branch in muster.toml, or fetch the remote."
                  ) : (
                    <>
                      <p>
                        Commits since {changes.base} ·{" "}
                        {changes.commits.length + changes.more_commits}
                      </p>
                      {changes.commits.map((commit) => (
                        <p key={commit.id}>
                          <span className="mu-mono">{commit.id}</span> {commit.subject}
                        </p>
                      ))}
                      {changes.more_commits ? <p>and {changes.more_commits} older</p> : null}
                    </>
                  )}
                </div>
              ) : null}
              <div
                ref={list}
                role="listbox"
                aria-label={view === "turns" ? "Turns" : "Files"}
                className="changes-list"
              >
                {view === "turns"
                  ? timeline.entries.map((entry) => (
                      <TurnRow
                        key={entry.number}
                        entry={entry}
                        label={entryLabel(timeline, entry)}
                        selected={selected === String(entry.number)}
                        onSelect={() => setSelection(String(entry.number))}
                        onOpen={() => {
                          setSelection(String(entry.number));
                          setOpened(String(entry.number));
                        }}
                      />
                    ))
                  : files.map((file) => (
                      <FileRow
                        key={file.path}
                        file={file}
                        selected={selected === file.path}
                        onSelect={() => setSelection(file.path)}
                        onOpen={() => {
                          setSelection(file.path);
                          setOpened(file.path);
                        }}
                      />
                    ))}
              </div>
              {!ids.length ? (
                <p className="mu-meta">
                  {view === "turns"
                    ? "None kept yet: each agent's turn here is, from now on."
                    : view === "uncommitted"
                      ? "Nothing is waiting to be committed."
                      : `Nothing has changed here since ${changes.base ?? "its base"}.`}
                </p>
              ) : null}
              <p className="mu-meta">
                {view === "turns"
                  ? "The newest 100 entries are kept."
                  : "Muster only asks git; nothing here changes the worktree."}
              </p>
            </>
          )}
        </div>
        {shown ? (
          <DiffPanel
            command={
              view === "turns"
                ? { EntryDiff: { workspace, number: Number(shown) } }
                : { FileDiff: { workspace, part: view, path: shown } }
            }
            revision={revision}
            title={
              entry ? `#${entry.number} · ${entryLabel(timeline, entry)}` : (file?.path ?? shown)
            }
            part={
              view === "uncommitted"
                ? "Not committed"
                : view === "branch"
                  ? "On the branch"
                  : "Turns"
            }
          />
        ) : (
          <div className="mu-empty changes-diff">
            <p className="mu-empty-text">
              {view === "turns"
                ? "Choose a turn to see what it changed."
                : "Choose a file to see its diff."}
            </p>
          </div>
        )}
      </div>
      {undoTarget !== null ? (
        <PutBack
          workspace={workspace}
          number={undoTarget}
          timeline={timeline}
          onClose={() => setUndoTarget(null)}
          onDone={(entry) => {
            setUndoTarget(null);
            setOpened(null);
            setSelection(String(entry));
            setAgain((count) => count + 1);
          }}
        />
      ) : null}
    </section>
  );
}
