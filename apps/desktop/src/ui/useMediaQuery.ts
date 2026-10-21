import { useCallback, useState, useSyncExternalStore } from "react";

/** Subscribe to the breakpoint, never the stream of resize events (ARC-25). */
export function useMediaQuery(query: string): boolean {
  const [media] = useState(() => matchMedia(query));
  const subscribe = useCallback(
    (changed: () => void) => {
      media.addEventListener("change", changed);
      return () => media.removeEventListener("change", changed);
    },
    [media],
  );
  return useSyncExternalStore(subscribe, () => media.matches);
}
