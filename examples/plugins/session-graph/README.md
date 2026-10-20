# session-graph

A panel plugin for Muster: beside a Claude Code or Codex session, a live flow graph of what its agent is doing, drawn by [zoetrope](https://github.com/furkankly/zoetrope)'s `zoe`.

The plugin is its manifest, `muster-plugin.toml`, and nothing else. It shows how a plugin wraps a terminal program someone else wrote: it names the program in `requires`, says how to get it in `how_to_get`, and runs it with what Muster knows of the session.

- **What it runs:** `zoe <the session's transcript> --follow`, in the session's workspace, when you press its key (`G`, unless you set another on *Settings › Keys*).
- **What it needs:** `zoe` on your `PATH`: `brew install furkankly/tap/zoetrope`. Muster never installs it; *Settings › Plugins* says whether it is found.
- **What it is given:** the session's transcript, as an argument. No variables of yours and no secrets.
- **Licence:** this manifest, MIT, as Muster. zoetrope is its authors', under its own licence (MIT).

A session has a transcript once its agent has been given a first prompt; before that, Muster says so and opens nothing.

## Try it

Copy the folder somewhere of your own and link it, from a terminal (not one inside Muster):

```bash
cp -R examples/plugins/session-graph ~/session-graph
muster plugin link ~/session-graph
```

[docs/plugins.md](../../../docs/plugins.md) says how plugins work and how to write one.
