# TicketText

A ticket's body, as its tracker wrote it, made readable.

**Anatomy.** `mu-prose`, in `prose` (14px on 22), no wider than `layout-measure`. Paragraphs are `space-12` apart. Headings are 14px at 600, with `space-20` above and `space-6` below. Lists are indented `space-20`. Inline code is in `font-mono` on `surface-raised`, with `radius-xs`.

**Untrusted.** Ticket text is written by anyone who can file a ticket. Before Muster shows it, it hides credentials that were pasted in and removes control characters. It renders Markdown's text structure only: no images, no raw HTML, and links shown as their text and address, so nothing loads from the ticket. Muster hands the same text to an agent as data, marked as a description of the work.

**Loading and errors.** While it is fetched, three skeleton lines in `surface-raised` at the measure. If it cannot be fetched, say why in `waiting` ("Its text could not be fetched: gh is not signed in") and offer `r` to try again.
