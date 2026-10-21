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

// Deliberately a text subset: unknown Markdown (including HTML and images)
// stays literal. There is no HTML parser, URL element, or resource loader.
function inline(text: string): ReactNode[] {
  const parts = text.split(
    /(!\[[^\]]*\]\([^\n]*?\)|`[^`]+`|\[[^\]]+\]\([^\n]*?\)|\*\*[^*]+\*\*|\*[^*]+\*)/g,
  );
  let offset = 0;
  return parts.map((part) => {
    const at = offset;
    offset += part.length;
    if (part.startsWith("!")) return part;
    if (part.startsWith("`")) return <code key={at}>{part.slice(1, -1)}</code>;
    const link = /^\[([^\]]+)\]\((.*)\)$/.exec(part);
    if (link) return `${link[1]} (${link[2]})`;
    if (part.startsWith("**")) return <strong key={at}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("*")) return <em key={at}>{part.slice(1, -1)}</em>;
    return part;
  });
}

export function TicketText({ body }: { body: string }) {
  const lines = ticketText(body).replace(/\r\n?/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  for (let at = 0; at < lines.length; ) {
    const line = lines[at] ?? "";
    if (!line.trim()) {
      at++;
      continue;
    }
    const key = at;
    if (line.startsWith("```")) {
      const code: string[] = [];
      at++;
      while (at < lines.length && !lines[at]?.startsWith("```")) code.push(lines[at++] ?? "");
      at++;
      blocks.push(
        <pre key={key}>
          <code>{code.join("\n")}</code>
        </pre>,
      );
      continue;
    }
    if (/^#{1,6}\s/.test(line)) {
      blocks.push(<h3 key={key}>{inline(line.replace(/^#{1,6}\s+/, ""))}</h3>);
      at++;
      continue;
    }
    const list = /^(?:[-*+] |\d+\. )/.exec(line);
    if (list) {
      const ordered = /^\d/.test(line);
      const items: ReactNode[] = [];
      const pattern = ordered ? /^\d+\. / : /^[-*+] /;
      while (at < lines.length && pattern.test(lines[at] ?? "")) {
        items.push(<li key={at}>{inline((lines[at++] ?? "").replace(pattern, ""))}</li>);
      }
      blocks.push(ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>);
      continue;
    }
    const paragraph: string[] = [line];
    at++;
    while (
      at < lines.length &&
      lines[at]?.trim() &&
      !/^(?:#{1,6}\s|```|[-*+] |\d+\. )/.test(lines[at] ?? "")
    )
      paragraph.push(lines[at++] ?? "");
    blocks.push(<p key={key}>{inline(paragraph.join("\n"))}</p>);
  }
  return (
    <article className="mu-prose tickets-prose" aria-label="The ticket's text">
      {blocks.length ? blocks : <p className="mu-quiet">(No description.)</p>}
    </article>
  );
}
