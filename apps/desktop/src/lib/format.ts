// Numbers and times as Muster says them: short, relative, tabular.

/** How long ago, or for how long: `32s`, `4m`, `3h`, `2d`. */
export function ago(millis: number): string {
  const seconds = Math.max(0, Math.floor(millis / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

/** What an agent used: `182k tokens`, `1.2M tokens`, `$0.42, 40k tokens`. */
export function spend(tokens: number, costMicros: number | null): string {
  const count =
    tokens >= 1_000_000
      ? `${(tokens / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`
      : tokens >= 1000
        ? `${Math.round(tokens / 1000)}k`
        : `${tokens}`;
  const words = `${count} tokens`;
  return costMicros === null ? words : `$${(costMicros / 1_000_000).toFixed(2)}, ${words}`;
}

/** The true minus sign, so `+120` and `−30` line up. */
export function removed(count: number): string {
  return `−${count}`;
}

/** A path's folder and name, for showing the name strong and the folder quiet. */
export function splitPath(path: string): { dir: string; name: string } {
  const slash = path.lastIndexOf("/");
  return slash < 0
    ? { dir: "", name: path }
    : { dir: path.slice(0, slash + 1), name: path.slice(slash + 1) };
}
