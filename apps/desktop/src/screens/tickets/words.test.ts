import { describe, expect, it } from "vitest";
import { oneTicketOf, repositoryOf, trackerName } from "./words";

describe("ARC-24 the words for a tracker and its tickets", () => {
  it("names one ticket of each tracker", () => {
    expect(oneTicketOf("GitHub issues")).toBe("GitHub issue");
    expect(oneTicketOf("GitLab issues")).toBe("GitLab issue");
    expect(oneTicketOf("Linear (KEP)")).toBe("Linear (KEP) ticket");
    expect(oneTicketOf("")).toBe("Ticket");
  });
  it("names the tracker a ticket is closed in", () => {
    expect(trackerName("GitHub issues")).toBe("GitHub");
    expect(trackerName("Jira (acme.atlassian.net)")).toBe("Jira (acme.atlassian.net)");
    expect(trackerName("")).toBe("its tracker");
  });
  it("finds the repository in an issue's address, and nothing in another", () => {
    expect(repositoryOf("https://github.com/FurkanCodes/muster/issues/205")).toBe(
      "FurkanCodes/muster",
    );
    expect(repositoryOf("https://gitlab.example.com/team/sub/app/-/issues/12")).toBe(
      "team/sub/app",
    );
    expect(repositoryOf("https://linear.app/kep/issue/KEP-31")).toBeNull();
    expect(repositoryOf("javascript:alert(1)//issues/1")).toBeNull();
    expect(repositoryOf("")).toBeNull();
  });
});
