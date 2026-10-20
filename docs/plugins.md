# Writing a plugin for Muster

Muster keeps a lean core and grows through plugins. The core is what everything else stands on: the daemon, projects and worktrees, sessions and their status, the layout, keys and Settings, and the plugin system itself. The rest is plugins, and anyone can write one. Muster's own ticket sources, for GitHub, GitLab, Jira and Linear, are plugins speaking the same protocol yours will. If you write one others could use, please share it: [PLUGINS.md](../PLUGINS.md) lists them.

This guide is for plugin authors. It says what a plugin is and may do, every key of its manifest, how plugins are installed and approved, how to write a ticket source and a panel, and how to test and publish one. Two complete examples are in [examples/plugins](../examples/plugins).

## What a plugin is

A plugin is a folder holding a manifest, `muster-plugin.toml`, and programs in any language. Muster runs the programs as separate processes. It never loads a plugin's code into itself.

A plugin brings one or both of:

- **A ticket source:** a program that lists, shows and closes tickets, which Muster asks for a project's tickets. Its tickets fill the Tickets screen, and the person assigns agents to them as to GitHub's.
- **Panels:** terminal programs shown in a panel beside a session, told what Muster knows of it: its workspace, branch, transcript and more.

### What a plugin may do, and may not

A plugin's programs run as the person, like any program they start. Muster adds rules around them (`PRD-15` in [PRODUCT.md](../PRODUCT.md), `ARC-23` in [ARCHITECTURE.md](../ARCHITECTURE.md)):

- **The person adds it.** Only the person installs a plugin, turns it on or approves it, after Muster shows everything it would run. Muster runs no installer of a plugin's: the programs it needs, the person installs.
- **It never drives an agent.** A plugin never starts, prompts or types into an agent. The person assigns an agent to a ticket; the plugin only says what the ticket is.
- **It gets only what it names.** Anyone's ticket source starts with the basics of the environment (`PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TMPDIR`, `TZ`, `LANG`, `LC_*`), then only the person's variables its manifest names in `env` and the secrets it names in `secrets`. No other credential reaches it.
- **Its tickets are untrusted.** What a ticket source answers is checked and cleaned before anything sees it: keys must have a ticket's shape, control characters go, text is cut to a size, and a credential pasted into a ticket is hidden. A ticket reaches an agent fenced off as a description of the work, never as instructions (`PRD-07`).
- **It lives in Muster's own folder.** An installed plugin is a copy in Muster's data folder (`~/Library/Application Support/muster/plugins` on macOS, `~/.local/share/muster/plugins` on Linux), private to the person, never in a repository or an agent's settings.

### Approval, and its honest limit

Approval binds the manifest. Muster keeps the SHA-256 of `muster-plugin.toml` as the person approved it. A manifest that changes afterwards turns its plugin off, saying why, until the person approves it again by updating it or linking it again. So a plugin cannot add a command to itself after its preview.

Installing, linking, updating and turning a plugin on happen only on the Settings screen's *Plugins* section or with `muster plugin` in a terminal, each after the preview and a yes. Without a terminal the command line refuses unless given `--yes`. Inside a Muster session it refuses outright, so an agent cannot add a plugin by running a command.

The programs a manifest names are not hashed. An installed plugin's are Muster's copy of the commit the person approved. A linked folder's change as its author works.

The honest limit: a program running as the person can write the person's files, Muster's settings and its copies of plugins among them, as the person can. Muster makes adding or approving a plugin neither silent nor easy, marks on the *Plugins* section each change made outside the UI, and catches a changed manifest. It cannot stop the person's own programs. Install plugins you would run yourself.

## The manifest

`muster-plugin.toml` sits at the plugin's root. It is read strictly: an unknown key, or a value of the wrong kind, is an error naming the key (`panels[0].command`), never a default. A manifest is at most 64 KiB.

A plugin with both a ticket source and a panel:

```toml
id = "youtrack"
name = "YouTrack"
version = "0.1.0"
description = "Tickets from YouTrack, and a panel with the ticket's comments."
muster = "0.4.0"
requires = ["curl", "jq"]
how_to_get = "brew install curl jq"
env = ["YOUTRACK_SITE"]
secrets = ["YOUTRACK_TOKEN"]

[tickets]
name = "YouTrack"
noun = "issue"
command = ["./tickets"]

[[tickets.settings]]
key = "project"
label = "Project, such as APP"
required = true

[[tickets.settings]]
key = "query"
label = "Or a query of your own"

[[panels]]
id = "comments"
title = "comments"
on = "workspace"
command = ["./comments", "{ticket}"]
key = "Y"
```

Its keys:

| Key | Needed | What it is |
|---|---|---|
| `id` | yes | What Muster knows the plugin by: 1 to 32 of `a-z`, `0-9` and `-`, starting with a letter or digit. `github`, `gitlab`, `jira`, `linear`, `muster` and `muster-…` are Muster's own. |
| `name` | yes | Its name on screen, one line of at most 40 characters. |
| `version` | yes | Its own version, at most 32 letters, digits, `.`, `-` and `+`. |
| `description` | no | What it does, one line of at most 300 characters. |
| `muster` | yes | The oldest Muster it works with, as `0.4.0`. A Muster older than that refuses it, saying which it needs. Plugins came in 0.4.0; `muster --version` says which you have. |
| `requires` | no | The programs it needs on `PATH`, by name (`["zoe"]`), not by path. Muster never installs them, and shows whether each is found. |
| `how_to_get` | no | How to get them, in your words, one line of at most 300 characters. |
| `env` | no | The names of the person's variables it may see. Letters, digits and `_`, not starting with a digit, never Muster's own `MUSTER_…`. |
| `secrets` | no | The names of the secrets it is given, as `env`'s are written. See [Secrets](#secrets). |
| `[tickets]` | one or both | A ticket source. |
| `[[panels]]` | one or both | A panel, one table each. |

`[tickets]`:

| Key | Needed | What it is |
|---|---|---|
| `name` | yes | The tracker's name, at most 40 characters: *Tickets from* on the Settings screen offers it by this name, and an agent's first prompt says it ("Work on YouTrack issue APP-7"). |
| `noun` | yes | What one of its tickets is called, a word or two of at most 20 letters: `issue`, `ticket`, `task`. |
| `command` | yes | Its program and arguments. See [Commands](#commands). It takes no placeholders: one process serves every project. |
| `[[tickets.settings]]` | no | What a project says to find its tickets, one table a field. |

`[[tickets.settings]]`:

| Key | Needed | What it is |
|---|---|---|
| `key` | yes | The field's key in a project's `[tickets]`: lowercase letters, digits and `_`, starting with a letter, at most 32. Not `tracker` or `mine`, which are Muster's. |
| `label` | no | How *Tickets from* asks for it, at most 40 characters; the key otherwise. |
| `required` | no | `true` if a project must give it. `false` otherwise. |

`[[panels]]`:

| Key | Needed | What it is |
|---|---|---|
| `id` | yes | The panel's id within the plugin, written as a plugin's `id` is. |
| `title` | no | Its panel's title, at most 40 characters; its id otherwise. |
| `on` | no | `session`, beside a session (the default), or `workspace`, beside a workspace. |
| `command` | yes | Its program and arguments, with placeholders. See [Placeholders](#placeholders). |
| `key` | no | The key it suggests, as Muster's settings write keys: a character (`G`), or a named key (`f5`), after `ctrl-` or `alt-` if it has them. The person can set another. |

### Commands

A command is a program and its arguments, `["./tickets", "--quiet"]`, never a line for a shell. The program is one of:

- `./…`, a program inside the plugin's folder (without `..`). Make it executable (`chmod +x`), and commit it so.
- a name, found on `PATH` (`zoe`). Name it in `requires` too.
- a full path (`/usr/bin/python3`).

A script needs a first line saying what runs it (`#!/bin/sh`, `#!/usr/bin/env python3`). Muster runs a ticket source's program in the plugin's folder.

## Installing, linking and updating

From a terminal of your own (not a shell inside Muster, where the command line refuses):

```bash
muster plugin install https://github.com/you/muster-youtrack    # its default branch
muster plugin install git@github.com:you/muster-youtrack.git --ref v0.1.0
muster plugin link ~/code/muster-youtrack                       # while you write it
muster plugin list
```

`install` takes a git repository's address: `https://`, `ssh://`, `user@host:path`, `file://`, or a repository's folder. Plain `http://` and `git://` are refused, as anyone between could change what is fetched, and so is an address holding a password. Muster clones it (`git clone --depth 1`, at `--ref` if given), runs nothing of it, and shows its preview: everything it would run, as here for a linked folder.

```text
Tickets from a file 0.1.0 (tickets-from-file), linked from /Users/you/tickets-from-file
  Tickets from tickets.txt beside it, or a text file of the project's.
  works with Muster 0.4.0 or later
  ticket source File runs: ./tickets
    a project's [tickets] holds file (File, if not tickets.txt beside it)
  needs on PATH: awk (found), sed (found)
  its manifest's SHA-256: 076e48c0a8df5a54bf5fbbc49fb291f2923006b8e9fb63d3f72ba7d77605b76b
Link tickets-from-file and turn it on? [y/N]
```

Said yes to, the plugin is copied to Muster's data folder, approved and turned on.

`link` is for a plugin's author. Muster reads the folder where it is and never writes to it, so you edit it in place. It is previewed and approved as an installed one is.

| Command | What it does |
|---|---|
| `muster plugin list [--json]` | Every plugin, where it is from, whether it is on and why one cannot run. |
| `muster plugin install <address> [--ref <ref>] [--yes]` | Installs from git, after the preview. Updates follow `--ref`. |
| `muster plugin link <folder> [--yes]` | Links a folder. Link it again to approve its manifest after you change it. |
| `muster plugin update <id> [--yes]` | Fetches it again (a linked folder is read again), shows what it would run now, and asks. |
| `muster plugin disable <id>` | Turns it off. Its process stops. |
| `muster plugin enable <id> [--yes]` | Turns it on, after the preview. |
| `muster plugin remove <id> [--yes]` | Removes it and Muster's copy of it, after asking. A linked folder stays. |
| `muster plugin log <id>` | The end of what its program wrote on stderr, its secrets hidden. |
| `muster auth plugin <id> <NAME>` | Keeps a secret its manifest names. |

The Settings screen's *Plugins* section does the same: Space turns the selected plugin on or off, `a` adds one from an address or a folder, `u` updates one, `x` removes one. Beside the list it shows everything the selected plugin runs, the programs it needs and whether they are found, its variables and secrets, and its manifest's hash. A plugin changed from the command line is marked there, with when.

Muster's own four plugins are listed first. They can be turned off, not removed.

## A ticket source, step by step

The complete example is [examples/plugins/tickets-from-file](../examples/plugins/tickets-from-file): some 160 lines of `sh` serving tickets from a text file. This section says everything it relies on.

### 1. The manifest

```toml
id = "tickets-from-file"
name = "Tickets from a file"
version = "0.1.0"
muster = "0.4.0"

[tickets]
name = "File"
noun = "ticket"
command = ["./tickets"]
```

### 2. The protocol

Muster speaks to the program over its stdin and stdout, in JSON-RPC 2.0, one message a line. It writes a request as one line of JSON; the program writes its reply as one line of JSON, ending with a newline. This is version 1 of the protocol.

```text
→ {"jsonrpc":"2.0","id":1,"method":"hello","params":{"muster":"0.4.0","protocol":1}}
← {"jsonrpc":"2.0","id":1,"result":{"protocol":1}}
→ {"jsonrpc":"2.0","id":2,"method":"list","params":{"project":{"root":"/work/app","remote":"github.com/o/app","settings":{"file":"TICKETS.txt"}},"mine":false}}
← {"jsonrpc":"2.0","id":2,"result":{"where":"TICKETS.txt","source":null,"tickets":[{"key":"APP-7","title":"The login page says 404","url":"","labels":["bug"],"author":"sam"}]}}
→ {"jsonrpc":"2.0","id":3,"method":"view","params":{"project":{…},"key":"APP-7"}}
← {"jsonrpc":"2.0","id":3,"result":{"ticket":{"key":"APP-7","title":"The login page says 404"},"body":"After signing in…"}}
→ {"jsonrpc":"2.0","id":4,"method":"close","params":{"project":{…},"key":"APP-7"}}
← {"jsonrpc":"2.0","id":4,"result":{}}
  or, where it cannot:
← {"jsonrpc":"2.0","id":4,"error":{"code":"not_found","message":"there is no ticket APP-7"}}
```

The rules:

- **One process serves everything.** Muster starts the program at the first request for its tickets, says `hello`, and then sends it every project's requests, each naming its project. It stops the program after five minutes unused, and when the daemon stops: it closes the program's stdin, waits a second, then kills it. The program ends when its stdin does, once what is under way is answered.
- **Stdout is for replies only.** Every line the program writes on stdout must be a reply. Anything else, or a line over 4 MiB, is taken as the program breaking. Write anything else on stderr, which is its log.
- **Flush each reply.** Many languages hold back what a program writes to a pipe until a buffer fills, and a reply held back misses its deadline. Flush stdout after each line: in Python `print(…, flush=True)`, in C `fflush(stdout)`. A shell's `printf` and `echo` need nothing.
- **Replies by id.** Answer each request with its `id`. Replies may come in any order; a program may answer one request while working on another. At most eight requests wait on it at once.
- **Deadlines.** `hello` has 5 seconds, every other request 30. A program that lets a deadline pass is taken as hung and killed.
- **Crashes.** A program that ends, or breaks, has every request waiting answered with why, on the Tickets screen; the next request starts it again.
- **Errors.** A program that cannot answer a request replies with an `error`, as below. A request it cannot read, or a method it does not know, is answered with `failed`.

### 3. The methods

**`hello`**, first, once a process.

- params: `muster`, the Muster asking, as `"0.4.0"`; `protocol`, the version it speaks, `1`.
- result: `{"protocol": 1}`. Answering with another version is refused, the message naming both.

**`list`**: the project's open tickets, newest first.

- params: `project` (below); `mine`, `true` where the person asked for only the tickets assigned to them (*Only my tickets* on the Settings screen). A tracker with no one to assign to can ignore it.
- result: `tickets`, a list of tickets (below); `where`, where they are, for the person (`"YouTrack (acme.myjetbrains.com)"`), shown on the Tickets screen, the plugin's `name` where it is missing; `source`, optional, the same text for every project listing the same tickets, so Muster lists them once under one heading. Leave it out, or `null`, for tickets that are the project's own.

**`view`**: one ticket, with its text.

- params: `project`; `key`, the ticket's key.
- result: `ticket`, the ticket, which must be the one asked for; `body`, its text, as plain text with newlines.

**`close`**: closes a ticket, because the person asked. It is the one change Muster makes in a tracker.

- params: `project`; `key`.
- result: `{}`.

A **project** is:

- `root`: its folder.
- `remote`: its git remote as `host/path` (`"github.com/o/app"`), never the remote's address, which can carry a password; `null` where it has none Muster can read.
- `settings`: what its `[tickets]` says for the plugin, its fields as text by key (`{"project": "APP"}`); `{}` with none.

A **ticket** is:

- `key`: `"42"` or `"APP-7"`: a number, or a word and a number with a dash between (letters, digits and `_`, starting with a letter). A number may also be written as a JSON number. Muster writes a key's word in capitals, so `app-7` comes back to `view` and `close` as `APP-7`.
- `title`: one line.
- `url`: optional, its address, for the person to open in their browser.
- `labels`: optional, a list of text.
- `author`: optional, who filed it.

Muster checks and cleans every answer. A ticket whose key is not one of those shapes, or with a field of the wrong kind, is left out (and counted in the daemon's log), not the whole listing. A listing keeps 1,000 tickets; a title is cut to 1,000 bytes, an address to 2,048, labels to 50 of 200, a body to 256 KiB, an error's message to 4,000. Control characters go, and credentials are hidden.

### 4. Errors

```json
{"jsonrpc":"2.0","id":7,"error":{"code":"signed_out","message":"set YOUTRACK_TOKEN with: muster auth plugin youtrack YOUTRACK_TOKEN"}}
```

The `code` says what went wrong, and the person reads the `message` on the Tickets screen, so make it say what to do:

| Code | Means |
|---|---|
| `not_found` | No such ticket. |
| `refused` | The tracker refused: a permission, a bad query. |
| `signed_out` | The person is not signed in, or their sign-in ran out. |
| `not_configured` | The project does not say enough to find its tickets. |
| `failed` | Anything else: the tracker could not be reached, the plugin broke. |

Any other code, or a plain JSON-RPC number, reads as `failed`.

### 5. Settings

A project chooses a ticket source by its `id`, and gives it the fields its `[[tickets.settings]]` declare. The person chooses on the Settings screen: *Tickets from* offers every ticket source turned on, by its `[tickets] name`, and asks for its fields by their labels. Muster writes the choice to the person's own settings for the project, never to its repository. A team can write it in the repository's `muster.toml` instead:

```toml
[tickets]
tracker = "tickets-from-file"
file = "TICKETS.txt"
mine = false        # Muster's own: "Only my tickets"
```

A field the plugin does not declare is refused by name, and a required one left out is asked for. The plugin gets the fields in each request's `project.settings`.

### 6. Environment and secrets

The program starts in its folder with the basics of the daemon's environment (`PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TMPDIR`, `TZ`, `LANG`, `LC_*`), then:

- **`env`:** each variable the manifest names, as the daemon's environment has it.
- **`secrets`:** each secret the manifest names, as a variable of that name: the one the person kept in the Keychain with `muster auth plugin <id> <NAME>` (pasted when asked, not shown, or piped in), else the daemon's variable of that name, which is the way off macOS. A secret reaches only its own plugin. Its log never shows its value.

```bash
muster auth plugin youtrack YOUTRACK_TOKEN
```

A secret that is missing is the plugin's to say: answer with `signed_out` and how to set it.

Nothing of Muster's own (`MUSTER_…`) reaches it, nor any variable not named. A ticket source has no folder of its own for state yet. An installed plugin's folder is Muster's copy of its commit, replaced by each update: keep nothing there you would miss.

### 7. Its log

What the program writes on stderr is its log. Muster keeps the last 64 KiB of it, across restarts, while the daemon runs; secrets and anything shaped like a credential are hidden. Read it with:

```bash
muster plugin log tickets-from-file
```

When the program ends on its own, the last line it wrote there is what the Tickets screen says of why, so make a failing program's last line say what went wrong. Log what helps the person: what it fetched, what it closed, what it skipped and why.

## A panel

A panel is a terminal program shown beside a session or a workspace. Any program that runs in a terminal can be one; it gets the panel's size and the person's keys when they type in it. Its manifest names it, and what Muster knows goes into its arguments as placeholders:

```toml
[[panels]]
id = "graph"
title = "graph"
on = "session"
command = ["zoe", "{transcript}", "--follow"]
key = "G"
```

Its key opens it beside the selected session (one with `on = "workspace"` on the selected worktree or checkout), and pressed again puts it away, its program ended; a line's right-click menu and the help list it too. A session shows only the panels opened beside it, or on its workspace. The person sets its key on *Settings › Keys*, under the plugin's name, kept in their settings as `"plugin.<id>.<panel>" = "G"`; a key Muster already uses on the lists of sessions, or another panel has, is not given to it, and Muster says so. It runs in the session's workspace, and ends with its workspace, as a shell does, and when its plugin is turned off or removed. It is no agent: it is never news, and never needs the person. [examples/plugins/session-graph](../examples/plugins/session-graph) wraps a program someone else wrote, [zoetrope](https://github.com/furkankly/zoetrope)'s `zoe`, with nothing but a manifest.

### Placeholders

A placeholder is a word between braces, anywhere in an argument (`--branch={branch}`). Muster fills them in as it opens the panel:

| Placeholder | Its value | Panels |
|---|---|---|
| `{project}` | the project's name | both |
| `{root}` | the project's folder | both |
| `{workspace}` | the workspace's folder | both |
| `{branch}` | what the workspace has checked out | both |
| `{ticket}` | the ticket the workspace's branch is for | both |
| `{pr}` | the address of the pull request from the workspace's branch | both |
| `{session}` | the session's id | `session` only |
| `{harness}` | the session's agent: `claude`, `codex`, one the person added | `session` only |
| `{resume}` | the id of the agent's conversation, as its resume takes it | `session` only |
| `{transcript}` | the agent's own record of the conversation, Claude Code's or Codex's | `session` only |

Any other `{word}` is refused as the manifest is read, and so is a session's placeholder in a workspace's panel. Other braces are the argument's own (`{"x":1}`). A value Muster does not have refuses the panel with why ("this session has no transcript yet"), never an empty argument. The same values are in the program's environment, as `MUSTER_PLUGIN_PROJECT`, `MUSTER_PLUGIN_BRANCH` and so on.

### Its environment

The program starts in the workspace's folder, in a terminal of Muster's, with the basics of the daemon's environment (as a ticket source's, and `TERMINFO`), the variables and secrets its manifest names (as a ticket source's, [above](#6-environment-and-secrets)), and the `MUSTER_PLUGIN_*` variables. Nothing else of the person's reaches it, nor anything of an agent's or of Muster's hooks.

Muster opens it only where it can run: the plugin on and approved, and every program in `requires` found. Otherwise it opens nothing and says why, with the plugin's `how_to_get`.

A panel shows text and colours, as a session does; pictures drawn by a terminal's graphics protocols do not show in it.

## Testing a plugin by hand

**Speak to it yourself.** A ticket source is a program on a pipe, so test it with no Muster at all. Save the requests in a file, one a line, and pipe them in:

```bash
cat > requests.jsonl <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"hello","params":{"muster":"0.4.0","protocol":1}}
{"jsonrpc":"2.0","id":2,"method":"list","params":{"project":{"root":"/tmp","remote":null,"settings":{}},"mine":false}}
{"jsonrpc":"2.0","id":3,"method":"view","params":{"project":{"root":"/tmp","remote":null,"settings":{}},"key":"WEB-2"}}
{"jsonrpc":"2.0","id":4,"method":"view","params":{"project":{"root":"/tmp","remote":null,"settings":{}},"key":"WEB-99"}}
EOF
./tickets < requests.jsonl
```

Each request should get one line back with its id. Check each is JSON, with `jq -c .` or `python3 -m json.tool --json-lines`. Try a missing ticket, a missing file or sign-in, and a key you made a mistake in.

A program flushes what it holds back as it ends, so that test passes even where replies are held back. Keep its stdin open, as Muster does, and the replies should still come at once, not ten seconds later:

```bash
(cat requests.jsonl; sleep 10) | ./tickets
```

**Then in Muster.** From a terminal of your own:

1. `muster plugin link .` in the plugin's folder. Fix what the manifest's errors name, and link again.
2. On the Settings screen, choose your plugin under *Tickets from* for a project, with its fields.
3. On the Tickets screen, its tickets are listed; `r` lists them again. Show one, and assign an agent to it if you like.
4. `muster plugin log <id>` shows its stderr, and why it ended if it did.

While you work on it:

- **You changed its program:** the running process still has the old one. Turn the plugin off and on again (`muster plugin disable <id>`, then `muster plugin enable <id>`, or Space twice on *Settings › Plugins*), and the next request starts it afresh.
- **You changed its manifest:** it is off until you approve it again. Link it again.

## Publishing

1. **A public git repository, one plugin each,** with `muster-plugin.toml` at its root. Muster installs from its default branch, or from a tag or branch the person names with `--ref`; tag your releases.
2. **A README** that says what it runs, what it needs and how to get it, what of the person's environment and which secrets it is given, and its licence. People read it before they approve.
3. **The topic `muster-plugin`** on the repository, so people can find it.
4. **A line in [PLUGINS.md](../PLUGINS.md)**, by pull request to Muster's repository, saying the same in short.

Each update you publish is previewed again before it runs, so a person always sees what changed. Bump `version` with each.
