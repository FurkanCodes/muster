# ssh (submodules of `ssh.rs`)

## Overview

How the `muster` binary reaches Muster on another of the person's machines
through their own `ssh` (`PRODUCT.md` §6b, `ARC-27`). The parent
[`../ssh.rs`](../ssh.rs) holds `muster machine add|remove`, `muster list machines`,
the `--on MACHINE` switch and `muster ssh MACHINE [COMMAND…]` (probe, install
or upgrade, then `exec ssh`). This directory holds:

- `args.rs`: the one place ssh options are chosen.
- `link.rs`: the `--on` carrier (ssh to a remote `muster bridge`) and the
  bridge itself.
- `report.rs`: the remote check script and its report.

## Interface

- `How` (args.rs) — `Interactive | Background`: whether ssh may ask the
  person anything.
- `ssh_args` (args.rs) — the ssh argv before the remote command: `-t/-T`, `-C`,
  keepalives, `ConnectTimeout=10`, batch options in the background,
  ControlMaster sharing (persist 600 s), then `-- target`.
- `control_dir` (args.rs) — the private `/tmp/muster-<uid>/ssh` folder for
  shared connections, or `None` when it cannot be trusted.
- `REMOTE_PATH`, `quoted`, `ui_command`, `shell_command` (args.rs) — the PATH
  prefix every remote script sets; shell quoting; the remote `exec muster` and
  `sh -c` command lines.
- `user_for` (args.rs) — the user ssh would sign in as (`ssh -G`).
- `hint` (args.rs) — turns ssh's stderr (host key, auth, unreachable, Muster
  missing or old) into advice.
- `connect` (link.rs) — spawns `ssh … exec muster bridge`, waits for the
  `BRIDGE_READY` line, then speaks the daemon protocol over ssh's stdio
  (`Connection::over`).
- `connector` (link.rs) — a `muster_tui::Connector` that reconnects with
  `How::Background`.
- `how_here` (link.rs) — `Interactive` when stdin and stderr are terminals.
- `bridge` (link.rs) — `muster bridge` on the remote: links a forwarded SSH
  agent, connects to (or starts) the local daemon, prints the ready line,
  copies both ways, then `exit(0)`.
- `link_agent`, `read_until_ready` (link.rs) — the `AGENT_LINK` symlink to a
  forwarded agent socket; skipping login output before the ready line.
- `script`, `has`, `claude_missing`, `report` (report.rs) — the marked-line
  `muster-check:` script (agents, git, gh, curl, Claude and gh sign-in, systemd
  linger) and the human report for `--check`.

## Invariants

- The machine is reached only through `ssh` on PATH with the person's own
  config, keys and agent. Muster has no server, account or listener, and keeps
  no credential (`ARC-27`).
- All ssh options are chosen in `args.rs`. They override `~/.ssh/config`,
  which still sets host, user, keys and proxy.
- Background ssh never prompts (`BatchMode=yes`, no password prompts, strict
  host keys) and notices a dead link fast (5 s × 3; interactive 15 s × 4).
- The control folder must be a real directory owned by this uid with no group
  or other bits, or there is no sharing. Stale sockets are removed.
- Login output never reaches the protocol. The bridge prints `BRIDGE_READY` on
  its own line first; at most 64 KiB of greeting is read, and it must be ready
  within 25 s.
- ssh's stderr (16 KiB at most) is turned into advice and never written over
  the UI.
- The `--on` carrier's ssh child (`connect`) is not killed on drop, so the
  last input reaches the other end. The bridge spawns no ssh of its own.
- The agent link points only at a socket owned by the same uid, replaced
  atomically.
- Muster never copies a sign-in; the report tells the person to sign in on the
  machine (`PRD-06`). Check answers are read only from marked lines.

## Side Effects

- Spawns `ssh` (tokio child) and `ssh -G`.
- Creates `/tmp/muster-<uid>/ssh` (0700) and deletes stale sockets in it.
- `bridge` (remote): connects to or starts the local daemon
  (`crate::daemon_start::stream`), copies stdin/stdout, creates or replaces
  `AGENT_LINK` in the runtime dir, reads `SSH_AUTH_SOCK`, and exits the process.
- `report.rs` is pure string building; its script runs remotely and may call
  `security`, `gh auth status` or `loginctl`.

## Verification

- Run: `cargo check -p muster`, then `cargo nextest run -p muster ssh` (unit)
  and `cargo nextest run -p muster --test cli_ssh` (end to end).
