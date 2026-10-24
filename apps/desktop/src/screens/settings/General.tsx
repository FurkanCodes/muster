// Settings › General (board: SettingsGeneral): how Muster looks and lays
// itself out. The theme is this window's own; the rest are Muster's own
// settings, which the terminal UI shares.

import { useState } from "react";
import { useActions, useBridge, useDaemon, useUi } from "../../app/context";
import type { Palette } from "../../ipc/generated/Palette";
import type { Preferences } from "../../ipc/generated/Preferences";
import type { SessionLines } from "../../ipc/generated/SessionLines";
import { useKeyLabel } from "../../keys/useKeymap";
import { errorText } from "../../lib/text";
import { chats } from "../../state/selectors";
import type { Theme } from "../../state/ui";
import { Button, Segmented } from "../../ui/controls";
import { Select } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { canAsk } from "./asks";
import { useSettings } from "./context";
import { rowKey } from "./model";
import {
  Control,
  Footnote,
  Group,
  openSelect,
  Page,
  SettingRow,
  SettingText,
  Unreadable,
} from "./Row";
import { SIDE_BY_SIDE_CHOICES, sideBySideLabel, sideBySideSaid, stepSideBySide } from "./words";

/** How the panels are laid out at first (`layout::DEFAULT`). */
export const FIRST_LAYOUT = "list | session 60% | (changes / shell / plugin)";

const THEMES: readonly { value: Theme; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "Follow the Mac" },
];

const THEME_SAID: Readonly<Record<Theme, string>> = {
  dark: "This window is dark.",
  light: "This window is light.",
  system: "This window follows the Mac, light or dark.",
};

const LINES: readonly { value: SessionLines; label: string }[] = [
  { value: "two", label: "Two" },
  { value: "one", label: "One" },
];

const LINES_SAID: Readonly<Record<SessionLines, string>> = {
  two: "Each session takes two lines.",
  one: "Each session takes one line.",
};

const COLOURS: readonly { value: Palette; label: string }[] = [
  { value: "muster", label: "Muster's own" },
  { value: "terminal", label: "The terminal's own" },
];

const COLOURS_SAID: Readonly<Record<Palette, string>> = {
  muster: "Muster draws in its own colours.",
  terminal: "Muster draws in your terminal's colours.",
};

/** The widths offered, and one written by hand among them where it is none of Muster's. */
function widths(now: number) {
  const choices = SIDE_BY_SIDE_CHOICES.includes(now)
    ? SIDE_BY_SIDE_CHOICES
    : [...SIDE_BY_SIDE_CHOICES, now].sort((a, b) => a - b);
  return choices.map((columns) => ({ value: String(columns), label: sideBySideLabel(columns) }));
}

/** The one of `list` `by` steps on from `now`, no further than either end. */
function stepped<T>(list: readonly { value: T }[], now: T, by: number): T {
  const at = list.findIndex((item) => item.value === now);
  const next = list[Math.min(list.length - 1, Math.max(0, at + by))];
  return next ? next.value : now;
}

/** The one after `now`, round. */
function next<T>(list: readonly { value: T }[], now: T): T {
  const at = list.findIndex((item) => item.value === now);
  return (list[(at + 1) % list.length] as { value: T }).value;
}

export function General({ preferences }: { preferences: Preferences }) {
  const ui = useActions();
  const { state, actions } = useSettings();
  const theme = useUi((window) => window.theme);
  const version = useDaemon((daemon) => (daemon.link.state === "up" ? daemon.link.version : null));
  const sessions = useDaemon((daemon) => chats(daemon).length);
  const sharedBuilds = useDaemon((daemon) => daemon.link.state === "up");
  const readKey = useKeyLabel("read_settings");
  const bridge = useBridge();
  const [restarting, setRestarting] = useState(false);
  const restart = () => {
    if (!canAsk(bridge)) return ui.tell("This window cannot restart the daemon.", true);
    if (restarting) return;
    setRestarting(true);
    ui.tell("Restarting the daemon in place…");
    bridge
      .restartInPlace()
      .then(
        (said) => ui.tell(said || "The daemon restarted in place."),
        (error: unknown) => ui.tell(errorText(error), true),
      )
      .finally(() => setRestarting(false));
  };
  const row = (name: string) => rowKey("general", name);
  const busy = (name: string) => state.busy.has(row(name));

  const setTheme = (value: Theme) => {
    ui.setTheme(value);
    ui.tell(THEME_SAID[value]);
  };
  const setLines = (value: SessionLines) => {
    if (value !== preferences.session_lines) {
      actions.preference(row("session_lines"), { SessionLines: value }, LINES_SAID[value]);
    }
  };
  const setColours = (value: Palette) =>
    actions.preference(row("colours"), { Palette: value }, COLOURS_SAID[value]);
  const setWidth = (columns: number) => {
    if (columns !== preferences.side_by_side_from) {
      actions.preference(row("side_by_side"), { SideBySideFrom: columns }, sideBySideSaid(columns));
    }
  };
  const toggleSharedBuilds = () =>
    actions.preference(
      row("shared_builds"),
      { SharedBuilds: !preferences.shared_builds },
      preferences.shared_builds
        ? "Each worktree builds into its own folder again; the shared builds are deleted."
        : "A project's worktrees build into one folder beside them.",
    );
  const putPanelsBack = () => {
    if (preferences.panels === FIRST_LAYOUT && !preferences.list_folded) {
      ui.tell("The panels are where they were at first.");
      return;
    }
    if (preferences.list_folded) actions.preference(row("folded"), { ListFolded: false }, null);
    actions.preference(
      row("panels"),
      { Panels: FIRST_LAYOUT },
      "The panels are back where they were at first.",
    );
  };

  return (
    <Page title="General" lead="How Muster looks and lays itself out.">
      {preferences.problem ? <Unreadable problem={preferences.problem} readKey={readKey} /> : null}
      <Group title="Look">
        <SettingRow
          id={row("theme")}
          act={() => setTheme(next(THEMES, theme))}
          step={(by) => setTheme(stepped(THEMES, theme, by))}
        >
          <SettingText
            name="Theme"
            label="this app"
            does="Dark is Muster's first theme. Terminals keep their own programs' colours either way."
          />
          <Control>
            <Segmented label="Theme" segments={THEMES} value={theme} onChange={setTheme} />
          </Control>
        </SettingRow>
        <SettingRow
          id={row("session_lines")}
          act={() => setLines(next(LINES, preferences.session_lines))}
          step={(by) => setLines(stepped(LINES, preferences.session_lines, by))}
        >
          <SettingText
            name="Lines for each session"
            does="Two fit a name with what it is doing under it. One fits twice as many."
          />
          <Control>
            <Segmented
              label="Lines for each session"
              segments={LINES}
              value={preferences.session_lines}
              onChange={setLines}
              busy={busy("session_lines")}
            />
          </Control>
        </SettingRow>
      </Group>

      <Group title="Layout">
        <SettingRow id={row("panels")} act={putPanelsBack}>
          <SettingText
            name="Panels"
            does={
              <>
                Now <code>{preferences.panels}</code>
                {preferences.list_folded ? ", the list folded away" : ""}. Drag a panel's title to
                move it, or the line between two to resize them.
              </>
            }
          />
          <Control>
            <Button busy={busy("panels")} onClick={putPanelsBack}>
              Put back as at first
            </Button>
          </Control>
        </SettingRow>
      </Group>

      <Group title="Terminal UI only">
        <SettingRow
          id={row("colours")}
          act={() => openSelect(row("colours"))}
          step={(by) => setColours(stepped(COLOURS, preferences.palette, by))}
        >
          <SettingText
            name="Colours"
            does="Muster's own are soft and the same in any terminal. The terminal's own sixteen follow its theme."
          />
          <Control>
            <Select
              label="Colours"
              size="md"
              value={preferences.palette}
              options={COLOURS}
              onChange={(value) => setColours(value as Palette)}
              onStep={(by) => setColours(stepped(COLOURS, preferences.palette, by))}
              disabled={busy("colours")}
            />
          </Control>
        </SettingRow>
        <SettingRow
          id={row("side_by_side")}
          act={() => openSelect(row("side_by_side"))}
          step={(by) => setWidth(stepSideBySide(preferences.side_by_side_from, by))}
        >
          <SettingText
            name="Session beside the list from"
            does="In a narrower terminal the list takes the window, and a session opens over it."
          />
          <Control>
            <Select
              label="Session beside the list from"
              size="md"
              value={String(preferences.side_by_side_from)}
              options={widths(preferences.side_by_side_from)}
              onChange={(value) => setWidth(Number(value))}
              onStep={(by) => setWidth(stepSideBySide(preferences.side_by_side_from, by))}
              disabled={busy("side_by_side")}
            />
          </Control>
        </SettingRow>
      </Group>

      {sharedBuilds ? (
        <Group title="Worktrees">
          <SettingRow id={row("shared_builds")} press="acts" act={toggleSharedBuilds}>
            <SettingText
              name="Shared Rust builds"
              does="The worktrees Muster makes for a project build into one folder beside them, so a Rust project's dependencies are built and kept once, not again in every worktree. Your repository and your own checkout are never touched. Builds share one lock: agents building in two worktrees at once take turns, one waiting while the other builds. Turned off, each builds on its own again and the shared builds are deleted."
            />
            <Control>
              <Switch
                on={preferences.shared_builds}
                label="Shared Rust builds"
                onChange={toggleSharedBuilds}
                busy={busy("shared_builds")}
              />
            </Control>
          </SettingRow>
        </Group>
      ) : null}

      <Group title="Muster">
        <SettingRow id={row("restart")} act={restart}>
          <SettingText
            name={version ? `Muster ${version}` : "Muster"}
            does={
              <>
                Its daemon holds {sessions} {sessions === 1 ? "session" : "sessions"}. Restarting in
                place moves it onto the Muster installed now, and every session runs on.
              </>
            }
          />
          <Control>
            <Button
              title="Restart in place, as muster reload does"
              disabled={restarting}
              busy={restarting}
              onClick={restart}
            >
              {restarting ? "Restarting…" : "Restart in place"}
            </Button>
          </Control>
        </SettingRow>
      </Group>

      <Footnote>
        Saved in <code>{preferences.file}</code>, Muster's own settings. Theme is kept by this app
        alone.
      </Footnote>
    </Page>
  );
}
