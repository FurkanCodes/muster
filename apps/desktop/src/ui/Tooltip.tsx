// A short word on what a control does, and its key, shown as the pointer
// rests on it (design system: Tooltip), on Radix's tooltip: after a short
// rest, at once from one to the next, away on a press or ⎋, kept inside
// the window. It names nothing a screen reader lacks: the control has its
// own label, and the tooltip describes it while shown.

import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import type { ReactElement, ReactNode } from "react";
import { portalRoot } from "./portal";

export function Tooltip({ content, children }: { content: ReactNode; children: ReactElement }) {
  return (
    <TooltipPrimitive.Provider delayDuration={400} skipDelayDuration={300} disableHoverableContent>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal container={portalRoot()}>
          <TooltipPrimitive.Content
            className="mu-tooltip"
            side="bottom"
            sideOffset={6}
            collisionPadding={8}
          >
            {content}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}
