import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useBridge, useUi } from "../../app/context";
import type { Command } from "../../ipc/generated/Command";
import type { Reply } from "../../ipc/generated/Reply";
import { errorText } from "../../lib/text";

export type Answer<T> =
  | { state: "loading" }
  | { state: "error"; error: string }
  | { state: "ready"; value: T };

/**
 * The daemon's answer to `command`, asked again whenever `revision` says
 * so; the latest asked wins. Nothing is asked while the window is hidden,
 * and a window seen again asks only what it has no answer to (ARC-25).
 */
export function useReply(command: Command | null, revision: string): Answer<Reply> {
  const bridge = useBridge();
  const visible = useUi((state) => state.visible);
  const newest = useEffectEvent(() => command);
  const tag = `${JSON.stringify(command)}:${revision}`;
  const [answer, setAnswer] = useState<{ tag: string; answer: Answer<Reply> }>();
  const asked = useRef<string | null>(null);
  useEffect(() => {
    const request = newest();
    if (!visible || !request || asked.current === tag) return;
    asked.current = tag;
    let current = true;
    let settled = false;
    const settle = (result: Answer<Reply>) => {
      settled = true;
      if (current) setAnswer({ tag, answer: result });
    };
    bridge.command(request).then(
      (value) => settle({ state: "ready", value }),
      (error: unknown) => settle({ state: "error", error: errorText(error) }),
    );
    return () => {
      current = false;
      // An answer dropped on the way is asked again next time.
      if (!settled && asked.current === tag) asked.current = null;
    };
  }, [bridge, visible, tag]);
  return answer?.tag === tag ? answer.answer : { state: "loading" };
}
