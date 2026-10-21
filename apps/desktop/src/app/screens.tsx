// The screens: the two lists the sidebar shows, each drawing its session
// beside it, and the pages shown beside the sidebar instead. Each loads when
// first shown, so the window starts with only what it draws first
// (bundle-dynamic-imports, `ARC-25`). Each screen's folder belongs to one
// branch (see apps/desktop/PLAN.md).

import { type ComponentType, lazy } from "react";
import type { ListId, ScreenId } from "../state/ui";

/** A list's screen; `active` while it is the screen shown, so its keys and its session are drawn. */
export type ListScreen = ComponentType<{ active: boolean }>;

export const LISTS: Record<ListId, ListScreen> = {
  all: lazy(() => import("../screens/sessions/AllSessions")),
  "needs-you": lazy(() => import("../screens/queue/NeedsYou")),
};

export const PAGES: Record<Exclude<ScreenId, ListId>, ComponentType> = {
  tickets: lazy(() => import("../screens/tickets/Tickets")),
  settings: lazy(() => import("../screens/settings/Settings")),
};
