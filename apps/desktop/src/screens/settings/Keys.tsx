// Settings › Keys (board: SettingsKeys, `app::rebind`): every key Muster
// answers, by what it does, under the screen it works on, then the panels
// of the plugins that run under each plugin's name. ⏎ or a click waits for
// the next key pressed and sets it, refused where the terminal UI refuses
// it (`keys::Keymap::check`); ⌫ puts an action's keys back, ⇧R every key.
// The keys are kept in Muster's own settings (`[keys]`), which the
// terminal UI reads too.

import { useEffect, useRef } from "react";
import { useActions } from "../../app/context";
import type { Preferences } from "../../ipc/generated/Preferences";
import { pressedChord, writtenChord } from "../../keys/chord";
import { checkKey, isSet, refusalText, warningsOf, warningText } from "../../keys/keymap";
import { useKeymap } from "../../keys/useKeymap";
import { Button, Kbd } from "../../ui/controls";
import { useSettings } from "./context";
import { type KeyEntry, keyEntries, narrowed } from "./keyRows";
import { rowKey } from "./model";
import { Footnote, Page, SettingRow } from "./Row";

/** This window's own keys, the same on every screen: set in no file, so never the person's. */
const WINDOW_KEYS: readonly { what: string; keys: readonly string[] }[] = [
  { what: "Go to a session or run a command", keys: ["⌘K"] },
  { what: "Needs you, All sessions, Tickets", keys: ["⌘1", "⌘2", "⌘3"] },
  { what: "Settings", keys: ["⌘,"] },
  { what: "Fold the sidebar", keys: ["⌘\\"] },
];

const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Fn"]);

const ALL_BACK = "Every key is back as Muster has it.";

function KeyItem({ entry, suggested }: { entry: KeyEntry; suggested: boolean }) {
  const { state, actions } = useSettings();
  const ui = useActions();
  const map = useKeymap();
  const row = rowKey("keys", `key.${entry.name}`);
  const waiting = state.waiting?.name === entry.name;
  const busy = state.busy.has(row);
  const start = () => {
    if (busy) return;
    actions.wait({ name: entry.name, refused: null });
    ui.tell(`Press the key for: ${entry.what}.`);
  };
  return (
    <>
      <SettingRow
        id={row}
        press="acts"
        className={`mu-keyrow ${waiting ? "is-waiting" : ""}`}
        act={start}
        back={() => {
          if (!isSet(map, entry.name)) return;
          actions.preference(
            row,
            { Keys: { action: entry.name, keys: null } },
            "Its keys are back as Muster has them.",
          );
        }}
      >
        <span className="mu-keyrow-what">
          {entry.what}
          {entry.set ? <span className="mu-label">yours</span> : null}
          {suggested ? <span className="mu-label">its suggestion</span> : null}
        </span>
        <button
          type="button"
          className="app-keyrow-set"
          aria-label={`Set the key for ${entry.what}, now ${entry.labels.join(" ") || "none"}`}
          onClick={start}
        >
          <span className="mu-keyrow-keys">
            {waiting ? (
              <>
                <span className="mu-keyrow-wait">Press a key…</span>
                <span className="mu-meta">⎋ cancels</span>
              </>
            ) : entry.labels.length > 0 ? (
              entry.labels.map((label) => <Kbd key={label}>{label}</Kbd>)
            ) : (
              <span className="mu-quiet">none</span>
            )}
          </span>
        </button>
      </SettingRow>
      {waiting && state.waiting?.refused ? (
        <div className="mu-keyrow-error" role="alert">
          {state.waiting.refused}
        </div>
      ) : null}
    </>
  );
}

/** While a row waits, the next key pressed is its own: nothing else in the window hears it. */
function useKeyCapture() {
  const { state, actions } = useSettings();
  const ui = useActions();
  const map = useKeymap();
  const name = state.waiting?.name ?? null;
  useEffect(() => {
    if (!name) return;
    const capture = (event: KeyboardEvent) => {
      if (MODIFIERS.has(event.key)) return;
      event.preventDefault();
      event.stopPropagation();
      const refuse = (text: string) => actions.wait({ name, refused: text });
      if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.metaKey) {
        actions.wait(null);
        ui.tell("Its keys stay as they are.");
        return;
      }
      if (event.metaKey) {
        refuse("⌘ keys are this window's own. Press a key with ⌃ or ⌥, or none.");
        return;
      }
      const chord = pressedChord(event);
      if (!chord) {
        refuse("That key cannot be one of Muster's. Press another key.");
        return;
      }
      const refused = checkKey(map, name, chord);
      if (refused) {
        refuse(refusalText(map, refused));
        return;
      }
      actions.wait(null);
      actions.preference(
        rowKey("keys", `key.${name}`),
        { Keys: { action: name, keys: [writtenChord(chord)] } },
        "The key is set.",
      );
    };
    window.addEventListener("keydown", capture, true);
    return () => window.removeEventListener("keydown", capture, true);
  }, [name, map, actions, ui]);
}

export function Keys({ preferences }: { preferences: Preferences }) {
  const { state, actions, meta } = useSettings();
  const map = useKeymap();
  const find = useRef<HTMLInputElement>(null);
  useKeyCapture();
  const entries = narrowed(keyEntries(map), state.find);
  const warnings = warningsOf(map);
  const groups: { title: string; entries: KeyEntry[] }[] = [];
  for (const entry of entries) {
    const last = groups[groups.length - 1];
    if (last?.title === entry.group) last.entries.push(entry);
    else groups.push({ title: entry.group, entries: [entry] });
  }
  const suggestedPanels = new Set(
    map.panels.filter((panel) => !panel.set && panel.chords.length > 0).map((panel) => panel.name),
  );
  const toFirst = () => {
    const first = entries[0];
    if (!first) return;
    meta.focusFollows.current = true;
    actions.select(rowKey("keys", `key.${first.name}`));
  };
  return (
    <Page
      title="Keys"
      lead="Every key Muster answers, by what it does. ⏎ on one waits for the key you press next; ⌫ puts it back. Moving about and ⏎ stay as they are."
      aside={
        <Button
          variant="quiet"
          title="Put every key back (⇧R)"
          onClick={() => actions.preference("keys/all_back", "AllKeysBack", ALL_BACK)}
        >
          Put every key back
        </Button>
      }
    >
      <SettingRow
        id={rowKey("keys", "find")}
        press="acts"
        className="app-keys-find"
        act={() => find.current?.focus()}
      >
        <input
          ref={find}
          className="mu-field-input app-keys-find-input"
          aria-label="Find an action or a key"
          placeholder="Find an action or a key"
          value={state.find}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => actions.setFind(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown" || event.key === "Enter") {
              event.preventDefault();
              toFirst();
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              if (state.find) actions.setFind("");
              else find.current?.blur();
            }
          }}
        />
      </SettingRow>

      {state.find.trim() === "" ? (
        <section className="mu-settings-group" aria-labelledby="keys-window">
          <h3 className="mu-settings-group-title" id="keys-window">
            This window
          </h3>
          {WINDOW_KEYS.map((key) => (
            <div
              key={key.what}
              className="mu-keyrow app-setting--still"
              title="This window's own key: the same on every screen"
            >
              <span className="mu-keyrow-what">{key.what}</span>
              <span className="mu-keyrow-keys">
                {key.keys.map((label) => (
                  <Kbd key={label}>{label}</Kbd>
                ))}
              </span>
            </div>
          ))}
        </section>
      ) : null}

      {groups.map((group) => (
        <section key={group.title} className="mu-settings-group" aria-label={group.title}>
          <h3 className="mu-settings-group-title">{group.title}</h3>
          {group.entries.map((entry) => (
            <KeyItem key={entry.name} entry={entry} suggested={suggestedPanels.has(entry.name)} />
          ))}
        </section>
      ))}
      {entries.length === 0 ? (
        <p className="app-settings-loading">No key does “{state.find.trim()}”.</p>
      ) : null}

      {warnings.map((warning) => (
        <p
          key={warningText(warning)}
          className="mu-footnote app-settings-footnote app-settings-footnote--warn"
        >
          {warningText(warning)}.
        </p>
      ))}
      {map.problem ? (
        <p className="mu-footnote app-settings-footnote app-settings-footnote--warn">
          {map.problem}: Muster's own keys hold until it reads.
        </p>
      ) : null}

      <Footnote>
        Saved in <code>{preferences.file}</code>, under <code>[keys]</code>. The terminal UI reads
        the same keys. In a terminal every key goes to its program but those that leave it or fold
        the list.
      </Footnote>
    </Page>
  );
}
