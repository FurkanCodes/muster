# TicketRow

One open ticket in the Tickets list, and the agents working on it.

**Group header** (`mu-group`). Each project's tickets sit under its swatch and its name in `title-section`. Where a project's tickets are in a tracker it shares with other projects (one Jira project, one Linear team), they are listed once under one header that names every project in its colour. After the name comes where the tickets are from, in `meta` ("GitHub issues", "Jira LAMP, only yours"), then a small **SegmentedControl**, *Mine* or *All*, which is the project's *Only my tickets* setting. Headers have `space-24` above them (`space-8` for the first) and `space-6` below.

**Row** (`mu-ticket`). At least `size-row` (32px) high, with `space-6` by `space-12` padding and `radius-md`. In order:
- the key, in `code` and `text-quiet`, in a `size-ticket-key` (64px) column, so titles line up;
- the title in `body`, cut at its end;
- under the title, the agents on it, each as its **StatusMark** and its logo at `size-logo-sm`, `space-10` apart. Rows with no agent have no second line;
- at the end, the **PullRequest** that says most about how the ticket's work stands: merged, then open, then draft, then closed.

**States.** Hover `surface-raised`, selected `surface-lit`, focus ring. A ticket that newly has an agent needing the person flashes like a queue item.

**Keys.** ⏎ opens its agent, the newest still running, or else assigns the first. `a` assigns another, `b` opens the ticket in the browser, `x` closes it, `r` fetches again.

- Don't: colour labels or titles by the tracker. Ticket text is untrusted, so it is shown as plain text.
