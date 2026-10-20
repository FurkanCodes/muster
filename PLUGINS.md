# Plugins

Muster keeps a lean core and grows through plugins: ticket sources, and panels beside a session. This is the list of them. Muster's own come with it; the rest live in their authors' repositories and are installed by address:

```bash
muster plugin install https://github.com/someone/their-plugin
```

Muster shows everything a plugin would run before you say yes, and turns it off if its manifest changes after. Read a plugin's README before you install it: a plugin's programs run as you. [docs/plugins.md](docs/plugins.md) says how plugins work and how to write one.

## Muster's own

Built into `muster`, on unless you turn them off on *Settings › Plugins*. They speak the same protocol as anyone's.

| Plugin | What it is | What it runs | What it needs | Licence |
|---|---|---|---|---|
| `github` | GitHub issues | `muster plugin-run github` | `gh`, signed in | MIT |
| `gitlab` | GitLab issues, gitlab.com or self-managed | `muster plugin-run gitlab` | `glab`, signed in | MIT |
| `jira` | Jira work items, Cloud or Data Center | Muster itself | `muster auth jira` (a browser sign-in, or a token) | MIT |
| `linear` | Linear issues | Muster itself | `muster auth linear` (a browser sign-in, or an API key) | MIT |

## Examples

In this repository, to read and start from. Copy one somewhere of your own and `muster plugin link` it.

| Plugin | What it is | What it runs | What it needs | Licence |
|---|---|---|---|---|
| [`tickets-from-file`](examples/plugins/tickets-from-file) | Tickets from a plain text file: a ticket source in POSIX `sh` | `./tickets` | `awk`, `sed` | MIT |
| [`session-graph`](examples/plugins/session-graph) | A session's live flow graph beside it: a panel wrapping zoetrope's `zoe` | `zoe <transcript> --follow` | [zoetrope](https://github.com/furkankly/zoetrope) | MIT; zoetrope its own |

## The community's

None yet. Yours is welcome.

To add one, open a pull request adding a row here. The plugin must be in a public git repository with `muster-plugin.toml` at its root, and its README must say the same as its row:

- **What it is:** one line.
- **What it runs:** every command its manifest names.
- **What it needs:** the programs in its `requires`, the variables in its `env` and the secrets in its `secrets`, and how to get them.
- **Licence:** its own, and that of any program it wraps.

| Plugin | What it is | What it runs | What it needs | Licence |
|---|---|---|---|---|

Give the repository the topic `muster-plugin`, so it can be found. A listing is not a review: it says what a plugin does in its author's words. A plugin whose repository is gone, or that no longer does what its row says, is taken off.
