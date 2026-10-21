// Every key of the screen in view (`h`, or `?` from habit), or of the
// changes beside a session (`?` there), as the TUI's
// help lists them (crates/muster-tui/src/app/help.rs): under headings, each
// action by its keys as the person set them and in the words
// `muster_core::keys` says it in, and the keys no setting changes. The
// window's own keys (⌘1, ⌘K…) take the place of the terminal's tabs.

import type { Preferences } from "../ipc/generated/Preferences";
import type { ScreenId } from "../state/ui";
import { type KeyAction, labelOf } from "./keymap";

export interface HelpEntry {
  keys: string;
  what: string;
}

export interface HelpSection {
  heading: string;
  entries: HelpEntry[];
}

/** An action, said as `muster_core::keys` says it, or otherwise here; or fixed keys. */
type Entry = KeyAction | [KeyAction, string] | { fixed: string; what: string };

const WHAT: Record<KeyAction, string> = {
  help: "these keys",
  add_project: "add a project",
  new_worktree: "a new worktree for the project",
  new_session: "a new session in the worktree, or a project's checkout",
  show_tickets: "the project's tickets",
  full_screen: "the session full screen",
  show_changes: "what the worktree or session has changed, and its turns",
  shell: "a shell in the workspace, beside the session (again: put it away)",
  stop_session: "stop the session",
  restart_session: "start an ended session again",
  close: "close the session; remove the worktree or the project, after asking",
  rename: "rename the session's chat",
  fold: "fold or unfold a project or a worktree (on a session, its worktree)",
  checks: "run the project's checks in the worktree",
  review: "have Claude Code review the worktree's change",
  tidy: "have an agent tidy the worktree's commits",
  move_branch: "move the worktree's branch onto its base, after asking",
  open_pull_request: "push the branch and open a draft pull request",
  pull_request_in_browser: "the branch's pull request in the browser",
  move_up: "move up (as ↑)",
  move_down: "move down (as ↓)",
  last_alarm: "the session that needed you last, as notified",
  fold_list: "fold the sidebar away, or bring it back, as ⌘\\",
  refresh_tickets: "fetch the tickets again",
  another_agent: "start another agent on it, in a worktree of its own",
  ticket_in_browser: "the ticket in the browser",
  close_ticket: "close the ticket in its tracker, after asking; then clean up",
  read_settings: "read the settings again, after editing a file by hand",
  add_plugin: "add a plugin from a git repository or a folder, after showing what it runs",
  update_plugin: "update the plugin, after showing what it runs then",
  remove_plugin: "remove the plugin, after asking",
  put_back: "on a turn, put the files back as before it, after asking",
  changes_again: "ask git again",
  close_changes: "back to the terminal",
  leave_session: "back to the terminal",
  changes_help: "these keys",
};

const fixed = (keys: string, what: string): Entry => ({ fixed: keys, what });

const ANYWHERE: Entry[] = [
  fixed("⌘K", "go to a session, a ticket or a setting, or do something"),
  fixed("⌘1", "Needs you"),
  fixed("⌘2", "All sessions"),
  fixed("⌘3", "Tickets"),
  fixed("⌘,", "Settings"),
  fixed("⌘\\", "fold the sidebar away, or bring it back"),
  "fold_list",
  "last_alarm",
  "help",
];

const IN_A_SESSION: HelpSectionOf = [
  "In a session",
  [
    fixed("⇧Q", "back to the list (or a click on it)"),
    fixed("⌘ shortcuts", "the window's, as everywhere"),
    fixed("any other key", "goes to its program, ⎋ and ⇥ too"),
  ],
];

type HelpSectionOf = [string, Entry[]];

const SESSIONS: HelpSectionOf[] = [
  [
    "On the list",
    [
      fixed("↑ ↓", "move"),
      "move_up",
      "move_down",
      fixed(
        "⏎",
        "type in the session beside the list; on a worktree a new session, on a project a new worktree",
      ),
      "full_screen",
      "fold",
      "add_project",
      "new_worktree",
      "new_session",
      "show_tickets",
      "show_changes",
      "shell",
      "stop_session",
      "restart_session",
      "rename",
      "close",
      "checks",
      "review",
      "tidy",
      "move_branch",
      "open_pull_request",
      "pull_request_in_browser",
      fixed("right-click", "what can be done on a line, with its keys"),
    ],
  ],
  IN_A_SESSION,
  ["Anywhere", ANYWHERE],
];

const QUEUE: HelpSectionOf[] = [
  [
    "On the queue",
    [
      fixed("↑ ↓", "move"),
      "move_up",
      "move_down",
      fixed("⏎", "type in the session"),
      "full_screen",
      ["show_changes", "what its workspace has changed"],
      ["new_session", "a new session in its worktree"],
      "add_project",
      "shell",
      "stop_session",
      "restart_session",
      "rename",
      ["close", "close the session"],
      fixed("right-click", "what can be done on a line, with its keys"),
    ],
  ],
  IN_A_SESSION,
  ["Anywhere", ANYWHERE],
];

const TICKETS: HelpSectionOf[] = [
  [
    "On the tickets",
    [
      fixed("↑ ↓", "move"),
      "move_up",
      "move_down",
      fixed("⏎", "open the agent on the ticket, or start the first"),
      "another_agent",
      "ticket_in_browser",
      "close_ticket",
      "refresh_tickets",
    ],
  ],
  [
    "Choosing an agent",
    [
      fixed("⇥ ⇧⇥", "the agent"),
      fixed("← →", "what its worktree is made on top of"),
      fixed("↑ ↓", "which project, where several share the tickets"),
    ],
  ],
  ["Anywhere", ANYWHERE],
];

const SETTINGS: HelpSectionOf[] = [
  [
    "On the settings",
    [
      fixed("↑ ↓", "move"),
      "move_up",
      "move_down",
      fixed("⏎ space", "turn the setting on or off, go to its next choice, or open its dialog"),
      fixed("← →", "a smaller or a larger choice"),
      "read_settings",
    ],
  ],
  [
    "On the plugins",
    [fixed("⏎ space", "turn the plugin on or off"), "add_plugin", "update_plugin", "remove_plugin"],
  ],
  ["Anywhere", ANYWHERE],
];

// The changes beside a session (crates/muster-tui/src/app/help.rs, CHANGES).
const CHANGES: HelpSectionOf[] = [
  [
    "In the changes",
    [
      fixed("↑ ↓  j k", "choose a file or a turn; in a diff, scroll"),
      fixed("⏎ → l", "its diff"),
      fixed("PgUp PgDn", "a page"),
      fixed("Home End", "the first or the last; in a diff, its top or bottom"),
      fixed("n p", "in a diff, the next and the last hunk"),
      fixed("⎋ ← h", "back from a diff; on the list, back to the terminal"),
      fixed("⇥ ⇧⇥", "not committed, on the branch, turns"),
      "changes_again",
      "put_back",
      "close_changes",
      "leave_session",
      "changes_help",
    ],
  ],
  ["Anywhere", ANYWHERE],
];

const BY_SCREEN: Record<ScreenId | "changes", HelpSectionOf[]> = {
  "needs-you": QUEUE,
  "not-seen": QUEUE,
  working: QUEUE,
  all: SESSIONS,
  tickets: TICKETS,
  settings: SETTINGS,
  changes: CHANGES,
};

/**
 * The keys of `screen`, the person's, narrowed to those whose keys or words
 * hold `typed`, ignoring case: a heading with none goes, and so does an
 * action the person took every key from.
 */
export function helpFor(
  screen: ScreenId | "changes",
  preferences: Preferences | null,
  typed = "",
): HelpSection[] {
  const wanted = typed.trim().toLowerCase();
  const said = (entry: Entry): HelpEntry => {
    if (typeof entry === "string") return { keys: labelOf(preferences, entry), what: WHAT[entry] };
    if (Array.isArray(entry)) return { keys: labelOf(preferences, entry[0]), what: entry[1] };
    return { keys: entry.fixed, what: entry.what };
  };
  return BY_SCREEN[screen]
    .map(([heading, entries]) => ({
      heading,
      entries: entries
        .map(said)
        .filter((entry) => entry.keys !== "none")
        .filter(
          (entry) =>
            !wanted ||
            entry.keys.toLowerCase().includes(wanted) ||
            entry.what.toLowerCase().includes(wanted),
        ),
    }))
    .filter((section) => section.entries.length > 0);
}
