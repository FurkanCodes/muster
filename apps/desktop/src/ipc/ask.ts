// A command whose answer is one kind of reply: what that reply carries, or
// an error where the daemon answered anything else.

import type { Bridge } from "./bridge";
import type { Command } from "./generated/Command";
import type { Reply } from "./generated/Reply";

/** The kinds of reply that carry something: `Issues`, `SessionStarted`. */
export type ReplyKind = Reply extends infer Each
  ? Each extends object
    ? keyof Each
    : never
  : never;

/** What reply `kind` carries. */
export type ReplyOf<K extends ReplyKind> = Extract<Reply, Record<K, unknown>>[K];

/** What `reply` carries if it is a `kind`. */
export function replyAs<K extends ReplyKind>(reply: Reply, kind: K): ReplyOf<K> | undefined {
  if (typeof reply !== "object" || !(kind in reply)) return undefined;
  return (reply as unknown as Record<K, ReplyOf<K>>)[kind];
}

/** Asks `command` of the daemon, whose answer is a `kind`: what it carries; `otherwise` thrown if not. */
export async function ask<K extends ReplyKind>(
  bridge: Pick<Bridge, "command">,
  command: Command,
  kind: K,
  otherwise = "The daemon answered something else; try again.",
): Promise<ReplyOf<K>> {
  const carried = replyAs(await bridge.command(command), kind);
  if (carried === undefined) throw new Error(otherwise);
  return carried;
}
