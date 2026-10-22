// A short word on what a control does, and its key, shown as the pointer
// rests on it (design system: Tooltip), on Base UI's tooltip: after a short
// rest, at once from one to the next, away on a press or ⎋, kept inside
// the window. It names nothing a screen reader lacks: the control has its
// own label, and the tooltip describes it while shown.

import { Tooltip as Base } from "@base-ui/react/tooltip";
import type { ReactElement, ReactNode } from "react";
import { portalRoot } from "./portal";

export function Tooltip({ content, children }: { content: ReactNode; children: ReactElement }) {
  return (
    <Base.Provider delay={400} timeout={300}>
      <Base.Root disableHoverablePopup>
        <Base.Trigger delay={400} render={children} />
        <Base.Portal container={portalRoot()}>
          <Base.Positioner className="ui-tooltip" side="bottom" sideOffset={6} collisionPadding={8}>
            <Base.Popup className="mu-tooltip" role="tooltip">
              {content}
            </Base.Popup>
          </Base.Positioner>
        </Base.Portal>
      </Base.Root>
    </Base.Provider>
  );
}
