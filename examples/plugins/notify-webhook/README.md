# notify-webhook

A hook plugin for Muster in POSIX `sh`, with `sed` and `curl`. When an agent needs you, finishes a turn or fails, it posts one short line to a webhook:

```text
muster · muster/fix-login · Fix the login · needs your permission
```

That is the project, the branch, the chat's name and what happened. It is the shortest useful hook, and an example to start one of your own from.

- **What it runs:** `./notify`, in its own folder, for `session.needs_you`, `session.finished` and `session.error`, at most 15 seconds each.
- **What it needs:** `curl` and `sed`, which come with macOS and Linux.
- **What it is given:** the secret `WEBHOOK_URL`, the webhook's address; no variables of yours.
- **What it sends:** the line above and nothing else: never a prompt, an agent's output or code, which Muster never gives a hook.
- **Licence:** MIT, as Muster.

## Where it posts

The address you keep as `WEBHOOK_URL` decides the shape of what is posted:

| Service | Address | Posted as |
|---|---|---|
| Slack, Mattermost, Google Chat and most chats' incoming webhooks | `https://hooks.slack.com/services/…` | `{"text": "…"}` |
| Discord | `https://discord.com/api/webhooks/…` | `{"content": "…"}` |
| ntfy.sh, or your own ntfy | `https://ntfy.sh/your-topic` | the line as plain text |

## Try it

Copy the folder somewhere of your own, and link it from a terminal (not one inside Muster):

```bash
cp -R examples/plugins/notify-webhook ~/notify-webhook
muster plugin link ~/notify-webhook
muster auth plugin notify-webhook WEBHOOK_URL
```

Muster keeps the address in your Keychain and gives it to the hook alone. Then try it without waiting for an agent:

```bash
muster plugin hook notify-webhook session.needs_you
```

which sends a sample (`sample · muster/issue-42-sample · A sample chat · has a question for you`) and prints what `./notify` said and how it ended. `muster plugin log notify-webhook` shows every run since the daemon started.

To be told only while you are away from Muster, or only of some projects, add filters to its hook in `muster-plugin.toml`, then link it again to approve the change:

```toml
[hooks.when]
away = true
project = ["muster"]
```

[docs/plugins.md](../../../docs/plugins.md) explains every line: the manifest, each event and what it carries, the filters, and the bounds every hook runs within.
