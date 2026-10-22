import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";
import { AgentLogo } from "./status";
import "./agent-choice.css";

/**
 * The coding agent to start, picked in a dialog (design system:
 * AgentChoice), on Base UI's radio group, each a card drawn as a button.
 * The consumer gives the agents in the order offered, the last chosen
 * first, and goes round them on ⇥ and ⇧⇥.
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
    <RadioGroup
      className="mu-choices"
      aria-label="Agent"
      value={value}
      onValueChange={(next) => {
        if (typeof next === "string") onChange(next);
      }}
      // The arrows are the dialog's (which project, what it is made on);
      // ⇥ goes round the agents, as the consumer says.
      onKeyDown={(event) => {
        if (event.key.startsWith("Arrow")) event.preventBaseUIHandler();
      }}
    >
      {agents.map((agent) => (
        <Radio.Root
          key={agent.id}
          value={agent.id}
          nativeButton
          render={<button type="button" />}
          className="mu-choice"
          // ⏎ is the dialog's answer, not the radio's to swallow.
          onKeyDown={(event) => {
            if (event.key === "Enter") event.preventBaseUIHandler();
          }}
        >
          <span className="ui-choice-logo" aria-hidden="true">
            <AgentLogo harness={agent.id} />
          </span>
          <span className="mu-cut">{agent.name}</span>
          {agent.id === last ? <span className="mu-choice-note">last chosen</span> : null}
        </Radio.Root>
      ))}
    </RadioGroup>
  );
}
