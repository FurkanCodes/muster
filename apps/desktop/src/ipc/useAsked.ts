// What the daemon answers something asked of it from a screen: asked as
// the screen needs it, asked again when what it asks changes, and an
// answer to what is no longer asked dropped.

import { useEffect, useEffectEvent, useState } from "react";
import { errorText } from "../lib/text";

export type Asked<T> =
  | { state: "asking" }
  | { state: "answered"; value: T }
  | { state: "failed"; error: string };

/**
 * What `ask` answers, asked again whenever `key` changes (an answer to an
 * earlier key is dropped); nothing is asked while `ask` is null.
 */
export function useAsked<T>(ask: (() => Promise<T>) | null, key: string): Asked<T> {
  const asking = ask !== null;
  const newest = useEffectEvent(() => ask);
  const [answer, setAnswer] = useState<{ key: string; asked: Asked<T> } | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: asked again as `key` says, with the newest `ask`.
  useEffect(() => {
    const ask = newest();
    if (!ask) return;
    let current = true;
    ask().then(
      (value) => {
        if (current) setAnswer({ key, asked: { state: "answered", value } });
      },
      (error: unknown) => {
        if (current) setAnswer({ key, asked: { state: "failed", error: errorText(error) } });
      },
    );
    return () => {
      current = false;
    };
  }, [key, asking]);
  return answer?.key === key && asking ? answer.asked : { state: "asking" };
}
