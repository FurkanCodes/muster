import { useEffect, useRef, useState } from "react";
import { useBridge, useUi } from "../../app/context";
import type { Command } from "../../ipc/generated/Command";
import type { Reply } from "../../ipc/generated/Reply";

export type Answer<T> =
  | { state: "loading" }
  | { state: "error"; error: string }
  | { state: "ready"; value: T };

/** Latest request wins; hidden windows wait until seen. No polling (ARC-25). */
export function useReply(command: Command | null, revision: string): Answer<Reply> {
  const bridge = useBridge();
  const visible = useUi((state) => state.visible);
  const request = useRef(command);
  request.current = command;
  const key = JSON.stringify(command);
  const tag = `${key}:${revision}`;
  const [answer, setAnswer] = useState<{ tag: string; answer: Answer<Reply> }>();
  useEffect(() => {
    if (!visible || !request.current) return;
    let current = true;
    void bridge.command(request.current).then(
      (value) => {
        if (current) setAnswer({ tag, answer: { state: "ready", value } });
      },
      (error: unknown) => {
        if (current)
          setAnswer({
            tag,
            answer: {
              state: "error",
              error: error instanceof Error ? error.message : String(error),
            },
          });
      },
    );
    return () => {
      current = false;
    };
  }, [bridge, visible, tag]);
  return answer?.tag === tag ? answer.answer : { state: "loading" };
}
