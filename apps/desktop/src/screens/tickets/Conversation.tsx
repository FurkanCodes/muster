// What was said on a ticket after it was filed, under its text, oldest
// first: each comment under who wrote it and when (its whole date in its
// tooltip), its text drawn as the ticket's is (PRD-07: data, never loaded
// or run). From a tracker that gives none, or a daemon before protocol 50,
// nothing is drawn. Where there are comments, its text and each comment
// are parts the person gives an agent or not (a box, or space on the part
// the keys are on, `]` and `[` going between them), as the terminal UI's.

import type { ReactNode } from "react";
import { useNow } from "../../app/clock";
import type { IssueComment } from "../../ipc/generated/IssueComment";
import type { TicketBrief } from "../../ipc/generated/TicketBrief";
import { ago } from "../../lib/format";
import { Checkbox } from "../../ui/Checkbox";
import { Section, SectionTitle } from "../../ui/Section";
import { gives } from "./data";
import { TicketText, ticketText } from "./TicketText";

/** The parts being chosen: what is given, where the keys are, and how to change them. */
export interface Choosing {
  brief: TicketBrief;
  here: number;
  onChoose: (at: number) => void;
  onReach: (at: number) => void;
}

/** The id of part `at`'s element, scrolled into sight as the keys reach it. */
export function partId(at: number): string {
  return `ticket-part-${at}`;
}

/** The ticket's text as a part to give the agent or not. */
export function TextPart({ choosing, children }: { choosing: Choosing; children: ReactNode }) {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the keys reach a part by `]` and `[`; a click only follows them.
    <div
      id={partId(0)}
      className="tickets-part"
      data-here={choosing.here === 0 || undefined}
      onClick={() => choosing.onReach(0)}
    >
      <div className="tickets-part-head">
        <Checkbox
          checked={gives(choosing.brief, 0)}
          label="Give the agent the ticket's text"
          onChange={() => choosing.onChoose(0)}
        />
        <strong>Its text</strong>
      </div>
      {children}
    </div>
  );
}

export function Conversation({
  comments,
  choosing,
}: {
  comments: readonly IssueComment[];
  choosing?: Choosing | null;
}) {
  if (!comments.length) return null;
  return (
    <Section className="tickets-conversation">
      <SectionTitle>
        {comments.length === 1 ? "1 comment" : `${comments.length} comments`}
      </SectionTitle>
      <ol className="tickets-comments">
        {comments.map((comment, at) => (
          // A comment has no id of its own; the list only ever grows at its end.
          // biome-ignore lint/suspicious/noArrayIndexKey: comments are kept in their order
          <Comment key={at} at={at + 1} comment={comment} choosing={choosing ?? null} />
        ))}
      </ol>
    </Section>
  );
}

function Comment({
  at,
  comment,
  choosing,
}: {
  at: number;
  comment: IssueComment;
  choosing: Choosing | null;
}) {
  const now = useNow();
  const author = comment.author ? ticketText(comment.author) : "Someone";
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the keys reach a part by `]` and `[`; a click only follows them.
    <li
      id={choosing ? partId(at) : undefined}
      className="tickets-comment"
      data-here={(choosing && choosing.here === at) || undefined}
      onClick={choosing ? () => choosing.onReach(at) : undefined}
    >
      <span className="tickets-comment-mark" aria-hidden="true">
        {Array.from(author)[0]?.toUpperCase()}
      </span>
      <div className="tickets-comment-head">
        {choosing ? (
          <Checkbox
            checked={gives(choosing.brief, at)}
            label={`Give the agent ${author}'s comment`}
            onChange={() => choosing.onChoose(at)}
          />
        ) : null}
        <strong>{author}</strong>
        {comment.at !== null ? (
          <time
            className="mu-meta"
            dateTime={new Date(comment.at).toISOString()}
            title={new Date(comment.at).toLocaleString()}
          >
            {ago(now - comment.at)} ago
          </time>
        ) : null}
      </div>
      <TicketText body={comment.body} label={`${author}'s comment`} />
    </li>
  );
}
