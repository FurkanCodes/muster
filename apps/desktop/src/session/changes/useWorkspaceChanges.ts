// A workspace's listing and timeline, kept in step with the state as the
// TUI keeps its pane (app::changes::settle_changes): the timeline is asked
// again when the workspace's newest entry moves, the listing when its
// summary says other than the listing does, both on `r`. Nothing polls.

import { useEffect, useRef, useState } from "react";
import { useDaemon } from "../../app/context";
import type { ChangeSummary } from "../../ipc/generated/ChangeSummary";
import type { Changes } from "../../ipc/generated/Changes";
import type { Timeline } from "../../ipc/generated/Timeline";
import type { WorkspaceId } from "../../ipc/generated/WorkspaceId";
import { sameSummary, summaryOf } from "./labels";
import { type Answer, useReply } from "./useReply";

export function useWorkspaceChanges(workspace: WorkspaceId, again: number) {
  const summary = useDaemon((state) => state.workspaces[workspace]?.changes ?? null);
  const newest = useDaemon((state) => state.workspaces[workspace]?.timeline ?? null);
  const [stale, setStale] = useState(0);
  const listingReply = useReply({ WorkspaceChanges: { workspace } }, `${again}:${stale}`);
  const timelineReply = useReply({ Timeline: { workspace } }, `${again}:${newest}`);
  const listing: Answer<Changes> =
    listingReply.state !== "ready"
      ? listingReply
      : typeof listingReply.value !== "string" && "Changes" in listingReply.value
        ? { state: "ready", value: listingReply.value.Changes }
        : { state: "error", error: "The daemon did not send the changes." };
  const timeline: Answer<Timeline> =
    timelineReply.state !== "ready"
      ? timelineReply
      : typeof timelineReply.value !== "string" && "Timeline" in timelineReply.value
        ? { state: "ready", value: timelineReply.value.Timeline }
        : { state: "error", error: "The daemon did not send the timeline." };
  const seen = useRef<ChangeSummary | null>(summary);
  const shown = useRef(listing);
  shown.current = listing;
  useEffect(() => {
    if (sameSummary(seen.current, summary)) return;
    seen.current = summary;
    const now = shown.current;
    // The daemon tells the state the listing's own summary as it answers:
    // only another says something changed since.
    if (
      now.state === "error" ||
      (now.state === "ready" && !sameSummary(summaryOf(now.value), summary))
    )
      setStale((count) => count + 1);
  }, [summary]);
  return { listing, timeline };
}
