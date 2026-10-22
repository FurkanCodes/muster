// What a list says before the daemon has told it anything: that Muster is
// being reached, or that its daemon is out of reach.

import { EmptyState, EmptyStateText } from "../ui/EmptyState";
import { useDaemon } from "./context";

export function Connecting() {
  const down = useDaemon((state) => state.link.state === "down");
  return (
    <EmptyState role="status">
      <EmptyStateText>
        {down ? "Muster's daemon is out of reach." : "Connecting to Muster…"}
      </EmptyStateText>
    </EmptyState>
  );
}
