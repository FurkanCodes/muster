// Cloning a project from GitHub (`g`, the Projects `+`), as the TUI's
// dialog does (crates/muster-tui/src/app/cloning.rs): the repositories the
// person can reach, as the daemon lists them through their own `gh`,
// narrowed by what they type, or one they name (owner/name, or its
// github.com address). ⏎ asks where in the Mac's own folder panel, and the
// daemon clones it into a folder of its name there and adds it; where the
// app has no panel (its tests' build, the mock), a field asks instead.

import { type KeyboardEvent, useEffect, useId, useState } from "react";
import { useActions, useApp } from "../app/context";
import type { HostedRepository } from "../ipc/generated/HostedRepository";
import { whenKnown } from "../state/whenKnown";
import { Button } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import { FolderOption } from "../ui/FolderOption";
import { Label } from "../ui/Label";
import { ErrorText } from "../ui/status";
import { TextField } from "../ui/TextField";
import { said } from "./actions";
import { folderOf, repositoryName, shownRepositories } from "./repositories";
import { useTask } from "./useTask";
import "./flows.css";

type Listing =
  | { state: "asking" }
  | { state: "listed"; repositories: HostedRepository[] }
  | { state: "failed"; error: string };

/** The repositories the person can reach, asked of the daemon once as the dialog opens. */
function useListing(): Listing {
  const app = useApp();
  const [listing, setListing] = useState<Listing>({ state: "asking" });
  useEffect(() => {
    let current = true;
    app.bridge.command("ListRepositories").then(
      (reply) => {
        if (!current) return;
        if (typeof reply === "object" && "Repositories" in reply)
          setListing({ state: "listed", repositories: reply.Repositories });
        else setListing({ state: "failed", error: "The daemon answered something else." });
      },
      (error: unknown) => {
        if (current) setListing({ state: "failed", error: said(error) });
      },
    );
    return () => {
      current = false;
    };
  }, [app]);
  return listing;
}

export default function CloneProject() {
  const app = useApp();
  const actions = useActions();
  const id = useId();
  const listing = useListing();
  const [typed, setTyped] = useState("");
  const [at, setAt] = useState<number | null>(null);
  /** Where to clone it, typed, where the app has no panel to ask in. */
  const [into, setInto] = useState<string | null>(null);
  const task = useTask();
  const shown = listing.state === "listed" ? shownRepositories(listing.repositories, typed) : [];
  const chosen = at === null ? null : (shown[at] ?? null);
  const target = chosen?.name ?? repositoryName(typed);

  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const type = (next: string) => {
    setTyped(next);
    // A name typed chooses its first match.
    setAt(next.trim() ? 0 : null);
  };
  const cloneInto = async (name: string, folder: string) => {
    const reply = await app.bridge.command({ CloneProject: { repository: name, into: folder } });
    if (typeof reply !== "object" || !("ProjectAdded" in reply))
      throw new Error("The daemon answered something else; try again.");
    const project = reply.ProjectAdded.id;
    actions.openFlow(null);
    actions.show("all");
    whenKnown(app, "projects", project, () => {
      actions.select("all", project);
      actions.tell(`Cloned ${name}.`);
    });
  };
  const submit = () =>
    task.run(async () => {
      if (!target)
        throw new Error("Choose one, or type its name as owner/name or its github.com address.");
      if (into !== null) return cloneInto(target, into);
      const picked = await app.bridge.pickFolder(`Clone ${target} into`);
      if (picked.kind === "chosen") return cloneInto(target, picked.path);
      if (picked.kind === "unavailable") {
        // No panel here: a field asks, from the home folder.
        const home = await app.bridge.listFolders("~/");
        setInto(home.path);
      }
    });

  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (!(event.target instanceof HTMLInputElement) || into !== null) return;
    const by = { ArrowDown: 1, ArrowUp: -1 }[event.key];
    if (by === undefined || shown.length === 0) return;
    event.preventDefault();
    setAt((was) => {
      if (was === null) return by > 0 ? 0 : null;
      const next = was + by;
      return next < 0 ? null : Math.min(next, shown.length - 1);
    });
  };

  const start = !target ? "Clone" : into === null ? `Clone ${target}…` : `Clone ${target} here`;

  return (
    <Dialog
      title="Clone from GitHub"
      meta="Your repositories, through your own gh. It is cloned into a folder of its name where you choose, and added as a project."
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
            title={`${start} (⏎)`}
            busy={task.busy}
            onClick={() => void submit()}
          >
            {task.busy ? `Cloning ${target ?? ""}…` : start}
          </Button>
        </>
      }
    >
      <TextField
        label="Repository"
        mono
        role="combobox"
        aria-expanded="true"
        aria-controls={`${id}-repositories`}
        aria-autocomplete="list"
        aria-activedescendant={chosen ? `${id}-${chosen.name}` : undefined}
        autoComplete="off"
        spellCheck={false}
        placeholder="Search yours, or type owner/name"
        data-dialog-initial={into === null ? true : undefined}
        disabled={into !== null}
        value={typed}
        onChange={type}
        error={into === null ? task.error : null}
      >
        <div
          id={`${id}-repositories`}
          role="listbox"
          aria-label="Your repositories on GitHub"
          className="ui-flow-folders"
        >
          {shown.map((repository, index) => (
            <FolderOption
              key={repository.name}
              id={`${id}-${repository.name}`}
              selected={index === at}
              className="ui-flow-repository"
              onChoose={() => setAt(index)}
              onOpen={() => {
                setAt(index);
                void submit();
              }}
            >
              <span className="mu-mono mu-cut">{repository.name}</span>
              {repository.private ? <Label>private</Label> : null}
              {repository.description ? (
                <span className="mu-meta mu-cut">{repository.description}</span>
              ) : null}
            </FolderOption>
          ))}
        </div>
        {listing.state === "asking" ? (
          <span className="mu-meta" role="status">
            Asking GitHub…
          </span>
        ) : listing.state === "failed" ? (
          <ErrorText size="meta">{listing.error} ⏎ still clones one typed as owner/name.</ErrorText>
        ) : shown.length === 0 ? (
          <span className="mu-meta">None of yours matches; ⏎ clones one typed as owner/name.</span>
        ) : null}
      </TextField>
      {into !== null && target ? (
        <TextField
          label={`Clone into (as ${folderOf(target)}/ there)`}
          mono
          autoComplete="off"
          spellCheck={false}
          data-dialog-initial
          value={into}
          onChange={setInto}
          error={task.error}
        />
      ) : null}
    </Dialog>
  );
}
