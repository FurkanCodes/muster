// The screens: the list the sidebar shows, every session under its space
// and project, and the pages shown beside the sidebar instead: Tickets,
// *Needs you* and Settings. The list is the window's first sight, so it
// loads with it, as Needs you does, small and asked for often; the others
// load when first shown (bundle-dynamic-imports, `ARC-25`). Each screen's folder
// belongs to one branch (see apps/desktop/PLAN.md).

import { type ComponentType, lazy } from "react";
import { NeedsYouPage } from "../screens/queue/QueueScreen";
import AllSessions from "../screens/sessions/AllSessions";
import type { ListId, ScreenId } from "../state/ui";

/** A list in the sidebar; `active` while its keys are the window's (it is shown, and no page is). */
export type ListScreen = ComponentType<{ active: boolean }>;

export const LISTS: Record<ListId, ListScreen> = {
  all: AllSessions,
};

export const PAGES: Record<Exclude<ScreenId, ListId>, ComponentType> = {
  "needs-you": NeedsYouPage,
  tickets: lazy(() => import("../screens/tickets/Tickets")),
  settings: lazy(() => import("../screens/settings/Settings")),
};
