# AgentChoice

The coding agent to start, picked in a dialog: assigning an agent to a ticket, tidying commits, starting a session.

**Anatomy.** A grid (`mu-choices`, columns at least 156 wide, `space-8` apart) of radio cards with `role="radiogroup"`. Each card (`mu-choice`, `role="radio"`) is `size-choice` (40px) high, with `space-10` sides and `radius-md`. It holds the agent's logo at `size-logo`, its name in `body`, and an optional note at the end in 11px `text-quiet` ("last chosen"). Resting it is `surface-ground` with a `line` frame. Chosen (`aria-checked="true"`) it is `surface-lit` with a 2px `accent` frame drawn inside.

**Order.** The one chosen last for this kind of work comes first and is chosen to begin with. Then Muster's own agents, then those the person added. A shell is never offered for a ticket.

**Keys.** ⇥ and ⇧⇥ go round the agents; ⏎ starts the one chosen.

**The consumer provides** the agents that can take this work, and which one was chosen last.
