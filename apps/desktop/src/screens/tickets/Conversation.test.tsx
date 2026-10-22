import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { NOW } from "../../ipc/mock/now";
import { renderWithApp } from "../../test/render";
import { Conversation } from "./Conversation";

describe("PRD-07 a ticket's conversation", () => {
  it("shows each comment oldest first under who wrote it and when, its text inert", () => {
    renderWithApp(
      <Conversation
        comments={[
          { author: "mira-k", at: NOW - 26 * 3_600_000, body: "Its **own** connection?" },
          { author: null, at: null, body: "[Read this](javascript:alert) <img src=x>" },
        ]}
      />,
    );
    expect(screen.getByRole("region", { name: "2 comments" })).toBeInTheDocument();
    const [first, second] = screen.getAllByRole("listitem");
    expect(first).toHaveTextContent("mira-k1d ago");
    expect(
      within(first as HTMLElement).getByRole("article", { name: "mira-k's comment" }),
    ).toHaveTextContent("Its own connection?");
    expect(second).toHaveTextContent("Someone");
    expect(second).toHaveTextContent("Read this (javascript:alert) <img src=x>");
    expect(document.querySelectorAll("a,img")).toHaveLength(0);
  });
  it("draws nothing where nothing was said", () => {
    const { container } = renderWithApp(<Conversation comments={[]} />);
    expect(container.querySelector(".tickets-conversation")).toBeNull();
  });
});
