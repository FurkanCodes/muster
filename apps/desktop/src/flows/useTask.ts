// One answer at a time: a dialog's acting button is busy while the daemon
// answers, a second press does nothing, and a refusal stays in the dialog
// saying what went wrong.

import { useRef, useState } from "react";
import { said } from "./actions";

export function useTask() {
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (task: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      await task();
    } catch (failure) {
      setError(said(failure));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  return { busy, error, setError, run };
}
