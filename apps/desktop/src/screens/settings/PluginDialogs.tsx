// Adding, updating and removing a plugin (board: PluginApprove,
// `view::plugin_dialog`): where to add one from, a wait while it is
// fetched, which the person can give up; then everything it would run, for
// the person to say yes or no (`PRD-15`, `PRD-01`); or why it could not be
// fetched. Removing one asks first.

import { useState } from "react";
import { useActions, useBridge } from "../../app/context";
import type { PluginPreview } from "../../ipc/generated/PluginPreview";
import { Banner, BannerBody } from "../../ui/Banner";
import { Button, Spinner } from "../../ui/controls";
import { Dialog, DialogText } from "../../ui/Dialog";
import { ErrorText } from "../../ui/status";
import { TextField } from "../../ui/TextField";
import { type PluginDialog, useSettings } from "./context";
import { PluginFacts } from "./PluginFacts";
import { isFolder, missingPrograms, sourceText } from "./pluginWords";
import { oneLine } from "./words";

function AddPlugin({ dialog }: { dialog: Extract<PluginDialog, { step: "add" }> }) {
  const { actions } = useSettings();
  const [address, setAddress] = useState(dialog.address);
  const [gitRef, setGitRef] = useState(dialog.gitRef);
  const folder = isFolder(address.trim());
  const fetch = () => actions.fetchPreview(address, folder ? "" : gitRef);
  const cancel = () => actions.setDialog(null);
  return (
    <Dialog
      title="Add a plugin"
      onClose={cancel}
      onAccept={fetch}
      actions={
        <>
          <Button variant="secondary" size="lg" title="Cancel (⎋)" onClick={cancel}>
            Cancel
          </Button>
          <Button variant="primary" size="lg" title="Fetch it (⏎)" onClick={fetch}>
            Fetch and show what it runs
          </Button>
        </>
      }
    >
      <TextField
        label="A git repository's address, or a folder's path"
        value={address}
        onChange={setAddress}
        placeholder="https://github.com/you/your-plugin"
        hint="To install from: https://, ssh://, user@host:path or file://. Or a folder's path, from /, to link where it is, for a plugin you write."
        error={dialog.refused ? oneLine(dialog.refused) : null}
        data-dialog-initial
      />
      <TextField
        label="Branch or tag"
        value={folder ? "" : gitRef}
        onChange={setGitRef}
        placeholder={folder ? "None: a folder is read as it is" : "Empty for its default branch"}
        disabled={folder}
      />
      <DialogText>
        Nothing of it runs yet: Muster fetches it, reads what it would run, and shows you before
        anything is added.
      </DialogText>
    </Dialog>
  );
}

function Fetching({ dialog }: { dialog: Extract<PluginDialog, { step: "fetching" }> }) {
  const { actions } = useSettings();
  const stop = () => actions.setDialog(null);
  return (
    <Dialog
      title={dialog.adding ? "Add a plugin" : "Update a plugin"}
      onClose={stop}
      onAccept={() => {}}
      actions={
        <Button
          variant="secondary"
          size="lg"
          title="Stop waiting (⎋)"
          data-dialog-initial
          onClick={stop}
        >
          Stop waiting
        </Button>
      }
    >
      <DialogText busy>
        <Spinner /> Fetching {dialog.what}…
      </DialogText>
      <DialogText className="mu-quiet">
        Nothing of it runs: Muster reads what it would run, and shows you before anything is added.
        git may take up to three minutes.
      </DialogText>
    </Dialog>
  );
}

/** The preview's question: what saying yes does. */
function question(preview: PluginPreview): string {
  const name = oneLine(preview.details.name);
  if (preview.kind === "install") return `Install ${name}?`;
  if (preview.kind === "update") return `Update ${name}?`;
  return preview.current ? `Approve ${name} again?` : `Link ${name}?`;
}

function answer(preview: PluginPreview): string {
  if (preview.kind === "install") return "Install and turn on";
  if (preview.kind === "update") return "Update";
  return preview.current ? "Approve again" : "Link and turn on";
}

/** Where it is from, and where it goes. */
function from(preview: PluginPreview): string {
  const source = preview.source;
  if (preview.kind === "link" && typeof source === "object" && "folder" in source) {
    return `the folder ${oneLine(source.folder.path)}, linked where it is and read from there, never written to`;
  }
  const commit = typeof source === "object" && "git" in source ? source.git.commit.slice(0, 7) : "";
  return `${sourceText(source)}, into Muster's data folder, plugins/${oneLine(preview.id)}/${oneLine(commit)}/, yours alone`;
}

/** What it is now, for an update or a folder linked again. */
function now(preview: PluginPreview): string | null {
  const current = preview.current;
  if (!current) return null;
  const version = current.details ? `${oneLine(current.details.version)} ` : "";
  const manifest =
    current.details?.sha256 === preview.details.sha256
      ? "the same manifest"
      : "a manifest that is not this one";
  return `${version}from ${sourceText(current.source)}, with ${manifest}`;
}

function Approve({ preview }: { preview: PluginPreview }) {
  const { actions } = useSettings();
  const ui = useActions();
  const details = preview.details;
  const missing = missingPrograms(details);
  const was = now(preview);
  const yes = () => actions.installPreview(preview);
  const no = () => {
    actions.setDialog(null);
    ui.tell(
      preview.kind !== "update" && !preview.current
        ? "Nothing was added."
        : `${oneLine(preview.id)} stays as it was.`,
    );
  };
  const done = preview.kind === "update" ? "updated" : preview.current ? "approved" : "added";
  const after =
    preview.kind === "update"
      ? "Updating keeps it on or off, as it is."
      : preview.current
        ? "Approving keeps it on or off, as it is."
        : "Adding it turns it on.";
  return (
    <Dialog
      title={question(preview)}
      meta={details.description ? oneLine(details.description) : undefined}
      width="wide"
      onClose={no}
      onAccept={yes}
      onKeyDown={(event) => {
        const mods = event.metaKey || event.ctrlKey || event.altKey;
        if (mods || (event.key !== "y" && event.key !== "n")) return;
        event.preventDefault();
        if (event.key === "y") yes();
        else no();
      }}
      actions={
        <>
          <Button variant="secondary" size="lg" title="Cancel (n)" onClick={no}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="lg"
            title={`${answer(preview)} (y)`}
            data-dialog-initial
            onClick={yes}
          >
            {answer(preview)}
          </Button>
        </>
      }
    >
      <DialogText>
        Everything it will run is below, as its manifest says. Nothing of it runs until you say yes.
      </DialogText>
      <PluginFacts details={details} from={from(preview)}>
        {was ? (
          <>
            <dt>Now</dt>
            <dd>{was}</dd>
          </>
        ) : null}
      </PluginFacts>
      {missing.length > 0 ? (
        <DialogText className="app-settings-untold">
          It needs programs that are not on PATH: it can be {done}, but they are yours to get.
        </DialogText>
      ) : null}
      <Banner tone="note">
        <BannerBody>
          {details.hooks.length > 0
            ? "A hook only hears. It cannot start, prompt or type into an agent. "
            : ""}
          If its manifest changes, it is turned off until you approve it again. {after}
        </BannerBody>
      </Banner>
    </Dialog>
  );
}

function Failed({ dialog }: { dialog: Extract<PluginDialog, { step: "failed" }> }) {
  const { actions } = useSettings();
  const close = () => actions.setDialog(null);
  return (
    <Dialog
      title={dialog.title}
      onClose={close}
      onAccept={close}
      actions={
        <Button variant="secondary" size="lg" title="Close (⎋)" data-dialog-initial onClick={close}>
          Close
        </Button>
      }
    >
      <DialogText>It could not be fetched:</DialogText>
      <ErrorText>{oneLine(dialog.message)}</ErrorText>
      <DialogText className="mu-quiet">Nothing was changed.</DialogText>
    </Dialog>
  );
}

function Remove({ dialog }: { dialog: Extract<PluginDialog, { step: "remove" }> }) {
  const { actions } = useSettings();
  const bridge = useBridge();
  const ui = useActions();
  const id = oneLine(dialog.id);
  const keep = () => actions.setDialog(null);
  const remove = () => {
    actions.setDialog(null);
    bridge.command({ Plugin: { Remove: { id: dialog.id } } }).then(
      () => ui.tell(`Removed ${id}.`),
      (error: unknown) => ui.tell(error instanceof Error ? error.message : String(error), true),
    );
  };
  return (
    <Dialog
      title={`Remove ${id}?`}
      onClose={keep}
      onAccept={remove}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            title="Keep it (⎋)"
            data-dialog-initial
            onClick={keep}
          >
            Keep it
          </Button>
          <Button variant="danger" size="lg" title="Remove it (⌘⏎)" onClick={remove}>
            Remove {id}
          </Button>
        </>
      }
    >
      <DialogText>{dialog.question}</DialogText>
    </Dialog>
  );
}

export function PluginDialogs() {
  const { state } = useSettings();
  const dialog = state.dialog;
  if (!dialog) return null;
  switch (dialog.step) {
    case "add":
      return <AddPlugin key={`${dialog.address}-${dialog.refused}`} dialog={dialog} />;
    case "fetching":
      return <Fetching dialog={dialog} />;
    case "preview":
      return <Approve preview={dialog.preview} />;
    case "failed":
      return <Failed dialog={dialog} />;
    case "remove":
      return <Remove dialog={dialog} />;
  }
}
