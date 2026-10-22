// What was said on a ticket after it was filed, under its text, oldest
// first: each comment under who wrote it and when (its whole date in its
// tooltip), its text drawn as the ticket's is (PRD-07: data, never loaded
// or run). From a tracker that gives none, or a daemon before protocol 50,
// nothing is drawn.

import { useNow } from "../../app/clock";
import type { IssueComment } from "../../ipc/generated/IssueComment";
import { ago } from "../../lib/format";
import { Section, SectionTitle } from "../../ui/Section";
import { TicketText, ticketText } from "./TicketText";

export function Conversation({ comments }: { comments: readonly IssueComment[] }) {
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
          <Comment key={at} comment={comment} />
        ))}
      </ol>
    </Section>
  );
}

function Comment({ comment }: { comment: IssueComment }) {
  const now = useNow();
  const author = comment.author ? ticketText(comment.author) : "Someone";
  return (
    <li className="tickets-comment">
      <span className="tickets-comment-mark" aria-hidden="true">
        {Array.from(author)[0]?.toUpperCase()}
      </span>
      <div className="tickets-comment-head">
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
