// How a session stands, as a mark, a colour and a word (README: Colour).

import type { Session } from "../ipc/generated/Session";
import type { WaitReason } from "../ipc/generated/WaitReason";

export type StatusKind = "working" | "waiting" | "done" | "error" | "ended" | "fresh";

export function statusKind(session: Pick<Session, "status">): StatusKind {
  const status = session.status;
  switch (status.state) {
    case "running":
      return "working";
    case "needs_you":
      return "waiting";
    case "finished":
      return "done";
    case "exited":
      return status.success ? "ended" : "error";
    case "fresh":
      return "fresh";
  }
}

const WAITS: Record<WaitReason, string> = {
  permission: "wants your permission",
  question: "asks you something",
  startup: "asks before it starts",
  action: "wants you to act",
};

/** What a session is doing, in the words of its second line. */
export function doing(session: Pick<Session, "status" | "activity">): string {
  if (session.activity) return session.activity.text;
  const status = session.status;
  switch (status.state) {
    case "running":
      return "working";
    case "needs_you":
      return WAITS[status.reason];
    case "finished":
      return "finished";
    case "exited":
      return status.success ? "ended" : "exited with an error";
    case "fresh":
      return "starting";
  }
}
