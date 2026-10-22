// Settings › Agents (board: SettingsAgents): the coding agents Muster can
// start, its own and those the person added in Muster's own settings
// (`[agents.<name>]`), each with where the daemon finds it and the version
// it says (agentsFound.ts); written by hand, so the page says what does not
// read, opens the file in the person's editor and reads it again on their
// word.

import { type ReactNode, useState } from "react";
import { useActions, useBridge } from "../../app/context";
import type { AddedAgent } from "../../ipc/generated/AddedAgent";
import type { AgentFound } from "../../ipc/generated/AgentFound";
import type { Preferences } from "../../ipc/generated/Preferences";
import { useKeyLabel } from "../../keys/useKeymap";
import { agentOf } from "../../lib/agents";
import { Button } from "../../ui/controls";
import { Label } from "../../ui/Label";
import { Loading } from "../../ui/Loading";
import {
  AgentLogo,
  AgentMark,
  Found as FoundText,
  Missing,
  Version as VersionText,
} from "../../ui/status";
import { OWN_AGENTS, type OwnAgent } from "./agentList";
import { type Found, useAgentsFound } from "./agentsFound";
import { canAsk } from "./asks";
import { useSettings } from "./context";
import { rowKey } from "./model";
import { Footnote, Group, Page, SettingRow, Unreadable } from "./Row";
import { oneLine } from "./words";

/** A thing that holds or not: ✓ in the finished colour, or a quiet ✗; then what it is. */
function Check({ holds, children }: { holds: boolean; children: string }) {
  return (
    <span>
      {holds ? (
        <FoundText>
          <span aria-hidden="true">✓</span>
        </FoundText>
      ) : (
        <span className="mu-quiet" aria-hidden="true">
          ✗
        </span>
      )}{" "}
      {children}
    </span>
  );
}

/** One thing one of Muster's own agents can do with it, ✓, or ✗ while it is not checked yet. */
function Fact({ can, children }: { can: boolean; children: string }) {
  return <Check holds={can}>{can ? children : `${children}, not checked yet`}</Check>;
}

/** A home folder written `~`, as the board writes a path. */
function homely(path: string): string {
  return path.replace(/^\/(Users|home)\/[^/]+/, "~");
}

/** The agent `harness` as the daemon finds it, if it said. */
function foundOf(found: Found, harness: string): AgentFound | null {
  return found.state === "found" ? (found.agents[harness] ?? null) : null;
}

/** Where the agent's program is, or that it is not found, or not known. */
function Where({
  found,
  harness,
  install,
}: {
  found: Found;
  harness: string;
  install: ReactNode;
}): ReactNode {
  if (found.state === "asking") return <Loading label="Looking for it" layout="inline" />;
  if (found.state === "not-known")
    return (
      <span className="mu-quiet">
        {found.why === "older"
          ? "where it is installed is not known: Muster's daemon is older than this app"
          : `where it is installed is not known: ${oneLine(found.why)}`}
      </span>
    );
  const path = foundOf(found, harness)?.path;
  if (!path)
    return (
      <>
        <Missing>✗ not found</Missing>
        {install}
      </>
    );
  return (
    <FoundText>
      <span aria-hidden="true">✓</span>{" "}
      <span className="mu-mono" title={path}>
        {homely(oneLine(path))}
      </span>
    </FoundText>
  );
}

/** The version it says it is, where it said one. */
function AgentVersion({ found, harness }: { found: Found; harness: string }) {
  const version = foundOf(found, harness)?.version;
  return version ? <VersionText>{oneLine(version)}</VersionText> : null;
}

function OwnRow({ agent, found }: { agent: OwnAgent; found: Found }) {
  return (
    <SettingRow id={rowKey("agents", `agent.${agent.id}`)} className="mu-agentrow">
      <AgentLogo harness={agent.id} size="lg" />
      <span className="mu-agentrow-text">
        <span className="mu-agentrow-name">
          <b>{agentOf(agent.id).name}</b>
          <AgentVersion found={found} harness={agent.id} />
        </span>
        <span className="mu-agentrow-facts">
          <Where
            found={found}
            harness={agent.id}
            install={
              <span>
                Install it with <span className="mu-mono">{agent.install}</span>, then read again.
              </span>
            }
          />
          <Fact can>{agent.status}</Fact>
          <Fact can={agent.resumes}>resumes after a restart</Fact>
          <Fact can={agent.names}>names chats</Fact>
        </span>
      </span>
    </SettingRow>
  );
}

function AddedRow({ agent, found }: { agent: AddedAgent; found: Found }) {
  return (
    <SettingRow id={rowKey("agents", `agent.${agent.name}`)} className="mu-agentrow">
      <AgentMark mark={oneLine(agent.mark)} label={oneLine(agent.label)} size="lg" />
      <span className="mu-agentrow-text">
        <span className="mu-agentrow-name">
          <b>{oneLine(agent.label)}</b>
          <AgentVersion found={found} harness={agent.name} />
          <Label>added by you</Label>
        </span>
        <span className="mu-agentrow-facts">
          <span className="mu-mono">{oneLine(agent.name)}</span>
          <Where found={found} harness={agent.name} install={null} />
          <Check holds={agent.takes_prompt}>
            {agent.takes_prompt ? "takes a first prompt" : "takes no first prompt"}
          </Check>
          <span>status from its window title</span>
          <Check holds={agent.asks}>
            {agent.asks ? "names chats" : "names no chats: its settings say no way to ask it"}
          </Check>
        </span>
      </span>
    </SettingRow>
  );
}

export function Agents({ preferences }: { preferences: Preferences }) {
  const { state, actions } = useSettings();
  const readKey = useKeyLabel("read_settings");
  const found = useAgentsFound(state.reads);
  const bridge = useBridge();
  const ui = useActions();
  const [opening, setOpening] = useState(false);
  const edit = () => {
    if (!canAsk(bridge)) return ui.tell("This window cannot open an editor.", true);
    if (opening) return;
    setOpening(true);
    bridge
      .editSettingsFile()
      .then(
        () =>
          ui.tell(`Opened ${preferences.file} in your editor. Press ${readKey} once it is saved.`),
        (error: unknown) => ui.tell(error instanceof Error ? error.message : String(error), true),
      )
      .finally(() => setOpening(false));
  };
  return (
    <Page
      title="Agents"
      lead="The coding agents Muster can start, with your own sign-ins. Muster hears each the way it allows, without editing its settings."
    >
      <Group title="Muster's own">
        {OWN_AGENTS.map((agent) => (
          <OwnRow key={agent.id} agent={agent} found={found} />
        ))}
      </Group>

      <Group title="Added by you">
        {preferences.problem ? (
          <Unreadable problem={preferences.problem} readKey={readKey} />
        ) : null}
        {preferences.agents.map((agent) => (
          <AddedRow key={agent.name} agent={agent} found={found} />
        ))}
        {preferences.agents.length === 0 && !preferences.problem ? (
          <Footnote>
            None yet. An agent you add is offered beside Muster's own: for a new session, and for a
            ticket where it takes a first prompt.
          </Footnote>
        ) : null}
        <SettingRow id={rowKey("agents", "edit")} className="app-settings-buttons" act={edit}>
          <Button title="Open settings.toml in your editor" disabled={opening} onClick={edit}>
            Edit settings.toml
          </Button>
        </SettingRow>
        <SettingRow
          id={rowKey("agents", "read_again")}
          className="app-settings-buttons"
          act={actions.readAgain}
        >
          <Button title={`Read it again (${readKey})`} onClick={actions.readAgain}>
            Read it again
          </Button>
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
