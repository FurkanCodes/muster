# tickets-from-file

A ticket plugin for Muster in POSIX `sh`, with `awk` and `sed`: no `jq`, no Python. It serves tickets from a plain text file, so a project with no tracker still has a Tickets screen, and it is a short, complete example to start a ticket plugin of your own from.

- **What it runs:** `./tickets`, in its own folder. It reads the tickets' file and, when you close a ticket, writes it.
- **What it needs:** `awk` and `sed`, which come with macOS and Linux.
- **What it is given:** no variables of yours and no secrets.
- **Licence:** MIT, as Muster.

## Its tickets

`tickets.txt` beside it holds them, a paragraph each:

```text
WEB-2 Make the button blue
labels: ui, good first issue
The button on the first page is grey.
Make it the blue of the logo.
```

The first line is the ticket's key and its title. A key is a number (`42`) or a word and a number (`WEB-7`). A `labels:` line may come next, then the ticket's text, until a blank line. A line starting with `#` is a comment. Closing a ticket on the Tickets screen takes its paragraph out of the file.

A project can keep its own file instead: *Tickets from* on the Settings screen asks for one (*File*), or the project's `muster.toml` names it, relative to the project's folder:

```toml
[tickets]
tracker = "tickets-from-file"
file = "TICKETS.txt"
```

Installed from git, the `tickets.txt` beside it is Muster's copy of the plugin, which an update replaces; a project's own file is the project's.

## Try it

Copy the folder somewhere of your own, and link it from a terminal (not one inside Muster):

```bash
cp -R examples/plugins/tickets-from-file ~/tickets-from-file
muster plugin link ~/tickets-from-file
```

Then, on the Settings screen, choose *File* under a project's *Tickets from*, and its tickets are on the Tickets screen. `muster plugin log tickets-from-file` shows what it wrote on stderr: each file it listed, each ticket it closed, and each paragraph it skipped for want of a key.

[docs/plugins.md](../../../docs/plugins.md) explains every line of it: the manifest, the ticket protocol, and how to test and publish a plugin.
