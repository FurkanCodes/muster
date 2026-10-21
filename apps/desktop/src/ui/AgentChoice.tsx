import { AgentLogo } from "./status";
import "./agent-choice.css";

/**
 * The coding agent to start, picked in a dialog (design system:
 * AgentChoice). The consumer gives the agents in the order offered, the
 * last chosen first, and goes round them on ⇥ and ⇧⇥.
 */
export interface AgentOption {
  id: string;
  name: string;
}
export function AgentChoice({
  agents,
  value,
  last,
  onChange,
}: {
  agents: readonly AgentOption[];
  value: string;
  last: string;
  onChange: (id: string) => void;
}) {
  return (
    <div className="mu-choices" role="radiogroup" aria-label="Agent">
      {agents.map((agent) => (
        // biome-ignore lint/a11y/useSemanticElements: radio cards contain a logo and last-chosen note, as specified by AgentChoice.
        <button
          key={agent.id}
          type="button"
          className="mu-choice"
          role="radio"
          aria-checked={value === agent.id}
          tabIndex={value === agent.id ? 0 : -1}
          onClick={() => onChange(agent.id)}
        >
          <span className="ui-choice-logo" aria-hidden="true">
            <AgentLogo harness={agent.id} />
          </span>
          <span className="mu-cut">{agent.name}</span>
          {agent.id === last ? <span className="mu-choice-note">last chosen</span> : null}
        </button>
      ))}
    </div>
  );
}
