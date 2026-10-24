// Settings › Notifications (board: SettingsNotify): whether the person is
// told of what newly needs them while they look away, which kinds, each
// with its sound, heard as it is chosen; and the agents in the notch.

import { useActions, useBridge, useDaemon } from "../../app/context";
import type { Preferences } from "../../ipc/generated/Preferences";
import type { Sound } from "../../ipc/generated/Sound";
import type { Tell } from "../../ipc/generated/Tell";
import { errorText } from "../../lib/text";
import { Button } from "../../ui/controls";
import { Select, type SelectOption } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { Found } from "../../ui/status";
import { canAsk } from "./asks";
import { useSettings } from "./context";
import { useHear } from "./hear";
import { rowKey } from "./model";
import { Control, Footnote, Group, Page, SettingRow, SettingText } from "./Row";
import { KINDS, MAC_SOUNDS, soundLabel, soundSaid, stepSound } from "./words";

const OFF = "Notifications are off: turn them on above.";

/** The sounds offered, the Mac's own set apart, and one of the person's own where it is chosen. */
function sounds(chosen: Sound): SelectOption[] {
  const own: SelectOption[] =
    chosen === "default" || chosen === "none" || MAC_SOUNDS.includes(chosen)
      ? []
      : [{ value: chosen, label: chosen, note: "yours" }];
  return [
    { value: "none", label: soundLabel("none") },
    { value: "default", label: soundLabel("default") },
    ...MAC_SOUNDS.map((name, at) => ({ value: name, label: name, apart: at === 0 })),
    ...own,
  ];
}

function KindRow({
  kind,
  tell,
  told,
}: {
  kind: (typeof KINDS)[number];
  tell: Tell;
  /** Whether notifications are on, which each kind is under. */
  told: boolean;
}) {
  const { state, actions } = useSettings();
  const hear = useHear();
  const row = rowKey("notifications", `tell.${kind.kind}`);
  const busy = state.busy.has(row);
  const change = (next: Tell) =>
    actions.preference(
      row,
      { Tell: { kind: kind.kind, tell: next } },
      next.on
        ? `${kind.label} are told, with ${soundSaid(next.sound)}.`
        : `${kind.label} are not told.`,
    );
  // A sound chosen is told with, as ← → step it in the terminal UI.
  const choose = (sound: Sound) => change({ on: true, sound });
  const step = (by: number) => {
    if (busy) return;
    const sound = stepSound(tell.sound, by);
    choose(sound);
    hear(sound);
  };
  return (
    <SettingRow
      id={row}
      press="acts"
      level="sub"
      off={told ? undefined : OFF}
      act={() => change({ ...tell, on: !tell.on })}
      step={step}
    >
      <SettingText name={kind.label} does={kind.does} />
      <Control>
        <span title={told ? undefined : OFF}>
          <Select
            label={`Sound for ${kind.key}`}
            size="md"
            value={tell.sound}
            options={sounds(tell.sound)}
            onChange={choose}
            onActive={hear}
            onStep={step}
            disabled={!told || busy}
          />
        </span>
        <Switch
          on={tell.on}
          label={`Tell me of ${kind.key}`}
          onChange={(on) => change({ ...tell, on })}
          busy={busy}
          disabled={!told}
          title={told ? undefined : OFF}
        />
      </Control>
    </SettingRow>
  );
}

export function Notifications({ preferences }: { preferences: Preferences }) {
  const { state, actions } = useSettings();
  const notifier = useDaemon((daemon) => daemon.notifier);
  const row = (name: string) => rowKey("notifications", name);
  const told = preferences.notifications;
  const bridge = useBridge();
  const ui = useActions();
  const openSystemSettings = () => {
    if (!canAsk(bridge)) return ui.tell("This window cannot open System Settings.", true);
    bridge.openNotificationSettings().then(
      () => ui.tell("Opened System Settings at Muster's notifications."),
      (error: unknown) => ui.tell(errorText(error), true),
    );
  };

  const toggleTold = () =>
    actions.preference(
      row("notifications"),
      { Notifications: !told },
      told
        ? "Nothing is told: the waiting count and Needs you still say it."
        : "You are told of what newly needs you while you look away.",
    );
  const toggleNotch = () =>
    actions.preference(
      row("notch"),
      { Notch: !preferences.notch },
      preferences.notch
        ? "Muster leaves the notch alone."
        : "The notch shows your agents while you look elsewhere.",
    );

  return (
    <Page
      title="Notifications"
      lead="When an agent newly needs you while you look away from every Muster window. Once for each wait."
    >
      <Group title="On this Mac">
        <SettingRow id={row("notifications")} press="acts" act={toggleTold}>
          <SettingText
            name="Notifications"
            does="Sent by Muster's helper, so they come with every window closed. A click opens that session."
          />
          <Control>
            <Switch
              on={told}
              label="Notifications"
              onChange={toggleTold}
              busy={state.busy.has(row("notifications"))}
            />
          </Control>
        </SettingRow>
        {KINDS.map((kind) => (
          <KindRow key={kind.kind} kind={kind} tell={preferences.telling[kind.key]} told={told} />
        ))}
        <SettingRow
          id={row("system_settings")}
          off={told ? undefined : "Notifications are off above."}
          act={openSystemSettings}
        >
          {told ? (
            <SettingText
              name="Allowed in macOS"
              does={
                notifier
                  ? "macOS keeps this choice. Change it in System Settings, Notifications, Muster."
                  : "Not yet: macOS asks once, for Muster. Allow it later in System Settings, Notifications, Muster."
              }
            />
          ) : (
            <SettingText name="Allowed in macOS" does="Off above, so Muster's helper sends none." />
          )}
          {told ? (
            <Control>
              {notifier ? <Found>✓ allowed</Found> : <span className="mu-quiet">not yet</span>}
              <Button title="System Settings, Notifications, Muster" onClick={openSystemSettings}>
                Open System Settings
              </Button>
            </Control>
          ) : null}
        </SettingRow>
      </Group>

      <Group title="The notch">
        <SettingRow id={row("notch")} press="acts" act={toggleNotch}>
          <SettingText
            name="Agents in the notch"
            does="At the notch while you look away and an agent works or waits. It opens on its own for what is new."
          />
          <Control>
            <Switch
              on={preferences.notch}
              label="Agents in the notch"
              onChange={toggleNotch}
              busy={state.busy.has(row("notch"))}
            />
          </Control>
        </SettingRow>
      </Group>

      <Footnote>
        Saved in <code>{preferences.file}</code>, Muster's own settings, for this app and the
        terminal UI alike.
      </Footnote>
    </Page>
  );
}
