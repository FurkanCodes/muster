# Drift report: EGM README contracts, 2026-10-26

Five read-only reviewers compared each of the 79 READMEs written by the EGM bootstrap with the code they describe. 26 READMEs show no drift. Most of the rest are interface lists missing secondary public items. A few are statements that are wrong.

## Where the code or a rule may be at fault

Most drift below is a README to correct. The items in this section also touch the code, code comments or a rule, so they need a decision rather than a doc edit:

- **`ARC-33` and free terminals.** `ARC-33` (`ARCHITECTURE.md:452`) says a free terminal is in one space or none. The code comments at `crates/muster-daemon/src/supervisor/spaces.rs:2-3` and `crates/muster/src/verbs/spaces.rs:2-3` say "exactly one". The code itself (desktop `terminalIn(app, null)`, mock `space: null`) follows the rule.
- **`ARC-01` and `muster-core` ids.** Each id type's `new()` calls `Ulid::generate()`, which reads the clock and randomness (`crates/muster-core/src/ids.rs:52-54`). The module comment admits it; the rule's text does not.
- **`ARC-03` in the desktop link.** `apps/desktop/src-tauri/src/link/actor.rs` reads `tokio::time::Instant::now()` directly in six places, outside `link::now`/`link::monotonic`.
- **`ARC-14` and Gemini spend.** `crates/muster-daemon/src/spend/gemini.rs:84-86` reads a whole recording with `std::fs::read`, unlike the chunked readers.
- **Rule IDs in the notch's comments.** Comments cite `ARC-13`/`ARC-14` for "no timers while hidden / nothing polls" (`crates/muster-notch/src/board.rs:134`, `hover.rs:67`, `mac/events.rs:2`). Those rules are task ownership and bounds.
- **Stale CI claim.** `apps/desktop/scripts/bindings.sh:4` says CI fails on a bindings diff. No workflow checks it.
- **Stale doc comment.** `crates/muster-tui/src/view/bottom_bar.rs:4` names `hint_at`, which no longer exists (`hint_areas`).

## crates/README.md

No drift detected.

## crates/muster-core/README.md

- Invariant "outside tests, nothing touches the … clock": every id type's `new()` calls `Ulid::generate()`, which reads the system clock and randomness (`crates/muster-core/src/ids.rs:52-54`). The module doc says so itself: "the one thing this crate takes from the system" (`ids.rs:6-8`).
- "`frozen/v28` … `frozen/v58`": there is no `frozen/v29`. `FROZEN` goes from 28 straight to 30 (`crates/muster-core/tests/frozen_protocol.rs:65-66`).

## crates/muster-core/src/README.md

- "`lib.rs` declares 40 public modules": it declares 39 `pub mod` lines (`crates/muster-core/src/lib.rs:9-47`).
- Invariant "No module touches … the clock": `ids.rs` reads the clock and randomness through `Ulid::generate()` (`ids.rs:52-54`, `6-8`).

## crates/muster-core/src/config/README.md

- `MusterConfig::parse` also reads `[names]`, `[worktrees]` (`shared_builds`) and `[delegation]` (`allowed`, `at_once`, `depth`), which the table list leaves out (`crates/muster-core/src/config/preferences.rs:58-69, 118-144, 224-228, 248`).
- Not listed: the public `agents::parse` (`agents.rs:281`), `AgentError` (`agents.rs:246`, carried by `ConfigError::Agent` at `config.rs:147-150`) and `AgentDefinition::added` (`agents.rs:232`).

## crates/muster-core/src/keys/README.md

- Invariant "each plugin's suggestions in plugin-id order": `Keymap::with_panels` binds suggestions "in the order given" by the caller and does not sort by id (`crates/muster-core/src/keys/panels.rs:76-101`). This module does not enforce plugin-id order.

## crates/muster-core/src/plugins/README.md

- Not listed: the public const `own::MANIFESTS`, the compiled-in manifest texts (`crates/muster-core/src/plugins/own.rs:47`).
- The reserved-id invariant names only `github`, `gitlab`, `jira` and `linear`. `is_reserved` also reserves `muster` and every id starting with `muster-` (`crates/muster-core/src/plugins.rs:30-32, 47-49`).

## crates/muster-core/src/plugins/manifest/README.md

- The placeholder rule is wrong. A session placeholder is refused if *any* of the hook's events is not a session's (`ManifestError::NoSession`, `crates/muster-core/src/plugins/manifest/hooks.rs:119-126`). The "none of its events" rule applies only to the `harness` and `reason` filters (`hooks.rs:155-166`).

## crates/muster-core/src/protocol/README.md

- Not listed: `Reading` (`Screen`/`LastReply`, used by `Command::ReadSession`, `crates/muster-core/src/protocol.rs:723`), `ChangedBy` (`protocol/plugins.rs:103`), `Required` (`plugins.rs:344`) and `PreviewKind` (`plugins.rs:372`).
- "Sets exist for v28 through v58": v29 has none (`crates/muster-core/tests/frozen_protocol.rs:65-66`).

## crates/muster-state/README.md

No drift detected.

## crates/muster-state/src/README.md

No drift detected.

## crates/muster-state/src/run/README.md

No drift detected.

## crates/muster-state/src/session/README.md

No drift detected.

## crates/muster-state/src/session/machine/README.md

No drift detected.

## crates/muster-model/README.md

No drift detected.

## crates/muster-model/src/README.md

- Invariant "A shell, the person's own terminal and a plugin's panel are never counted as agents": `Model::agents_on` filters out only plugin panels, so a shell in a ticket's worktree is returned as one of the ticket's agents (`crates/muster-model/src/lib.rs:237-251`). `agents_alive_in` and `counts` do exclude shells (`lib.rs:355-361, 448-459`).

## crates/muster-client/README.md

- Invariant "All frames are checked against the daemon's protocol version before sending": the opening `Hello` is written directly and never goes through `sendable`, in `Connection::over` (`crates/muster-client/src/connection.rs:124-126`) and in `daemon_identity` (`connection.rs:345-346`).

## crates/muster-client/src/README.md

- Invariant "Every outgoing frame goes through `sendable`": the handshake `Hello` bypasses it (`connection.rs:124-126, 345-346`).
- "`TransportError::Frame` (decode error)" is too narrow. It wraps the whole `FrameError`, including `TooLarge` and `Encode` (`crates/muster-client/src/transport.rs:15-16`, `crates/muster-core/src/frame.rs:19-26`).
- Not listed: `Connection::welcome()` (`connection.rs:157`).

## crates/muster-daemon/README.md

- The Interface leaves out much of the public API. Missing exports: `SignIn` (`lib.rs:96`), `jira_site` (`lib.rs:140`), `bitbucket_account` (`lib.rs:89`), `plugin_account` (`lib.rs:163`), `LINEAR_KEY_ACCOUNT` (`lib.rs:160`) and `HANDOFF_FORMAT` (`lib.rs:256`). Missing re-exports: `LogLimits`, `PluginRunError`, `KEYCHAIN_SERVICE`/`KEYCHAIN_ACCOUNT` and `ServiceError` (`lib.rs:74-77`).

## crates/muster-daemon/src/README.md

- `pub mod config` leaves out the public structs `Loaded` (`config.rs:26`), `Sources` (`config.rs:58`) and `OwnSettings` (`config.rs:139`), which `load`, `load_with_sources` and `own_settings` return.

## crates/muster-daemon/src/githost/README.md

No drift detected.

## crates/muster-daemon/src/handoff/README.md

No drift detected.

## crates/muster-daemon/src/harness/README.md

- Not listed: `Place` (`harness.rs:451`, a field of `Launch`), `CheckCommands` (`harness.rs:109`) and `TitleState` (`harness.rs:142`, what `read_title` returns).
- claude_stream.rs: the function actually used is `parse_events` (`claude_stream.rs:92`, called from `runs/work/agents.rs:14,351`), not `parse_line`. `TurnOutcome` (`claude_stream.rs:59`) is also missing.

## crates/muster-daemon/src/plugins/README.md

- fetch.rs: the module root re-exports `Fetched, Job, fetch` (`plugins.rs:31`). `Fetched` (`fetch.rs:47`), the result handed back to the loop, is not listed.
- Not listed `pub(crate)` items: `job()` (`plugins.rs:358`), `on_path()` (`plugins.rs:557`), `Reading` (`plugins.rs:158`), and `Registry::running_ids`/`folders`/`incoming` (`plugins.rs:194,218,312`).

## crates/muster-daemon/src/plugins/hooks/README.md

- `Given<'a>` has the fields `secrets`, `limits`, `log`, `cut` and `started` (`run.rs:31-39`), not "environment basics". The basics are read at spawn time from `trackers::plugin::basics(std::env::vars_os())` (`run.rs:194`).
- All three items are `pub(super)` (`run.rs:31,43,78`), which the README does not say.

## crates/muster-daemon/src/pty/README.md

- Side Effects names a reader and a writer thread per terminal. There is also a `muster-pty-wait` thread (`pty.rs:318-323`, `354-358`), so there are three.
- `PtyHandle` methods not listed: `pid`, `focus_now`, `resize_now` and `resume_if_suspended` (`pty.rs:182,221,227,246`).

## crates/muster-daemon/src/runs/README.md

- `NotYetBuilt` (`runs.rs:98-100`) and `FixedCredentials` (`credentials.rs:91-93`) are `#[cfg(test)]` only, yet are listed as interface.
- `image_for` never reads `MUSTER_RUN_IMAGE` (`image.rs:34-66`). The override is read in `lib.rs::run_work` (`lib.rs:609-611`) and held as `Inner::image` (`work.rs:50-51`).
- Not listed: `KEYCHAIN_ACCOUNT` (`credentials.rs:15`), `MISSING` (`credentials.rs:104`), and provision.rs's `Identity`, `identity` and `last_lines` (`provision.rs:42,153,172`).

## crates/muster-daemon/src/runs/work/README.md

- `build`, `check` and `review` are methods on the private `Inner` (`agents.rs:36-37,163,209`), not on `DockerWork`, which only wraps `Arc<Inner>` (`work.rs:34-36`).
- Side Effects leaves out that `review` posts the verdict as a pull-request review comment (`self.host.review_comment`, `agents.rs:305-307`).

## crates/muster-daemon/src/server/README.md

- Not listed: `pub(crate) async fn write_frames` (`outbox.rs:385`), the per-connection writer task that `server.rs:60` spawns.
- `Outbox` also has `is_behind`, `caught_up` and `clear` (`outbox.rs:313,319,339`); `new` returns `Arc<Outbox>` (`outbox.rs:281`).

## crates/muster-daemon/src/spend/README.md

- The invariant "a record is read in chunks… never the whole file (`ARC-14`)" is broken by Gemini: `gemini.rs::read` loads the whole file with `std::fs::read` (`gemini.rs:84-86`). The README's own Gemini bullet contradicts the invariant.

## crates/muster-daemon/src/store/README.md

- "Async code never touches SQLite directly" does not hold at startup. `Store::open` (migrations and the `VACUUM INTO` backup) runs inside the async `run` (`lib.rs:288`, `store.rs:104-123`), before `StoreHandle::spawn` (`lib.rs:289`).
- `Store` methods not listed (hidden by the trailing "…"): `insert_project`, `delete_project`, `update_ticket`, `project_by_root`, `remove_workspace`, `reopen_session`, `file_session` and `status_history` (`store.rs:143,185,214,235,251,327,353,386`).

## crates/muster-daemon/src/supervisor/README.md

- "Every project and every free terminal is in exactly one space" contradicts `ARC-33`, which allows a free terminal in one space or none (`ARCHITECTURE.md:452`, also `:34`). The code comment at `supervisor/spaces.rs:2-3` has the same wrong wording.
- `SupervisorHandle::stop` is `#[cfg(test)]` (`handle.rs:269-270`). Not listed: `hand_over` and `adopt` (called at `lib.rs:562,578`), and `ended`, `spend_targets`, `record_interrupted`, `record_stopped` and `hold_keys` (`handle.rs:231,371,388,397,476`).
- `Subscription` is `pub(super)` (`handle.rs:15`), yet it is presented as how the rest of the daemon reaches the Supervisor.
- Not listed: the module root's re-exports `TerminalLink` and `WatchGuard` (`supervisor.rs:68`), both used by `server.rs:24`.

## crates/muster-daemon/src/timeline/README.md

- "The person's index, refs and objects are only read" holds only for snapshots. `move_onto` runs `git rebase --onto` in the workspace and moves its branch (`moving.rs:174-191`).
- `Store` methods not listed: `open_with_index`, `inside` and `inside_allowing` (`timeline.rs:135,195,209`).

## crates/muster-daemon/src/trackers/README.md

- jira.rs and linear.rs items used by `muster auth` are not listed: `jira::MCP_ACCOUNT`, `KeptToken`, `token_account` and `EMAIL_VARIABLE` (`jira.rs:32,57,50,41`), and `linear::MCP_ACCOUNT` and `KEY_ACCOUNT` (`linear.rs:27-28`).
- plugin.rs: `is_basic`, `basics` and `refusal` (`plugin.rs:64,74,343`) are not listed.

## crates/muster-daemon/src/trackers/plugin/README.md

- `Limits` has the fields `hello`, `request` and `idle` (`process.rs:35-42`); there is no line length. The 4 MiB cap is `muster_core::ticket_protocol::MOST_LINE` (`process.rs:16`).
- `Lines::new(reader, most)` has no default limit; the caller passes it (`streams.rs:24`).
- `Process::log` is `#[cfg(test)]` (`process.rs:126-127`).
- `Log` holds more than the plugin's stderr: plugin hook runs append to the same log (`plugin.rs:292-297`, `plugins/hooks/run.rs:53`).

## crates/muster/README.md

- The dependency list leaves out `tracing-subscriber` (`crates/muster/Cargo.toml:29`, used by `daemon_run.rs:14` for `MUSTER_LOG`). The dev-dependency list leaves out `tempfile` (`Cargo.toml:34`).

## crates/muster/src/README.md

- commands.rs: only `connect` and `status` are listed. The `pub(crate)` handlers `add_project` (`:205`), `remove_project` (`:244`), `show_config` (`:263`), `stop_daemon` (`:378`), `list_issues` (`:414`), `check` (`:663`), `review` (`:744`), `open_pull_request` (`:767`), `store_claude_token` (`:629`) and the guard `no_agent_starts_one` (`:516`) are not.
- Side Effects names SIGUSR1 as the only signal sent. `muster daemon stop` also sends SIGTERM (`commands.rs:391`).
- "An upgrade checks SHA-256 and the attestation": the attestation check is skipped when `MUSTER_SKIP_ATTESTATION` is set, and for a stand-in release (`upgrade.rs:112-119`). Only the SHA-256 check always runs.

## crates/muster/src/plugins/README.md

No drift detected.

## crates/muster/src/ssh/README.md

- "The bridge's ssh child is not killed on drop": that ssh child belongs to the client-side carrier `connect` (`.kill_on_drop(false)`, `ssh/link.rs:60`). `bridge` (`link.rs:161-201`) spawns no ssh child.

## crates/muster/src/upgrade/README.md

- The attestation check is skipped for a stand-in release and when `MUSTER_SKIP_ATTESTATION` is set (`upgrade.rs:112-119`), though the README says it always runs. Side Effects also leaves out the environment reads `MUSTER_RELEASES` (`upgrade.rs:82`, called from `app.rs:32`) and `MUSTER_SKIP_ATTESTATION` (`upgrade.rs:116`).

## crates/muster/src/verbs/README.md

- `TermCommand` has only `New` (`verbs/terminals.rs:16-36`). Rename and close are the top-level verbs `muster rename` and `muster close` (`verbs.rs:157-168`), implemented by `terminals::rename`/`close` (`terminals.rs:129,158`). `muster term rename` and `muster term close` do not exist.
- `muster wait` exit codes: besides 0 / 3 / 4, `Waited::code` returns 124 when time runs out (`TIMED_OUT`) and 5 when an agent stalls (`STALLED`) (`wait.rs:61,63,112-113`; `verbs.rs:203` documents 124).
- "Ticket verbs refuse inside an agent's session" holds only for `close`, `assign`, `unassign` and starting an agent (`verbs/tickets.rs:206,237,262,290`). Reading a ticket (`tickets.rs:106`) and `open` (`tickets.rs:183`) do not refuse.
- "Each terminal outside a project is in exactly one space (`ARC-33`)": `ARC-33` allows a free terminal in one space or none (`ARCHITECTURE.md:452`). The doc comment in `verbs/spaces.rs:2-3` says the same wrong thing.

## crates/muster-tui/README.md

No drift detected.

## crates/muster-tui/src/README.md

No drift detected.

## crates/muster-tui/src/app/README.md

- File counts: of the 98 `.rs` files, 52 are `*_tests.rs` and one is `tests.rs`, leaving 45 production files, not "about 46". Five test modules are declared by `#[path]` in their feature files, not in `app.rs`: `rename.rs:62`, `spaces.rs:577`, `stacks.rs:166`, `tidy.rs:74` and `timeline.rs:207`.
- `on_changes`, `on_timeline` and `on_diff` are in `app/changes.rs` (`:470,486,506`), not timeline.rs. timeline.rs holds `ask_timeline`, `ask_undo_plan`, `on_undo_plan`, `undo_key` and `on_undone` (`:54,66,82,134,169`).

## crates/muster-tui/src/runtime/README.md

- "Synchronized output holds every draw until the shown session finishes its frame": the hold is capped at `FRAME_WAIT` = 150 ms (`runtime.rs:119-121`, `FrameHold::may_draw` at `:464`).

## crates/muster-tui/src/view/README.md

- `hint_at` (bottom_bar.rs) does not exist. A click on a hint goes through `hint_areas` (`view/bottom_bar.rs:150`), next to `hint` (`:186`). `hint_at` survives only in a stale doc comment (`bottom_bar.rs:4`).

## crates/muster-tui/src/view/dialogs/README.md

No drift detected.

## crates/muster-tui/src/view/settings/README.md

No drift detected.

## crates/muster-notch/README.md

- The Dependencies line leaves out `objc2-core-foundation`, which is a macOS-only dependency (`crates/muster-notch/Cargo.toml:48`, used in `src/mac/shape.rs:11`).

## crates/muster-notch/src/README.md

- `run`: the README says it "returns at once if … no screen has a notch". The code returns early only when there is no notch **and** the program is not running as Muster's app. Inside the app it carries on without a notch so notifications still work (`src/mac.rs:84-90`).
- `ARC-13` is cited for "hidden boards have no timer, and time labels wake only at their next boundary". In `ARCHITECTURE.md:432`, `ARC-13` is "every task and thread has an owner… every wait has a deadline". It says nothing about timers or time labels. The code comments repeat the same mis-citation (`src/board.rs:134`, `src/hover.rs:67`).
- The Interface leaves out the public `board::ago(then, now) -> String` (`src/board.rs:321`) and the public `geometry::Segment` enum (`src/geometry.rs:241`). `Segment` is the type that `outline` returns.
- Side Effects lists only `Command::Reveal` as sent to the daemon. The link also sends `ClientFrame::Subscribe` (`src/link.rs:125`) and `ClientFrame::Notifier { ready }` (`src/link.rs:190`).

## crates/muster-notch/src/mac/README.md

- "Nothing polls the pointer… hidden panels keep neither (`ARC-13`, `ARC-14`)" cites the wrong rules. `ARC-13` is task and thread ownership and `ARC-14` is bounded queues and caches (`ARCHITECTURE.md:432-433`); neither covers polling or timers. The same citation is in `src/mac/events.rs:2`.
- "Labels and logos are touched only when what they show changes" is only partly true. `Label::show` calls `setFrame` and `setHidden(false)` on every call; only the text and colour wait for a change (`src/mac/look.rs:78-89`). `Logo::show` also sets the frame every time (`src/mac/look.rs:195-196`).

## scripts/README.md

- The Overview says every `check-*` script exits "with a message naming the rule they enforce". `check-no-secrets.sh` prints "Something shaped like a real credential is in the repository" and names no rule; `ARC-20`/`PRD-06` appear only in a comment (`scripts/check-no-secrets.sh:3,17`).

## scripts/hooks/README.md

No drift detected.

## README.md (root: *How it is built* and *Repository blueprint*)

- Global side effects says the runtime folder holds "two private Unix sockets, a lock, the daemon log". It also holds the notch's lock, `notch.lock` (`crates/muster-notch/src/lib.rs:50`), and the ssh-agent link, `ssh-agent.sock` (`crates/muster-core/src/machines.rs:21-23`).

## apps/README.md

No drift detected.

## apps/desktop/README.md

- "The CSP allows only `self` and IPC": it also allows `style-src 'unsafe-inline'` and `img-src data:` (`apps/desktop/src-tauri/tauri.conf.json:31-35`). Nothing from outside is allowed, so `PRD-10` holds.

## apps/desktop/scripts/README.md

- "CI runs it and fails if the files change" and "`git diff src/ipc/generated` should be empty in CI": no workflow runs `bindings.sh` or checks `src/ipc/generated`. `.github/workflows/ci.yml:22-110` has only Rust jobs, and `build-app.yml:54-63` only installs and builds. The same stale claim is in `apps/desktop/scripts/bindings.sh:4`.

## apps/desktop/src-tauri/README.md

- "CSP: `self` only, IPC connects": `tauri.conf.json:32-33` also allows `'unsafe-inline'` styles and `data:` images.

## apps/desktop/src-tauri/src/README.md

- Not listed: the public `Gone` struct (`contract.rs:86-93`, carried by `Patch.gone`) and `Link::is_connected` (`link.rs:195`).

## apps/desktop/src-tauri/src/folders/README.md

No drift detected.

## apps/desktop/src-tauri/src/link/README.md

- "`ARC-03`: the wall and monotonic clocks are read only in `link::now` and `link::monotonic`" is broken. `actor.rs` calls `tokio::time::Instant::now()` directly (lines 419, 434, 443, 455, 558, 578) and `last.at.elapsed()` (line 556).
- Not listed: `pub(super)` `Screen` (`questions.rs:26`), which `streams.rs:23` uses.

## apps/desktop/src/README.md

No drift detected.

## apps/desktop/src/app/README.md

- "Every key goes to the program except Q/⇧Q and the ⌘ shortcuts" leaves out the panel (prefix) key. The app takes the panel key and the key after it (`app/keys.ts:185`), and the xterm handler keeps them from the program (`terminal/kept.ts:107`).
- Not listed: `KeyHandlers` (`app/keys.ts:38`) and the `App` interface (`app/context.ts:11`).

## apps/desktop/src/flows/README.md

- `rename` (`flows/actions.ts:139`, used by the palette at `catalogue.ts:117,122`) is missing from the verb list.
- "Only while the daemon link is up": the help key is bound whatever the link state (`flows/Flows.tsx:56-57`).
- Not listed: `KeyAction`, `Key`, `Keyed` (`flows/keymap.ts:12,14,24`), `Question` (`Questions.tsx:8`). The dialog components (`Tidy`, `Move`, `Stop`, `RemoveWorktree`, `RemoveProject`, `SpaceName`, `RemoveSpace`, `FileTerminal`) appear only by file name.

## apps/desktop/src/ipc/README.md

No drift detected.

## apps/desktop/src/ipc/mock/README.md

- "`ARC-33`: everything is in exactly one space" is wrong. A terminal in no project started with `space: null` gets no space (`ipc/mock/sessions.ts:75-76`), and `ARC-33` itself allows one or none (`ARCHITECTURE.md:452`).
- Not listed: `ByHandWorld` (`byHand.ts:15`), `folderOf`/`HOME` (`spaces.ts:19,56`), `HOME` (`folders.ts:6`), `ProjectRecord` (`settings.ts:34`), `TicketScenario` (`tickets.ts:18`), and `workingDayPlugins`, `fetchable`, `Fetchable`, `WEBHOOK_FOLDER`, `YOUTRACK_GIT` (`plugins.ts:56-216`).

## apps/desktop/src/keys/README.md

- Not listed: `keptAs` (`keymap.ts:60`), `panelWhat` (`keymap.ts:92`), `KeysWarning` (`keymap.ts:119`) and `PanelKeyHandler` (`panelKey.ts:12`).

## apps/desktop/src/lib/README.md

No drift detected.

## apps/desktop/src/panels/README.md

- `PanelBoxes` and `PaneActions` do not exist as exports. `PanelBoxes.tsx` exports `TerminalAlone`, `PlacedPanel`, `SessionPanel`, `PluginPanel` and `PanelLine` (lines 75, 108, 157, 207, 264); `PaneActions.tsx` exports `ShellButton` (line 14).
- `Layout` methods not listed: `parse`, `default`, `equals`, `swapped` and `without` (`layout.ts:365,389,397,434,458`).
- Not listed: `running`, `giveKeys` (`beside.ts:31,61`); `toggleFullScreen`, `usePanelHandlers` (`usePanelKeys.ts:18,29`); `neighbour` (`useArranging.ts:29`); `PANELS`, `panelsIn`, `contains`, `PlacedPanelName` (`layout.ts:24,46,208,214`); `Carried`, `Carrying` (`carry.ts:12,15`).

## apps/desktop/src/screens/README.md

No drift detected.

## apps/desktop/src/screens/queue/README.md

No drift detected.

## apps/desktop/src/screens/sessions/README.md

- Folds are said to live "in local storage". They go through `state/kept.ts` (`folds.ts:6,19,33`), which after `keepIn` writes `window.json` through the Rust side (`state/kept.ts:46-58`). Local storage is used only in mock builds and before `keepIn`.

## apps/desktop/src/screens/settings/README.md

- Not listed: `SettingsState` (`context.ts:45`), `Row` and `RowKind` (`model.ts:37,47`), and the `Row.tsx` parts `SettingRow`, `SettingText`, `Control`, `Group`, `Page`, `Footnote`, `Unreadable` and `openSelect` (`Row.tsx:23-211`).

## apps/desktop/src/screens/tickets/README.md

No drift detected.

## apps/desktop/src/session/README.md

No drift detected.

## apps/desktop/src/session/changes/README.md

No drift detected.

## apps/desktop/src/spaces/README.md

- "`ARC-33`: everything lives in one space" contradicts the code and the rule. `terminalIn(app, null)` starts a terminal in no space (`app/Sidebar.tsx:142`, `spaces/spaces.ts:109-125`), `spaceToJoin` can return null (`spaces.ts:52-53`), and `ARC-33` allows one or none (`ARCHITECTURE.md:452`).

## apps/desktop/src/state/README.md

- The `createUi()` state list leaves out `changesTurn`, `compose`, `drawnIn`, `lastAgent` and `spacesOpen` (`state/ui.ts:58,68,74,81,98`).

## apps/desktop/src/terminal/README.md

No drift detected.

## apps/desktop/src/ui/README.md

- Not listed: in `status.tsx`, `MarkSpace`, `BranchName`, `Found`, `Missing`, `Version` and `pullRequestWord` (lines 37, 42, 55, 60, 65, 145); `fileKind`, `FileKind` (`FileRow.tsx:21,26`); `LINE`, `HUNK`, `DiffLine` (`diff.ts:2,3,5`); `AgentOption` (`AgentChoice.tsx:12`), `Mood` (`Kep.tsx:6`) and `BannerTone` (`Banner.tsx:17`).
