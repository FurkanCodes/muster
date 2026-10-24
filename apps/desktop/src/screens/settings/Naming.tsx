// Settings › Naming chats (board: SettingsNaming, `app::naming`): whether
// an agent names chats, which (each session's own, or one the person picks
// among those that can be asked one question), and its model, one the
// agent lists (`ListModels`, asked as the agent is picked) or one typed.

import { useEffect, useRef, useState } from "react";
import { useActions, useBridge } from "../../app/context";
import { replyAs } from "../../ipc/ask";
import type { Preferences } from "../../ipc/generated/Preferences";
import { agentOf } from "../../lib/agents";
import { errorText, oneLine } from "../../lib/text";
import { Detail, DetailList } from "../../ui/DetailList";
import { Select, type SelectOption } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { Input } from "../../ui/TextField";
import { namers, OWN_AGENTS } from "./agentList";
import { useSettings } from "./context";
import { rowKey } from "./model";
import { Control, Footnote, Group, openSelect, Page, SettingRow, SettingText } from "./Row";
import { listed } from "./words";

const OFF = "Chats keep their first prompt's first line: turn this on above.";
const PICK_FIRST = "Pick the agent that names chats first: each uses its own model.";

/** What the Select says for none: each session's own agent, or the agent's own model. */
const OWN = "";
/** The model's choice that turns it into a field, to type one. */
const TYPE = "\u0000type";

/** The models an agent was asked for, as they came. */
type Listed = { agent: string; models: string[] | null; error: string | null };

function ModelField({
  initial,
  onKeep,
  onGiveUp,
}: {
  initial: string;
  onKeep: (model: string | null) => void;
  onGiveUp: () => void;
}) {
  const [typed, setTyped] = useState(initial);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, []);
  return (
    <Input
      ref={field}
      className="app-settings-field app-settings-field--wide"
      aria-label="Its model, typed"
      placeholder="Type a model, then ⏎"
      value={typed}
      autoComplete="off"
      spellCheck={false}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={onGiveUp}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          onKeep(typed.trim() || null);
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onGiveUp();
        }
      }}
    />
  );
}

export function Naming({ preferences }: { preferences: Preferences }) {
  const { state, actions } = useSettings();
  const bridge = useBridge();
  const ui = useActions();
  const naming = preferences.naming;
  const row = (name: string) => rowKey("naming", name);
  const [listedModels, setListed] = useState<Listed | null>(null);
  const [typing, setTyping] = useState(false);
  const choices = namers(preferences.agents, (id) => agentOf(id).name);
  const off = naming.by_agent ? undefined : OFF;
  const agent = naming.agent;

  // The models the agent picked lists, asked as it is picked.
  useEffect(() => {
    if (agent === null) return;
    setListed({ agent, models: null, error: null });
    bridge.command({ ListModels: { agent } }).then(
      (reply) => {
        const models = replyAs(reply, "Models");
        if (models) setListed({ agent, models, error: null });
      },
      (error: unknown) =>
        setListed({
          agent,
          models: [],
          error: errorText(error),
        }),
    );
  }, [agent, bridge]);

  const toggle = () =>
    actions.preference(
      row("names_by_agent"),
      { NamesByAgent: !naming.by_agent },
      naming.by_agent
        ? "Chats keep the first line of their first prompt."
        : "An agent names new chats.",
    );
  const chooseNamer = (chosen: string | null) => {
    if (chosen !== agent) {
      actions.preference(
        row("named_by"),
        { NamedBy: { agent: chosen, model: null } },
        "Chats are named by the agent shown.",
      );
    }
  };
  const stepNamer = (step: number) => {
    const at = choices.findIndex((choice) => choice.id === agent);
    const next = choices[Math.min(choices.length - 1, Math.max(0, at + step))];
    if (next) chooseNamer(next.id);
  };
  const chooseModel = (chosen: string | null) => {
    setTyping(false);
    if (agent === null || chosen === naming.model) return;
    actions.preference(
      row("naming_model"),
      { NamedBy: { agent, model: chosen } },
      "Chats are named with the model shown.",
    );
  };
  const openModels = () => {
    if (agent === null) ui.tell(PICK_FIRST, true);
    else openSelect(row("naming_model"));
  };

  const shown = listedModels?.agent === agent ? listedModels : null;
  const listedNames = shown?.models ?? [];
  const modelOptions: SelectOption[] = [
    { value: OWN, label: agent ? "Its own choice" : "Each agent's own" },
    ...listedNames.map((name) => ({ value: name, label: oneLine(name) })),
  ];
  if (naming.model !== null && !listedNames.includes(naming.model)) {
    modelOptions.push({ value: naming.model, label: oneLine(naming.model), note: "typed" });
  }
  modelOptions.push({
    value: TYPE,
    label: "Type one…",
    note: shown?.error ? shown.error : shown && shown.models === null ? "asking…" : undefined,
    apart: true,
  });
  const named = choices.filter((choice) => choice.id !== null).map((choice) => choice.label);
  const unchecked = OWN_AGENTS.filter((own) => !own.names).map((own) => agentOf(own.id).name);
  const modelOff = off ?? (agent === null ? PICK_FIRST : undefined);

  return (
    <Page
      title="Naming chats"
      lead="A chat is named at once by its first prompt's first line. A few seconds later an agent can name it in two to four words. A name you give it is never replaced."
    >
      <Group title="Named by an agent">
        <SettingRow id={row("names_by_agent")} press="acts" act={toggle}>
          <SettingText
            name="Chats named by an agent"
            does="Asks the agent you pick, with your own sign-in and a little of your plan each time. It sends only the first prompt, with credentials hidden, and keeps nothing in that agent's history."
          />
          <Control>
            <Switch
              on={naming.by_agent}
              label="Chats named by an agent"
              onChange={toggle}
              busy={state.busy.has(row("names_by_agent"))}
            />
          </Control>
        </SettingRow>
        <SettingRow
          id={row("named_by")}
          level="sub"
          off={off}
          act={() => openSelect(row("named_by"))}
          step={stepNamer}
        >
          <SettingText
            name="Named by"
            does="Each session's own agent, or one you pick of those that can answer one question."
          />
          <Control>
            <span className="app-settings-select--wide" title={off}>
              <Select
                label="Named by"
                size="md"
                value={agent ?? OWN}
                options={choices.map((choice) => ({
                  value: choice.id ?? OWN,
                  label: choice.label,
                }))}
                onChange={(value) => chooseNamer(value === OWN ? null : value)}
                onStep={stepNamer}
                disabled={Boolean(off) || state.busy.has(row("named_by"))}
              />
            </span>
          </Control>
        </SettingRow>
        <SettingRow id={row("naming_model")} level="sub" off={off} act={openModels}>
          <SettingText
            name="Its model"
            does="One the agent lists, or type one. A small model answers fast and costs little."
          />
          <Control>
            {typing ? (
              <ModelField
                initial={naming.model ?? ""}
                onKeep={chooseModel}
                onGiveUp={() => setTyping(false)}
              />
            ) : (
              <span className="app-settings-select--wide" title={modelOff}>
                <Select
                  label="Its model"
                  size="md"
                  value={naming.model ?? OWN}
                  options={modelOptions}
                  onChange={(value) => {
                    if (value === TYPE) setTyping(true);
                    else chooseModel(value === OWN ? null : value);
                  }}
                  disabled={Boolean(modelOff) || state.busy.has(row("naming_model"))}
                />
              </span>
            )}
          </Control>
        </SettingRow>
      </Group>

      <Group title="How a name is made">
        <div className="app-settings-example">
          <DetailList>
            <Detail name="The prompt">
              <span className="mu-mono">
                Sketch the Tauri shell for Muster Desktop: a crate that connects through
                muster-client and forwards the state to the webview.
              </span>
            </Detail>
            <Detail name="At once">Sketch the Tauri shell for Muster Desktop: a crate that…</Detail>
            <Detail name="A few seconds later">
              <span className="app-settings-strong">Sketch the Tauri shell</span>
            </Detail>
          </DetailList>
          <span className="mu-meta">
            Where the prompt is only a few words, the start of the agent's first answer goes too,
            and the chat is named once more as that turn ends.
          </span>
        </div>
      </Group>

      <Group title="Can name">
        <Footnote tone="text">
          {listed(named)}. {listed(unchecked)} are asked once each is checked.
        </Footnote>
      </Group>

      <Footnote>
        Saved in <code>{preferences.file}</code>, under <code>[names]</code>.
      </Footnote>
    </Page>
  );
}
