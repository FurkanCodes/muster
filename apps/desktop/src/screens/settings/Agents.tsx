// Settings › Agents (board: SettingsAgents): the coding agents Muster can
// start, its own and those the person added in Muster's own settings
// (`[agents.<name>]`), written by hand, so the page says what does not
// read and reads the file again on the person's word.

import type { AddedAgent } from "../../ipc/generated/AddedAgent";
import type { Preferences } from "../../ipc/generated/Preferences";
import { useKeyLabel } from "../../keys/useKeymap";
import { agentOf } from "../../lib/agents";
import { AgentLogo } from "../../ui/status";
import { OWN_AGENTS, type OwnAgent } from "./agentList";
import { useSettings } from "./context";
import { rowKey } from "./model";
import { Footnote, Group, Page, SettingRow, Unreadable } from "./Row";
import { oneLine } from "./words";

/** One thing an agent can do with Muster, ✓ or ✗. */
function Fact({ can, children }: { can: boolean; children: string }) {
  return (
    <span>
      {can ? (
        <span className="mu-found">✓</span>
      ) : (
        <span className="mu-quiet" aria-hidden="true">
          ✗
        </span>
      )}{" "}
      {can ? children : `${children}, not checked yet`}
    </span>
  );
}

function OwnRow({ agent }: { agent: OwnAgent }) {
  return (
    <SettingRow id={rowKey("agents", `agent.${agent.id}`)} className="mu-agentrow">
      <AgentLogo harness={agent.id} size="lg" />
      <span className="mu-agentrow-text">
        <span className="mu-agentrow-name">
          <b>{agentOf(agent.id).name}</b>
        </span>
        <span className="mu-agentrow-facts">
          <Fact can>{agent.status}</Fact>
          <Fact can={agent.resumes}>resumes after a restart</Fact>
          <Fact can={agent.names}>names chats</Fact>
        </span>
      </span>
    </SettingRow>
  );
}

function AddedRow({ agent }: { agent: AddedAgent }) {
  return (
    <SettingRow id={rowKey("agents", `agent.${agent.name}`)} className="mu-agentrow">
      <span
        className="mu-logo mu-logo--lg mu-logo--mark mu-logo--added"
        role="img"
        aria-label={oneLine(agent.label)}
      >
        {oneLine(agent.mark)}
      </span>
      <span className="mu-agentrow-text">
        <span className="mu-agentrow-name">
          <b>{oneLine(agent.label)}</b>
          <span className="mu-label">added by you</span>
        </span>
        <span className="mu-agentrow-facts">
          <span className="mu-mono">{oneLine(agent.name)}</span>
          <span>
            {agent.takes_prompt ? <span className="mu-found">✓</span> : "✗"}{" "}
            {agent.takes_prompt ? "takes a first prompt" : "takes no first prompt"}
          </span>
          <span>status from its window title</span>
          <span>
            {agent.asks ? <span className="mu-found">✓</span> : "✗"}{" "}
            {agent.asks ? "names chats" : "names no chats: its settings say no way to ask it"}
          </span>
        </span>
      </span>
    </SettingRow>
  );
}

export function Agents({ preferences }: { preferences: Preferences }) {
  const { actions } = useSettings();
  const readKey = useKeyLabel("read_settings");
  return (
    <Page
      title="Agents"
      lead="The coding agents Muster can start, with your own sign-ins. Muster hears each the way it allows, without editing its settings."
    >
      <Group title="Muster's own">
        {OWN_AGENTS.map((agent) => (
          <OwnRow key={agent.id} agent={agent} />
        ))}
      </Group>

      <Group title="Added by you">
        {preferences.problem ? (
          <Unreadable problem={preferences.problem} readKey={readKey} />
        ) : null}
        {preferences.agents.map((agent) => (
          <AddedRow key={agent.name} agent={agent} />
        ))}
        {preferences.agents.length === 0 && !preferences.problem ? (
          <p className="mu-footnote app-settings-footnote">
            None yet. An agent you add is offered beside Muster's own: for a new session, and for a
            ticket where it takes a first prompt.
          </p>
        ) : null}
        <SettingRow
          id={rowKey("agents", "read_again")}
          className="app-settings-buttons"
          act={actions.readAgain}
        >
          <button
            type="button"
            className="mu-btn mu-btn--quiet"
            title={`Read it again (${readKey})`}
            onClick={actions.readAgain}
          >
            Read it again
          </button>
        </SettingRow>
      </Group>

      <Footnote>
        Add an agent under <code>[agents.&lt;name&gt;]</code> in <code>{preferences.file}</code>:
        its program and fixed arguments, how it takes a first prompt, how it carries on after a
        restart, and its mark's colour. Never in a repository.
      </Footnote>
    </Page>
  );
}
