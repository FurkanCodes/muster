import type { ReactNode } from "react";

/** The daemon scrubs credentials; this also drops control characters at display. */
export function ticketText(text: string): string {
  return Array.from(text)
    .filter(
      (c) =>
        c === "\n" ||
        c === "\t" ||
        (c.charCodeAt(0) >= 32 && (c.charCodeAt(0) < 127 || c.charCodeAt(0) > 159)),
    )
    .join("");
}

// Deliberately a text subset, the terminal UI's too (view/markdown.rs):
// headings, paragraphs, lists and tasks, quotes, fenced code and rules;
// bold, italic, code and links inside a line. The rest (HTML, images)
// stays literal. There is no HTML parser, URL element or resource loader:
// a link is its words and its address.
function inline(text: string): ReactNode[] {
  const parts = text.split(
    /(!\[[^\]]*\]\([^\n]*?\)|`[^`]+`|\[[^\]]+\]\([^\n]*?\)|\*\*[^*]+\*\*|__[^_]+__|\*[^*\s][^*]*\*|(?<![\p{L}\p{N}])_[^_\s][^_]*_(?![\p{L}\p{N}]))/gu,
  );
  let offset = 0;
  return parts.map((part) => {
    const at = offset;
    offset += part.length;
    if (part.startsWith("!")) return part;
    if (part.startsWith("`")) return <code key={at}>{part.slice(1, -1)}</code>;
    const link = /^\[([^\]]+)\]\((.*)\)$/.exec(part);
    if (link) {
      const [, words = "", address = ""] = link;
      return (
        <span key={at}>
          <span className="tickets-link">{words}</span>
          {address && address !== words ? <span className="mu-quiet"> ({address})</span> : null}
        </span>
      );
    }
    if (part.startsWith("**") || part.startsWith("__"))
      return <strong key={at}>{inline(part.slice(2, -2))}</strong>;
    if (part.startsWith("*") || part.startsWith("_"))
      return <em key={at}>{inline(part.slice(1, -1))}</em>;
    return part;
  });
}

const FENCE = /^\s*(```|~~~)/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
const HEADING = /^#{1,6}\s/;
const QUOTE = /^\s*>/;
const ITEM = /^\s*(?:[-*+] |\d+[.)] )/;

/** A list item's words, its task box drawn where it has one. */
function item(words: string): ReactNode[] {
  const task = /^\[([ xX])\] /.exec(words);
  if (!task) return inline(words);
  return [task[1] === " " ? "☐ " : "☑ ", ...inline(words.slice(task[0].length))];
}

export function TicketText({
  body,
  label = "The ticket's text",
}: {
  body: string;
  /** What it is, for a screen reader: the ticket's text, or someone's comment. */
  label?: string;
}) {
  const lines = ticketText(body).replace(/\r\n?/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  for (let at = 0; at < lines.length; ) {
    const line = lines[at] ?? "";
    if (!line.trim()) {
      at++;
      continue;
    }
    const key = at;
    const fence = FENCE.exec(line)?.[1];
    if (fence) {
      const code: string[] = [];
      at++;
      while (at < lines.length && !lines[at]?.trimStart().startsWith(fence))
        code.push(lines[at++] ?? "");
      at++;
      blocks.push(
        <pre key={key}>
          <code>{code.join("\n")}</code>
        </pre>,
      );
      continue;
    }
    if (RULE.test(line)) {
      blocks.push(<hr key={key} />);
      at++;
      continue;
    }
    if (HEADING.test(line)) {
      blocks.push(
        <h3 key={key}>{inline(line.replace(/^#{1,6}\s+/, "").replace(/\s+#+\s*$/, ""))}</h3>,
      );
      at++;
      continue;
    }
    if (QUOTE.test(line)) {
      const quoted: string[] = [];
      while (at < lines.length && QUOTE.test(lines[at] ?? ""))
        quoted.push((lines[at++] ?? "").replace(/^\s*>\s?/, ""));
      blocks.push(
        <blockquote key={key}>
          <p>{inline(quoted.join("\n"))}</p>
        </blockquote>,
      );
      continue;
    }
    if (ITEM.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const items: ReactNode[] = [];
      const pattern = ordered ? /^\s*\d+[.)] / : /^\s*[-*+] /;
      while (at < lines.length && pattern.test(lines[at] ?? "")) {
        items.push(<li key={at}>{item((lines[at++] ?? "").replace(pattern, ""))}</li>);
      }
      blocks.push(ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>);
      continue;
    }
    const paragraph: string[] = [line];
    at++;
    while (at < lines.length) {
      const next = lines[at] ?? "";
      if (!next.trim() || FENCE.test(next) || RULE.test(next) || HEADING.test(next)) break;
      if (QUOTE.test(next) || ITEM.test(next)) break;
      paragraph.push(next);
      at++;
    }
    blocks.push(<p key={key}>{inline(paragraph.join("\n"))}</p>);
  }
  return (
    <article className="mu-prose tickets-prose" aria-label={label}>
      {blocks.length ? blocks : <p className="mu-quiet">(No description.)</p>}
    </article>
  );
}
