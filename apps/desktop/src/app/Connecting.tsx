// What a list says before the daemon has told it anything: that Muster is
// being reached, or that its daemon is out of reach.

import { EmptyState, EmptyStateText } from "../ui/EmptyState";
import { Loading } from "../ui/Loading";
import { useDaemon } from "./context";

export function Connecting() {
  const down = useDaemon((state) => state.link.state === "down");
  return down ? (
    <EmptyState role="status">
      <EmptyStateText>Muster's daemon is out of reach.</EmptyStateText>
    </EmptyState>
  ) : (
    <Loading label="Connecting to Muster" />
  );
}
