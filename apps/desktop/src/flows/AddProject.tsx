// Adding a project (`a`), as the TUI's picker does: the path field is also a
// folder browser. Typing a name chooses the first folder it matches, ↑↓
// another, ⇥ goes into the chosen folder (or the first shown), ⇧⇥ up a
// folder, and ⏎ adds the chosen folder, or the path as typed. The folders
// come from the app's own `list_folders`; the daemon's AddProject adds it.

import { type KeyboardEvent, useEffect, useId, useState } from "react";
import { useActions, useApp } from "../app/context";
import type { Folders } from "../ipc/generated/desktop/Folders";
import { Button } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import { FolderOption } from "../ui/FolderOption";
import { Label } from "../ui/Label";
import { ErrorText } from "../ui/status";
import { TextField } from "../ui/TextField";
import { addProjectAt, said } from "./actions";
import { root, shown, split, step, up } from "./folders";
import { found } from "./match";
import { useTask } from "./useTask";
import "./flows.css";

/** The folder typed so far, listed: its folders, or why not; null while asked. */
function useListing(folder: string) {
  const app = useApp();
  const [listing, setListing] = useState<{
    folder: string;
    folders?: Folders;
    error?: string;
  } | null>(null);
  useEffect(() => {
    if (!folder) return;
    let current = true;
    app.bridge.listFolders(folder).then(
      (folders) => {
        if (current) setListing({ folder, folders });
      },
      (error: unknown) => {
        if (current) setListing({ folder, error: said(error) });
      },
    );
    return () => {
      current = false;
    };
  }, [app, folder]);
  return listing?.folder === folder ? listing : null;
}

export default function AddProject() {
  const app = useApp();
  const actions = useActions();
  const [typed, setTyped] = useState("~/");
  const [at, setAt] = useState<number | null>(null);
  const [folder, name] = split(typed);
  const listing = useListing(folder);
  const folders = listing?.folders ? shown(listing.folders.folders, name) : [];
  const chosen = at === null ? null : (folders[at] ?? null);
  const listed = listing?.folders?.path ?? null;
  const task = useTask();
  const id = useId();
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const type = (next: string) => {
    setTyped(next);
    // A typed name chooses the first folder it matches.
    setAt(split(next)[1] ? 0 : null);
  };
  const adding = chosen?.name ?? (root(typed, listed, null).split("/").at(-1) || "the folder");
  const submit = () =>
    task.run(async () => {
      await addProjectAt(app, root(typed, listed, chosen));
      actions.openFlow(null);
    });

  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (!(event.target instanceof HTMLInputElement)) return;
    const by = { ArrowDown: 1, ArrowUp: -1, PageDown: 8, PageUp: -8 }[event.key];
    if (by !== undefined) {
      event.preventDefault();
      setAt(step(at, by, folders.length));
    } else if (event.key === "Tab" && !event.shiftKey) {
      event.preventDefault();
      const into = chosen ?? folders[0];
      if (into) type(`${folder}${into.name}/`);
    } else if (event.key === "Tab") {
      event.preventDefault();
      type(up(folder, listed));
    }
  };

  return (
    <Dialog
      title="Add a project"
      meta="A git repository gets worktrees, tickets and pull requests. A plain folder gets sessions in the folder itself, and the rest once it is a repository."
      width="wide"
      onClose={close}
      onAccept={() => void submit()}
      onKeyDown={onKeyDown}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            title="Cancel (⎋)"
            disabled={task.busy}
            onClick={close}
          >
            Cancel
          </Button>
          <Button
            variant="primary"
            size="lg"
            title={`Add ${adding} (⏎)`}
            busy={task.busy}
            disabled={!typed.trim()}
            onClick={() => void submit()}
          >
            {task.busy ? "Adding…" : `Add ${adding}`}
          </Button>
        </>
      }
    >
      <TextField
        label="Folder"
        mono
        role="combobox"
        aria-expanded="true"
        aria-controls={`${id}-folders`}
        aria-autocomplete="list"
        aria-activedescendant={chosen ? `${id}-${chosen.name}` : undefined}
        autoComplete="off"
        spellCheck={false}
        data-dialog-initial
        value={typed}
        onChange={type}
        error={task.error}
        hint="⇥ fills in the chosen folder. Folders starting with a dot show once you type one."
      >
        <div
          id={`${id}-folders`}
          role="listbox"
          aria-label={`Folders in ${folder || typed}`}
          className="ui-flow-folders"
        >
          {folders.map((entry, index) => (
            <FolderOption
              key={entry.path}
              id={`${id}-${entry.name}`}
              selected={index === at}
              onChoose={() => setAt(index)}
              onOpen={() => type(`${folder}${entry.name}/`)}
            >
              <Name name={entry.name} typed={name} />
              {entry.git ? <Label>git</Label> : null}
            </FolderOption>
          ))}
        </div>
        {!folder ? (
          <span className="mu-meta">Start the path with / or ~/.</span>
        ) : !listing ? (
          <span className="mu-meta" role="status">
            Reading {folder}…
          </span>
        ) : listing.error ? (
          <ErrorText size="meta">{listing.error}</ErrorText>
        ) : folders.length === 0 ? (
          <span className="mu-meta">
            {name ? `No folder in ${folder} holds “${name}”.` : `${folder} holds no folders.`} ⏎
            adds the path as typed.
          </span>
        ) : null}
      </TextField>
    </Dialog>
  );
}

function Name({ name, typed }: { name: string; typed: string }) {
  const at = typed ? found(name, typed) : null;
  if (at === null) return <span className="mu-cut">{name}</span>;
  return (
    <span className="mu-cut">
      {name.slice(0, at)}
      <mark>{name.slice(at, at + typed.length)}</mark>
      {name.slice(at + typed.length)}
    </span>
  );
}
