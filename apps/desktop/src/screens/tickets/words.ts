// What the screen calls a tracker and its tickets, from what the daemon says
// of them (`Issues.tracker`: "GitHub issues", "Jira (acme.atlassian.net)",
// "Linear (KEP)", a plugin's name).

/** One ticket of `tracker`: "GitHub issue", "Linear (KEP) ticket". */
export function oneTicketOf(tracker: string): string {
  if (/ issues$/.test(tracker)) return tracker.replace(/ issues$/, " issue");
  return tracker ? `${tracker} ticket` : "Ticket";
}

/** The tracker a ticket is closed in: "GitHub", "Jira (acme.atlassian.net)". */
export function trackerName(tracker: string): string {
  return tracker.replace(/ issues$/, "") || "its tracker";
}

/** The repository a GitHub or GitLab issue's address names (`owner/repo`), if it does. */
export function repositoryOf(url: string): string | null {
  try {
    const { protocol, pathname } = new URL(url);
    if (protocol !== "http:" && protocol !== "https:") return null;
    return /^\/(.+?)\/(?:-\/)?issues\/\d+\/?$/.exec(pathname)?.[1] ?? null;
  } catch {
    return null;
  }
}
