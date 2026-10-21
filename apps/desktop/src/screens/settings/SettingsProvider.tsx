// The Settings screen's state and how it changes (state-decouple-
// implementation): the rows and the one selected (the window's own
// `selected.settings`), each project's settings read from the daemon as
// the screen is shown and on `r`, what is on its way, and the plugin
// dialogs. Every change is the daemon's own command, said as the terminal
// UI says it once it is answered (crates/muster-tui/src/app/settings.rs,
// plugins.rs, tickets_from.rs).

import { type ReactNode, type RefObject, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useBridge, useDaemon, useUi } from "../../app/context";
import type { Command } from "../../ipc/generated/Command";
import type { PluginInfo } from "../../ipc/generated/PluginInfo";
import type { PluginPreview } from "../../ipc/generated/PluginPreview";
import type { ProjectId } from "../../ipc/generated/ProjectId";
import { useKeymap } from "../../keys/useKeymap";
import { projectsInOrder } from "../../state/selectors";
import { agentIds } from "./agentList";
import {
  type Draft,
  type PluginDialog,
  type Read,
  type RowBehaviour,
  SettingsContext,
  type SettingsState,
  type SettingsValue,
} from "./context";
import { keyEntries, narrowed } from "./keyRows";
import { keptRow, movedRow, rowKey, rowName, settingRows } from "./model";
import { isFolder, pluginOrder } from "./pluginWords";
import { chosenTracker, fieldsFor, offeredTrackers } from "./trackers";
import { oneLine, problemText } from "./words";

const NO_AGENTS: never[] = [];

/** The message of a refusal, as the daemon wrote it. */
function said(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What approving a preview is doing, for the note meanwhile. */
function doing(preview: PluginPreview): string {
  if (preview.kind === "install") return "Installing";
  return preview.kind === "link" ? "Linking" : "Updating";
}

/** What approving a preview did. */
function done(preview: PluginPreview): string {
  const id = oneLine(preview.id);
  if (preview.kind === "install") return `Installed ${id} and turned it on.`;
  if (preview.kind === "update") return `Updated ${id} to ${oneLine(preview.details.version)}.`;
  return preview.current
    ? `Approved ${id} as its folder holds it now.`
    : `Linked ${id} and turned it on.`;
}

function runs(plugin: PluginInfo): boolean {
  return plugin.enabled && plugin.problem === null;
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const bridge = useBridge();
  const ui = useActions();
  const wanted = useUi((state) => state.selected.settings);
  const projectIds = useDaemon(
    useShallow((state) => projectsInOrder(state).map((project) => project.id)),
  );
  const added = useDaemon((state) => state.preferences?.agents ?? NO_AGENTS);
  const plugins = useDaemon((state) => state.plugins);
  const map = useKeymap();
  const [projects, setProjects] = useState<ReadonlyMap<ProjectId, Read>>(() => new Map());
  const [drafts, setDrafts] = useState<ReadonlyMap<ProjectId, Draft>>(() => new Map());
  const [busy, setBusy] = useState<ReadonlySet<string>>(() => new Set());
  const [find, setFind] = useState("");
  const [waiting, setWaiting] = useState<SettingsState["waiting"]>(null);
  const [dialog, setDialog] = useState<PluginDialog | null>(null);
  const registry = useRef(new Map<string, RefObject<RowBehaviour>>());
  const focusFollows = useRef(false);
  // The preview waited for: an answer to any other is dropped.
  const asked = useRef(0);

  const offered = offeredTrackers(plugins);
  const rows = settingRows({
    agents: agentIds(added),
    plugins: pluginOrder(plugins).map((plugin) => plugin.id),
    projects: projectIds.map((id) => {
      const read = projects.get(id);
      if (read?.state !== "loaded") return { id, loaded: false, fields: [], prefix: false };
      const draft = drafts.get(id);
      const fields = fieldsFor(offered, projects, id, chosenTracker(read, draft), draft);
      return {
        id,
        loaded: true,
        fields: fields.map((field) => field.setting.key),
        prefix: read.settings.muster_prefix !== null,
      };
    }),
    keys: narrowed(keyEntries(map), find).map((entry) => entry.name),
  });
  const selected = keptRow(rows, wanted);

  const send = (command: Command) => bridge.command(command);
  const fail = (error: unknown) => ui.tell(said(error), true);
  const hold = (row: string, held: boolean) =>
    setBusy((now) => {
      const next = new Set(now);
      if (held) next.add(row);
      else next.delete(row);
      return next;
    });

  // Showing the screen reads the plugins again, so a manifest edited by hand shows.
  useEffect(() => {
    void bridge.command({ Plugin: "List" }).catch(() => {});
  }, [bridge]);

  // Each project's settings, read as it is shown without them.
  useEffect(() => {
    const missing = projectIds.filter((id) => !projects.has(id));
    if (missing.length === 0) return;
    setProjects((now) => {
      const next = new Map(now);
      for (const id of missing) next.set(id, { state: "loading" });
      return next;
    });
    for (const project of missing) {
      bridge.command({ ProjectSettings: { project } }).then(
        (reply) => {
          if (typeof reply === "object" && "ProjectSettings" in reply) {
            const settings = reply.ProjectSettings;
            setProjects((now) => new Map(now).set(project, { state: "loaded", settings }));
          }
        },
        (error: unknown) =>
          setProjects((now) => new Map(now).set(project, { state: "failed", error: said(error) })),
      );
    }
  }, [bridge, projectIds, projects]);

  const selectedPlugin = (): PluginInfo | undefined => {
    const name = selected?.section === "plugins" ? rowName(selected.key) : "";
    return name.startsWith("plugin.")
      ? plugins.find((plugin) => plugin.id === name.slice("plugin.".length))
      : undefined;
  };

  const waitForPreview = (
    command: Command,
    waiting: Extract<PluginDialog, { step: "fetching" }>,
    refused: (message: string) => PluginDialog,
  ) => {
    asked.current += 1;
    const request = asked.current;
    setDialog(waiting);
    send(command).then(
      (reply) => {
        if (asked.current !== request) return;
        if (typeof reply === "object" && "PluginPreview" in reply) {
          setDialog({ step: "preview", preview: reply.PluginPreview });
        }
      },
      (error: unknown) => {
        if (asked.current === request) setDialog(refused(said(error)));
      },
    );
  };

  const value: SettingsValue = {
    state: { rows, selected, busy, projects, drafts, find, waiting, dialog },
    meta: { focusFollows, registry },
    actions: {
      select: (key) => {
        // A key waited for is the selected row's alone.
        if (waiting && key !== rowKey("keys", `key.${waiting.name}`)) setWaiting(null);
        ui.select("settings", key);
      },
      move: (by) => {
        const next = movedRow(rows, selected?.key, by);
        if (!next) return;
        const focus = document.activeElement;
        focusFollows.current = Boolean(
          focus && focus !== document.body && focus.closest("[data-settings]"),
        );
        ui.select("settings", next.key);
      },
      preference: (row, change, words) => {
        if (busy.has(row)) return;
        hold(row, true);
        send({ ChangePreference: change })
          .then(() => {
            if (words) ui.tell(words);
          }, fail)
          .finally(() => hold(row, false));
      },
      setting: (project, row, change, words) => {
        const read = projects.get(project);
        if (read?.state === "failed") {
          ui.tell(`Its settings cannot be read: ${read.error}`, true);
          return;
        }
        // Read or changed just now: the answer is on its way.
        if (read?.state !== "loaded" || busy.has(row)) return;
        hold(row, true);
        send({ ChangeSetting: { project, change } })
          .then(
            (reply) => {
              if (typeof reply === "object" && "ProjectSettings" in reply) {
                const settings = reply.ProjectSettings;
                setProjects((now) => new Map(now).set(project, { state: "loaded", settings }));
                setDrafts((now) => {
                  const next = new Map(now);
                  next.delete(project);
                  return next;
                });
                ui.tell(words(settings));
              }
            },
            (error: unknown) => {
              // Refused: said, and the settings read again, as they still are.
              setProjects((now) => {
                const next = new Map(now);
                next.delete(project);
                return next;
              });
              fail(error);
            },
          )
          .finally(() => hold(row, false));
      },
      setDraft: (project, draft) =>
        setDrafts((now) => {
          const next = new Map(now);
          if (draft) next.set(project, draft);
          else next.delete(project);
          return next;
        }),
      readAgain: () => {
        ui.tell("Reading the settings again…");
        setProjects(new Map());
        setDrafts(new Map());
        send("ReadPreferences").then(() => ui.tell("Read the settings again."), fail);
      },
      setFind,
      wait: setWaiting,
      addPlugin: () => setDialog({ step: "add", address: "", gitRef: "", refused: null }),
      updatePlugin: () => {
        const plugin = selectedPlugin();
        if (!plugin) {
          ui.tell("Select a plugin to update it.", true);
          return;
        }
        const id = oneLine(plugin.id);
        if (plugin.source === "builtin") {
          ui.tell(`${id} is Muster's own: it updates with Muster.`, true);
          return;
        }
        waitForPreview(
          { Plugin: { Update: { id: plugin.id } } },
          { step: "fetching", what: `${id} as it is now`, adding: null },
          (message) => ({ step: "failed", title: `Update ${id}`, message }),
        );
      },
      removePlugin: () => {
        const plugin = selectedPlugin();
        if (!plugin) {
          ui.tell("Select a plugin to remove it.", true);
          return;
        }
        const id = oneLine(plugin.id);
        if (plugin.source === "builtin") {
          ui.tell(`${id} is Muster's own: it cannot be removed, only turned off.`, true);
          return;
        }
        const question =
          "git" in plugin.source
            ? "Muster's copy of it and its entry in your settings go."
            : `Muster forgets it; its folder ${oneLine(plugin.source.folder.path)} stays as it is.`;
        setDialog({ step: "remove", id: plugin.id, question });
      },
      togglePlugin: (id) => {
        const plugin = plugins.find((candidate) => candidate.id === id);
        const changing = [...busy].some((row) => row.startsWith(rowKey("plugins", "plugin.")));
        if (!plugin || changing) return;
        const on = !runs(plugin);
        if (on && plugin.problem) {
          const why = problemText(plugin.problem).replace(/\.$/, "");
          ui.tell(`${oneLine(id)} cannot be turned on: ${why}.`, true);
          return;
        }
        const row = rowKey("plugins", `plugin.${id}`);
        hold(row, true);
        send({ Plugin: { SetEnabled: { id, on, by: "ui" } } })
          .then(() => ui.tell(`Turned ${oneLine(id)} ${on ? "on" : "off"}.`), fail)
          .finally(() => hold(row, false));
      },
      setDialog: (next) => {
        if (dialog?.step === "fetching" && next === null) {
          // Given up: the daemon lets the preview go in ten minutes.
          asked.current += 1;
          ui.tell(`Stopped waiting for ${dialog.what}.`);
        }
        setDialog(next);
      },
      fetchPreview: (address, gitRef) => {
        const typed = address.trim();
        const adding = { address, gitRef };
        if (!typed) {
          setDialog({
            step: "add",
            ...adding,
            refused: "Type a git repository's address, or a folder's path.",
          });
          return;
        }
        if (isFolder(typed) && !typed.startsWith("/")) {
          setDialog({
            step: "add",
            ...adding,
            refused:
              "Type the folder's whole path, from /: this window cannot tell where ~ or . is.",
          });
          return;
        }
        const origin = isFolder(typed)
          ? { Folder: { path: typed } }
          : { Git: { url: typed, git_ref: gitRef.trim() || null } };
        waitForPreview(
          { Plugin: { Preview: { origin } } },
          { step: "fetching", what: oneLine(typed), adding },
          (message) => ({ step: "add", ...adding, refused: message }),
        );
      },
      installPreview: (preview) => {
        setDialog(null);
        ui.tell(`${doing(preview)} ${oneLine(preview.id)}…`);
        send({ Plugin: { Install: { preview: preview.preview, by: "ui" } } }).then(() => {
          ui.tell(done(preview));
          ui.select("settings", rowKey("plugins", `plugin.${preview.id}`));
        }, fail);
      },
      behaviour: (key) => registry.current.get(key)?.current,
    },
  };
  return <SettingsContext value={value}>{children}</SettingsContext>;
}
