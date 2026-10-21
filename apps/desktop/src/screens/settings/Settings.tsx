// Settings (PRODUCT.md §6 Settings, boards SettingsGeneral … SettingsProject):
// Muster's own sections, then each project's, beside the page shown. ↑↓
// go through every setting in order, page to page; ⏎ or space turns the
// selected one, opens its choices or does it; ← → step its choice; the
// person's keys for reading the files again and adding, updating and
// removing plugins work anywhere on the screen, as in the terminal UI.

import { type KeyboardEvent as ReactKeyboardEvent, type ReactNode, useEffect } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useDaemon } from "../../app/context";
import { type KeyHandlers, useKeys } from "../../app/keys";
import type { Preferences } from "../../ipc/generated/Preferences";
import { chordsOf, labelOf } from "../../keys/keymap";
import { useActionKeys, useKeymap } from "../../keys/useKeymap";
import { projectColour, projectsInOrder } from "../../state/selectors";
import type { Hint } from "../../state/ui";
import { Agents } from "./Agents";
import { OWN_AGENTS } from "./agentList";
import { useSettings } from "./context";
import { General } from "./General";
import { Keys } from "./Keys";
import {
  firstRow,
  MUSTER_SECTIONS,
  projectOf,
  projectSection,
  rowName,
  type Section,
} from "./model";
import { Naming } from "./Naming";
import { Notifications } from "./Notifications";
import { PluginDialogs } from "./PluginDialogs";
import { Plugins } from "./Plugins";
import { Project } from "./Project";
import { SettingsProvider } from "./SettingsProvider";
import "./settings.css";

export default function Screen() {
  return (
    <SettingsProvider>
      <SettingsScreen />
    </SettingsProvider>
  );
}

/** A control's own ⏎ and space are its own: the selected row hears neither. */
function keepControlKeys(event: ReactKeyboardEvent) {
  const target = event.target as HTMLElement;
  if (event.key !== "Enter" && event.key !== " ") return;
  if (target.closest("button, input, textarea, select, a[href]")) event.stopPropagation();
}

function SettingsScreen() {
  const { state } = useSettings();
  useSettingsKeys();
  useSettingsHints();
  const section = state.selected?.section ?? "general";
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: it only keeps a focused control's ⏎ and space from the screen's own keys.
    <div className="app-settings" data-settings onKeyDown={keepControlKeys}>
      <Sections shown={section} />
      <div className="mu-pane app-settings-page">
        <ShownPage section={section} />
      </div>
      <PluginDialogs />
    </div>
  );
}

function SectionItem({
  id,
  current,
  children,
}: {
  id: Section;
  current: boolean;
  children: ReactNode;
}) {
  const { state, actions } = useSettings();
  return (
    <button
      type="button"
      className="mu-nav-item"
      aria-current={current ? "page" : undefined}
      onClick={() => {
        const first = firstRow(state.rows, id);
        if (first) actions.select(first.key);
      }}
    >
      {children}
    </button>
  );
}

function ProjectItem({ id, current }: { id: string; current: boolean }) {
  const name = useDaemon((daemon) => daemon.projects[id]?.name ?? "");
  const colour = useDaemon((daemon) => projectColour(daemon, id));
  return (
    <SectionItem id={projectSection(id)} current={current}>
      <span className="mu-nav-icon">
        <span className={`mu-swatch mu-p${colour}`} />
      </span>
      <span className="app-settings-project">{name}</span>
    </SectionItem>
  );
}

function Sections({ shown }: { shown: Section }) {
  const projects = useDaemon(
    useShallow((daemon) => projectsInOrder(daemon).map((project) => project.id)),
  );
  const agents = useDaemon(
    (daemon) => OWN_AGENTS.length + (daemon.preferences?.agents.length ?? 0),
  );
  const plugins = useDaemon((daemon) => daemon.plugins.length);
  const counts: Partial<Record<Section, number>> = { agents, plugins };
  return (
    <section className="app-settings-sections" aria-label="Settings sections">
      <div className="mu-queue-head" data-tauri-drag-region>
        <h1 className="mu-queue-title">Settings</h1>
        <span className="mu-meta">The terminal UI shares them.</span>
      </div>
      <div className="app-settings-nav">
        <div className="mu-nav-label">Muster</div>
        {MUSTER_SECTIONS.map((section) => (
          <SectionItem key={section.id} id={section.id} current={shown === section.id}>
            <span className="mu-nav-icon" />
            {section.title}
            {counts[section.id] ? <span className="mu-nav-count">{counts[section.id]}</span> : null}
          </SectionItem>
        ))}
        {projects.length > 0 ? <div className="mu-nav-label">Projects</div> : null}
        {projects.map((id) => (
          <ProjectItem key={id} id={id} current={shown === projectSection(id)} />
        ))}
      </div>
    </section>
  );
}

function MusterPage({ section, preferences }: { section: Section; preferences: Preferences }) {
  switch (section) {
    case "notifications":
      return <Notifications preferences={preferences} />;
    case "agents":
      return <Agents preferences={preferences} />;
    case "naming":
      return <Naming preferences={preferences} />;
    case "plugins":
      return <Plugins />;
    case "keys":
      return <Keys preferences={preferences} />;
    default:
      return <General preferences={preferences} />;
  }
}

function ShownPage({ section }: { section: Section }) {
  const preferences = useDaemon((daemon) => daemon.preferences);
  const project = projectOf(section);
  if (project) return <Project project={project} />;
  if (!preferences) {
    const title =
      MUSTER_SECTIONS.find((candidate) => candidate.id === section)?.title ?? "Settings";
    return (
      <div className="mu-settings" aria-busy="true">
        <div className="mu-settings-head">
          <h2 className="mu-settings-title">{title}</h2>
          <span className="mu-meta">Reading Muster's own settings…</span>
        </div>
      </div>
    );
  }
  return <MusterPage section={section} preferences={preferences} />;
}

function useSettingsKeys() {
  const { state, actions } = useSettings();
  const view = useActions();
  const map = useKeymap();
  const selected = state.selected;
  const behaviour = () => (selected ? actions.behaviour(selected.key) : undefined);
  const back = () => view.show("all");
  useActionKeys("settings", {
    move_up: () => actions.move(-1),
    move_down: () => actions.move(1),
    read_settings: actions.readAgain,
    add_plugin: actions.addPlugin,
    update_plugin: actions.updatePlugin,
    remove_plugin: actions.removePlugin,
    back,
  });
  const fixed: Record<string, (event: KeyboardEvent) => void> = {
    ArrowUp: () => actions.move(-1),
    ArrowDown: () => actions.move(1),
    Home: () => actions.move(-state.rows.length),
    End: () => actions.move(state.rows.length),
    Enter: () => behaviour()?.act?.("enter"),
    " ": () => behaviour()?.act?.("space"),
    ArrowLeft: () => behaviour()?.step?.(-1),
    ArrowRight: () => behaviour()?.step?.(1),
  };
  if (selected?.kind === "key") {
    // ⌫ puts the selected key back; ⇧R every key (`app::rebind`).
    const backs = chordsOf(map, "back").some(
      (chord) => "named" in chord.key && chord.key.named === "backspace",
    );
    fixed.Backspace = () => {
      const row = behaviour();
      if (row?.back) row.back();
      else if (backs) back();
    };
    fixed.R = () =>
      actions.preference("keys/all_back", "AllKeysBack", "Every key is back as Muster has it.");
  }
  useKeys(fixed as KeyHandlers);
}

/** What the selected row's keys do, for the status bar. */
function hintsFor(
  kind: string | undefined,
  name: string,
  builtin: boolean,
  key: (action: string) => string,
): Hint[] {
  const move = { key: "↑↓", does: "settings" };
  const read = { key: key("read_settings"), does: "read the files again" };
  switch (kind) {
    case "switch":
      return [move, { key: "⏎", does: "turn on or off" }, read];
    case "select":
      return [move, { key: "⏎", does: "choose" }, { key: "←→", does: "change" }, read];
    case "segmented":
      return [move, { key: "←→", does: "change" }, read];
    case "button":
      return [move, { key: "⏎", does: BUTTON_HINTS[name] ?? "do it" }, read];
    case "field":
      return [move, { key: "⏎", does: name === "find" ? "find" : "keep it" }, read];
    case "plugin":
      return [
        { key: "␣", does: "turn on or off" },
        ...(builtin
          ? []
          : [
              { key: key("update_plugin"), does: "update" },
              { key: key("remove_plugin"), does: "remove" },
            ]),
        { key: key("add_plugin"), does: "add" },
      ];
    case "key":
      return [
        { key: "⏎", does: "set its key" },
        { key: "⌫", does: "put it back" },
        { key: "⇧R", does: "every key back" },
      ];
    default:
      return [move, read];
  }
}

const BUTTON_HINTS: Readonly<Record<string, string>> = {
  panels: "put back as at first",
  read_again: "read again",
  add_plugin: "add a plugin",
  remove: "remove the project",
};

function useSettingsHints() {
  const { state } = useSettings();
  const view = useActions();
  const map = useKeymap();
  const selected = state.selected;
  const name = selected ? rowName(selected.key) : "";
  const builtin = useDaemon(
    (daemon) =>
      name.startsWith("plugin.") &&
      daemon.plugins.find((plugin) => plugin.id === name.slice("plugin.".length))?.source ===
        "builtin",
  );
  const shown = JSON.stringify(
    hintsFor(selected?.kind, name, builtin, (action) => labelOf(map, action)),
  );
  useEffect(() => {
    view.setHints(JSON.parse(shown) as Hint[]);
  }, [view, shown]);
  useEffect(() => () => view.setHints([]), [view]);
}
