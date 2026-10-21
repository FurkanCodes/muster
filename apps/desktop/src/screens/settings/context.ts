// What every part of the Settings screen reaches: its rows and the one
// selected, what is on its way to the daemon, each project's settings as
// last read, and the ways to change them (composition:
// state-context-interface). The provider (SettingsProvider.tsx) is the
// only place that knows how this is kept.

import { createContext, type RefObject, use } from "react";
import type { PluginPreview } from "../../ipc/generated/PluginPreview";
import type { PreferenceChange } from "../../ipc/generated/PreferenceChange";
import type { ProjectId } from "../../ipc/generated/ProjectId";
import type { ProjectSettings } from "../../ipc/generated/ProjectSettings";
import type { SettingChange } from "../../ipc/generated/SettingChange";
import type { Row } from "./model";

/** What a row does on the screen's keys: ⏎ or space, ← →. */
export interface RowBehaviour {
  /** ⏎, space, or a press on the row outside its controls. */
  act?: (how: "enter" | "space" | "click") => void;
  step?: (by: number) => void;
  /** ⌫ on the row: its keys put back, on Settings › Keys. */
  back?: () => void;
}

/** A project's settings as last read. */
export type Read =
  | { state: "loading" }
  | { state: "loaded"; settings: ProjectSettings }
  | { state: "failed"; error: string };

/** Where a project's tickets are to come from, as chosen and typed but not yet kept. */
export interface Draft {
  tracker: string | null;
  /** What was typed for each tracker's fields, by tracker and key, kept as the choice goes round. */
  typed: Readonly<Record<string, Readonly<Record<string, string>>>>;
}

/** The steps of adding or updating a plugin, and removing one (`app::plugins::PluginDialog`). */
export type PluginDialog =
  | { step: "add"; address: string; gitRef: string; refused: string | null }
  | { step: "fetching"; what: string; adding: { address: string; gitRef: string } | null }
  | { step: "preview"; preview: PluginPreview }
  | { step: "failed"; title: string; message: string }
  | { step: "remove"; id: string; question: string };

export interface SettingsState {
  rows: readonly Row[];
  selected: Row | undefined;
  /** Rows whose change is on its way: each takes no second press until the answer. */
  busy: ReadonlySet<string>;
  projects: ReadonlyMap<ProjectId, Read>;
  drafts: ReadonlyMap<ProjectId, Draft>;
  /** What Settings › Keys is narrowed to. */
  find: string;
  /** The action whose key Settings › Keys waits for, and why the last one pressed was refused. */
  waiting: { name: string; refused: string | null } | null;
  dialog: PluginDialog | null;
}

export interface SettingsActions {
  select(key: string): void;
  /** Selects the row `by` rows on, through every page. */
  move(by: number): void;
  /** Makes `change` in Muster's own settings for `row`, saying `said` once it is made. */
  preference(row: string, change: PreferenceChange, said: string | null): void;
  /** Makes `change` in a project's settings, saying what `said` makes of them after. */
  setting(
    project: ProjectId,
    row: string,
    change: SettingChange,
    said: (settings: ProjectSettings) => string,
  ): void;
  setDraft(project: ProjectId, draft: Draft | null): void;
  /** `r`: reads Muster's settings, its plugins and every project's again. */
  readAgain(): void;
  setFind(text: string): void;
  /** Waits for the key `name` is to have, or stops waiting. */
  wait(waiting: { name: string; refused: string | null } | null): void;
  /** `a`: where to add a plugin from. */
  addPlugin(): void;
  /** `u`: what the selected plugin is now, fetched. */
  updatePlugin(): void;
  /** `x`: asks, then removes the selected plugin. */
  removePlugin(): void;
  /** Turns a plugin on or off, refused with why where it cannot run. */
  togglePlugin(id: string): void;
  setDialog(dialog: PluginDialog | null): void;
  /** Where the add dialog's ⏎ goes: the preview fetched, and the dialog waits. */
  fetchPreview(address: string, gitRef: string): void;
  installPreview(preview: PluginPreview): void;
  /** What `key`'s row does on the screen's keys, while it is shown. */
  behaviour(key: string): RowBehaviour | undefined;
}

export interface SettingsValue {
  state: SettingsState;
  actions: SettingsActions;
  meta: {
    /** Set as the keys move the selection while a control has the focus: the new row takes it. */
    focusFollows: RefObject<boolean>;
    /** What each row shown does on the screen's keys, by its key. */
    registry: RefObject<Map<string, RefObject<RowBehaviour>>>;
  };
}

export const SettingsContext = createContext<SettingsValue | null>(null);

export function useSettings(): SettingsValue {
  const value = use(SettingsContext);
  if (!value) throw new Error("useSettings() outside <SettingsProvider>");
  return value;
}
