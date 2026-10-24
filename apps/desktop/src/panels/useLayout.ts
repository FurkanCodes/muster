// The panels' layout as the window shows it: the person's, read from their
// settings (`preferences.panels`), or as moved in this window until the
// daemon has kept it (`app::drag::keep_panels`). A change is shown at once
// and sent as `ChangePreference(Panels)`; a refusal puts the kept one back
// and says why.

import { useState } from "react";
import { useApp, useDaemon } from "../app/context";
import { errorText } from "../lib/text";
import { Layout } from "./layout";

export interface PanelsLayout {
  layout: Layout;
  /** Shows `layout` while the person drags, not kept yet. */
  show(layout: Layout | null): void;
  /** Shows `layout` and has the daemon keep it. */
  keep(layout: Layout): void;
}

export function useLayout(): PanelsLayout {
  const app = useApp();
  const written = useDaemon((state) => state.preferences?.panels ?? null);
  const [moving, setMoving] = useState<Layout | null>(null);
  const [kept, setKept] = useState<{ layout: Layout; over: string | null } | null>(null);
  // What was kept in this window holds until the daemon's settings say something else.
  const holding = kept && kept.over === written ? kept.layout : null;
  const settled = holding ?? Layout.read(written);
  const layout = moving ?? settled;
  return {
    layout,
    show: setMoving,
    keep(next) {
      setMoving(null);
      if (next.equals(settled)) return;
      setKept({ layout: next, over: written });
      app.bridge.command({ ChangePreference: { Panels: next.toString() } }).catch((error) => {
        setKept(null);
        app.actions.tell(errorText(error), true);
      });
    },
  };
}
