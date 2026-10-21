// The screens the sidebar goes between. Each loads when first shown, so the
// window starts with only what it draws first (bundle-dynamic-imports,
// `ARC-25`). Each screen's folder belongs to one branch (see
// apps/desktop/PLAN.md).

import { type ComponentType, lazy } from "react";
import type { ScreenId } from "../state/ui";

export const SCREENS: Record<ScreenId, ComponentType> = {
  all: lazy(() => import("../screens/sessions/AllSessions")),
  "needs-you": lazy(() => import("../screens/queue/NeedsYou")),
  tickets: lazy(() => import("../screens/tickets/Tickets")),
  settings: lazy(() => import("../screens/settings/Settings")),
};
