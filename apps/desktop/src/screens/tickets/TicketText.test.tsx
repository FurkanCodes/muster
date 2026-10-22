import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TicketText, ticketText } from "./TicketText";

describe("PRD-07 PRD-10 ticket text is inert", () => {
  it("renders HTML and images as text and links as their label and address", () => {
    const body =
      '<img src="https://evil.test/pixel" onerror="alert(1)">\n\n![pixel](https://evil.test/pixel)\n\n[Read this](javascript:alert) <script>alert(1)</script>';
    const { container } = render(<TicketText body={body} />);
    expect(container.querySelectorAll("img,script,a,iframe,svg,style")).toHaveLength(0);
    expect(screen.getByRole("article")).toHaveTextContent('<img src="https://evil.test/pixel"');
    expect(screen.getByRole("article")).toHaveTextContent("![pixel](https://evil.test/pixel)");
    expect(screen.getByRole("article")).toHaveTextContent("Read this (javascript:alert)");
    expect(screen.getByRole("article")).toHaveTextContent("<script>alert(1)</script>");
  });
  it("keeps paragraphs, headings, lists, emphasis and code readable", () => {
    const { container } = render(
      <TicketText
        body={
          '### Done when\n- **One**\n- *Two* with `code`\n\n1. First\n2. Second\n\n```html\n<img src="x">\n```'
        }
      />,
    );
    expect(screen.getByRole("heading", { name: "Done when" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(4);
    expect(container.querySelector("strong")).toHaveTextContent("One");
    expect(container.querySelector("pre")).toHaveTextContent('<img src="x">');
    expect(container.querySelector("img")).toBeNull();
  });
  it("draws quotes, rules, tasks and italics as the terminal UI does", () => {
    const { container } = render(
      <TicketText
        body={
          "> it was fine\n> last week\n\n---\n\n- [ ] to do\n- [x] done\n\n_slanted_ but snake_case_name\n\n~~~\n*not bold*\n~~~"
        }
      />,
    );
    expect(container.querySelector("blockquote")).toHaveTextContent("it was fine last week");
    expect(container.querySelector("hr")).not.toBeNull();
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "☐ to do",
      "☑ done",
    ]);
    expect(container.querySelector("em")).toHaveTextContent("slanted");
    expect(screen.getByRole("article")).toHaveTextContent("snake_case_name");
    expect(container.querySelector("pre")).toHaveTextContent("*not bold*");
  });
  it("says when there is no description and drops control characters", () => {
    render(<TicketText body="  " />);
    expect(screen.getByText("(No description.)")).toBeInTheDocument();
    expect(ticketText(`a${String.fromCharCode(0, 27, 127, 159)}b\n\tc`)).toBe("ab\n\tc");
  });
});
