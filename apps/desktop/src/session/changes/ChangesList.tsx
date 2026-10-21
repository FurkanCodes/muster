// The list column of the changes view: the files not committed, the
// branch's commits and files, or the timeline's turns, each in its states
// (asking, refused, empty), in the TUI's words (app::changes::rows,
// app::timeline::rows).

import type { RefObject } from "react";
import type { Changes } from "../../ipc/generated/Changes";
import type { FileChange } from "../../ipc/generated/FileChange";
import type { Timeline } from "../../ipc/generated/Timeline";
import { Button } from "../../ui/controls";
import { FileRow } from "../../ui/FileRow";
import { TurnRow } from "../../ui/TurnRow";
import { commonFolder, entryLabel, oneLine } from "./labels";
import type { Answer } from "./useReply";

export type View = "uncommitted" | "branch" | "turns";

/** The words of an answer still on its way, or refused, with a way to ask again. */
function Waiting({
  answer,
  asking,
  refused = "",
  onAgain,
}: {
  answer: Answer<unknown>;
  asking: string;
  refused?: string;
  onAgain(): void;
}) {
  if (answer.state === "loading")
    return (
      <p className="mu-meta changes-note" role="status">
        {asking}
      </p>
    );
  if (answer.state !== "error") return null;
  return (
    <div className="changes-note" role="alert">
      <p className="mu-note--error">
        {refused}
        {answer.error}
      </p>
      <Button size="sm" title="Ask again (r)" onClick={onAgain}>
        Ask again
      </Button>
    </div>
  );
}

function plural(count: number, one: string): string {
  return `${count} ${one}${count === 1 ? "" : "s"}`;
}

/** Files in a listbox; `rows` is the list the keys move through. */
function Files({
  files,
  label,
  rows,
  selected,
  onSelect,
  onOpen,
}: {
  files: readonly FileChange[];
  label: string;
  rows: RefObject<HTMLDivElement | null>;
  selected: string | null;
  onSelect(path: string): void;
  onOpen(path: string): void;
}) {
  const folder = commonFolder(files.map((file) => file.path));
  return (
    <>
      <div ref={rows} role="listbox" aria-label={label} className="changes-list">
        {files.map((file) => (
          <FileRow
            key={file.path}
            file={file}
            folder={folder}
            selected={selected === file.path}
            onSelect={() => onSelect(file.path)}
            onOpen={() => onOpen(file.path)}
          />
        ))}
      </div>
      <p className="mu-meta changes-note">
        {folder ? (
          <>
            In <span className="mu-mono changes-folder">{oneLine(folder.slice(0, -1))}</span>.{" "}
          </>
        ) : null}
        Muster only asks git; nothing here changes the worktree.
      </p>
    </>
  );
}

export function ChangesList({
  view,
  listing,
  timeline,
  rows,
  selected,
  onSelect,
  onOpen,
  onAgain,
}: {
  view: View;
  listing: Answer<Changes>;
  timeline: Answer<Timeline>;
  rows: RefObject<HTMLDivElement | null>;
  selected: string | null;
  onSelect(id: string): void;
  onOpen(id: string): void;
  onAgain(): void;
}) {
  if (view === "turns") {
    if (timeline.state !== "ready")
      return (
        <Waiting
          answer={timeline}
          asking="Reading the turns…"
          refused="The timeline could not be read: "
          onAgain={onAgain}
        />
      );
    const kept = timeline.value;
    if (kept.entries.length === 0)
      return (
        <p className="mu-meta changes-note">
          None kept yet: each agent's turn here is, from now on.
        </p>
      );
    return (
      <>
        <div ref={rows} role="listbox" aria-label="Turns" className="changes-list">
          {kept.entries.map((entry) => (
            <TurnRow
              key={entry.number}
              entry={entry}
              label={entryLabel(kept, entry)}
              selected={selected === String(entry.number)}
              onSelect={() => onSelect(String(entry.number))}
              onOpen={() => onOpen(String(entry.number))}
            />
          ))}
        </div>
        <p className="mu-meta changes-note">Muster keeps each workspace's newest 100.</p>
      </>
    );
  }
  if (listing.state !== "ready")
    return <Waiting answer={listing} asking="Asking git…" onAgain={onAgain} />;
  const changes = listing.value;
  const base = changes.base ?? "its base";
  if (view === "uncommitted")
    return changes.uncommitted.length === 0 ? (
      <p className="mu-meta changes-note">Nothing here is waiting to be committed.</p>
    ) : (
      <Files
        files={changes.uncommitted}
        label="Files not committed"
        rows={rows}
        selected={selected}
        onSelect={onSelect}
        onOpen={onOpen}
      />
    );
  if (changes.base === null)
    return (
      <p className="mu-meta changes-note">
        No branch to measure its commits against: set base_branch in muster.toml, or fetch the
        remote.
      </p>
    );
  if (changes.commits.length === 0)
    return <p className="mu-meta changes-note">Nothing has changed here since {base}.</p>;
  return (
    <>
      <h3 className="meta-strong changes-label">
        Commits since {oneLine(base)} · {changes.commits.length + changes.more_commits}
      </h3>
      <ul className="changes-commits">
        {changes.commits.map((commit) => (
          <li key={commit.id} className="mu-cut">
            <span className="mu-mono mu-quiet">{oneLine(commit.id)}</span> {oneLine(commit.subject)}
          </li>
        ))}
        {changes.more_commits > 0 ? (
          <li className="mu-meta">and {changes.more_commits} older</li>
        ) : null}
      </ul>
      <h3 className="meta-strong changes-label">
        Changed on the branch · {plural(changes.branch.length, "file")}
      </h3>
      <Files
        files={changes.branch}
        label="Files on the branch"
        rows={rows}
        selected={selected}
        onSelect={onSelect}
        onOpen={onOpen}
      />
    </>
  );
}
