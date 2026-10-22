// The screens: the two lists the sidebar shows, and the pages shown beside
// the sidebar instead. The lists are the window's first sight and both stay
// drawn, one hidden (app/App.tsx: Body), so they load with it; a page loads
// when first shown (bundle-dynamic-imports, `ARC-25`). Each screen's folder
// belongs to one branch (see apps/desktop/PLAN.md).

import { type ComponentType, lazy } from "react";
import { QueueScreen } from "../screens/queue/QueueScreen";
import AllSessions from "../screens/sessions/AllSessions";
import type { ListId, ScreenId } from "../state/ui";

/** A list in the sidebar; `active` while its keys are the window's (it is shown, and no page is). */
export type ListScreen = ComponentType<{ active: boolean }>;

export const LISTS: Record<ListId, ListScreen> = {
  all: AllSessions,
  "needs-you": QueueScreen,
};

export const PAGES: Record<Exclude<ScreenId, ListId>, ComponentType> = {
  tickets: lazy(() => import("../screens/tickets/Tickets")),
  settings: lazy(() => import("../screens/settings/Settings")),
};
