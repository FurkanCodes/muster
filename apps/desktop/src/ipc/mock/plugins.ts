// The plugins of the mock's working day, as Settings › Plugins draws them:
// Muster's own four (crates/muster-core/src/plugins/own.rs), the
// repository's example plugins (examples/plugins), and one changed since
// it was approved; and what adding one from an address or a folder would
// fetch.

import type { PluginDetails } from "../generated/PluginDetails";
import type { PluginInfo } from "../generated/PluginInfo";
import type { PluginSource } from "../generated/PluginSource";
import type { TicketSetting } from "../generated/TicketSetting";
import { NOW } from "./now";

const DAY = 24 * 60 * 60 * 1000;

/** A hash-shaped hex string for `seed`: 64 digits, the same each time (FNV-1a, round). */
function sha(seed: string): string {
  let out = "";
  let hash = 0x811c9dc5;
  for (let round = 0; out.length < 64; round += 1) {
    for (const char of `${seed}#${round}`) {
      hash ^= char.codePointAt(0) ?? 0;
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    out += hash.toString(16).padStart(8, "0");
  }
  return out.slice(0, 64);
}

function details(parts: Partial<PluginDetails> & { name: string }): PluginDetails {
  return {
    version: "0.4.1",
    description: null,
    muster: "0.4.1",
    requires: [],
    how_to_get: null,
    env: [],
    secrets: [],
    panels: [],
    tickets: null,
    hooks: [],
    sha256: sha(parts.name),
    ...parts,
  };
}

function tickets(name: string, noun: string, id: string, settings: TicketSetting[] = []) {
  return { name, noun, command: ["muster", "plugin-run", id], settings };
}

function own(id: string, plugin: PluginDetails): PluginInfo {
  return { id, source: "builtin", enabled: true, problem: null, details: plugin, changed: null };
}

const NO_FILTER = { project: [], harness: [], reason: [], away: null };

export const WEBHOOK_FOLDER = "/Users/you/muster/examples/plugins/notify-webhook";
export const YOUTRACK_GIT = "https://github.com/acme/muster-youtrack";

function webhook(): PluginDetails {
  return details({
    name: "Notify by webhook",
    version: "0.1.0",
    description:
      "Posts project · branch · chat · what happened to a webhook when an agent needs you, finishes or fails.",
    requires: [
      { program: "curl", found: true },
      { program: "sed", found: true },
    ],
    how_to_get: "curl and sed come with macOS and Linux.",
    secrets: ["WEBHOOK_URL"],
    hooks: [
      {
        id: "notify",
        on: ["session.needs_you", "session.finished", "session.error"],
        command: ["./notify"],
        timeout: 15,
        when: NO_FILTER,
      },
    ],
  });
}

function youtrack(version: string): PluginDetails {
  return details({
    name: "YouTrack tickets",
    version,
    description: "Lists a YouTrack project's open issues as tickets.",
    muster: "0.4.0",
    requires: [
      { program: "curl", found: true },
      { program: "jq", found: false },
    ],
    how_to_get: "brew install jq",
    env: ["YOUTRACK_SITE"],
    secrets: ["YOUTRACK_TOKEN"],
    tickets: {
      name: "YouTrack",
      noun: "issue",
      command: ["./youtrack", "--serve"],
      settings: [
        { key: "site", label: "YouTrack site", required: true },
        { key: "query", label: "A query", required: false },
      ],
    },
    sha256: sha(`youtrack ${version}`),
  });
}

/** Every plugin, as the daemon lists them: Muster's own, then the person's. */
export function workingDayPlugins(): PluginInfo[] {
  return [
    own(
      "github",
      details({
        name: "GitHub issues",
        description: "A repository's GitHub issues, through your own gh login.",
        requires: [{ program: "gh", found: true }],
        how_to_get: "brew install gh, then gh auth login",
        tickets: tickets("GitHub", "issue", "github"),
      }),
    ),
    own(
      "gitlab",
      details({
        name: "GitLab issues",
        description: "A repository's GitLab issues, through your own glab login.",
        requires: [{ program: "glab", found: true }],
        how_to_get: "brew install glab, then glab auth login",
        tickets: tickets("GitLab", "issue", "gitlab"),
      }),
    ),
    own(
      "jira",
      details({
        name: "Jira",
        description: "A Jira project's tickets, signed in with muster auth jira.",
        tickets: tickets("Jira", "ticket", "jira", [
          { key: "site", label: "Jira site, such as acme.atlassian.net", required: true },
          { key: "project", label: "Project key, such as APP", required: false },
          { key: "jql", label: "Or a JQL query of your own", required: false },
        ]),
      }),
    ),
    own(
      "linear",
      details({
        name: "Linear",
        description: "A Linear team's issues, signed in with muster auth linear.",
        tickets: tickets("Linear", "issue", "linear", [
          { key: "team", label: "Team key, such as ENG", required: true },
        ]),
      }),
    ),
    {
      id: "notify-webhook",
      source: { folder: { path: WEBHOOK_FOLDER } },
      enabled: true,
      problem: null,
      details: webhook(),
      changed: null,
    },
    {
      id: "session-graph",
      source: {
        git: {
          url: "https://github.com/furkankly/muster-session-graph",
          git_ref: null,
          commit: "8c1f2e9a7b3d4c5e6f708192a3b4c5d6e7f80912",
        },
      },
      enabled: true,
      problem: null,
      details: details({
        name: "Session graph",
        version: "0.1.0",
        description: "A session's live flow graph beside it, drawn by zoetrope.",
        muster: "0.4.0",
        requires: [{ program: "zoe", found: true }],
        how_to_get: "brew install furkankly/tap/zoetrope",
        panels: [
          {
            id: "graph",
            title: "graph",
            on: "session",
            command: ["zoe", "{transcript}", "--follow"],
            key: "G",
          },
        ],
      }),
      changed: null,
    },
    {
      id: "youtrack",
      source: {
        git: {
          url: YOUTRACK_GIT,
          git_ref: "main",
          commit: "3f9a2c1d8e7b6a5f4c3d2e1f0a9b8c7d6e5f4a3b",
        },
      },
      enabled: true,
      problem: "Changed",
      details: youtrack("0.2.0"),
      changed: { what: "updated", by: "command_line", at: NOW - 2 * DAY },
    },
  ];
}

/** What fetching a plugin from an address or a folder finds: its id, where it is, its manifest. */
export interface Fetchable {
  id: string;
  source: PluginSource;
  details: PluginDetails;
}

export function fetchable(): Map<string, Fetchable> {
  return new Map<string, Fetchable>([
    [
      YOUTRACK_GIT,
      {
        id: "youtrack",
        source: {
          git: {
            url: YOUTRACK_GIT,
            git_ref: "main",
            commit: "9c41d07e5b2a8f6c3e1d0b9a8f7e6d5c4b3a2918",
          },
        },
        details: youtrack("0.2.1"),
      },
    ],
    [
      "https://github.com/acme/muster-standup",
      {
        id: "standup",
        source: {
          git: {
            url: "https://github.com/acme/muster-standup",
            git_ref: null,
            commit: "5e0a91c4d27b3f8e6a1c9d0b2e4f6a8c0d2e4f61",
          },
        },
        details: details({
          name: "Stand-up notes",
          version: "0.3.0",
          description: "Writes what each agent finished to a note for your stand-up.",
          requires: [{ program: "jq", found: false }],
          how_to_get: "brew install jq",
          hooks: [
            {
              id: "note",
              on: ["session.finished"],
              command: ["./note", "{project}"],
              timeout: 10,
              when: { project: ["muster"], harness: [], reason: [], away: null },
            },
          ],
        }),
      },
    ],
    [
      "/Users/you/muster/examples/plugins/tickets-from-file",
      {
        id: "tickets-from-file",
        source: { folder: { path: "/Users/you/muster/examples/plugins/tickets-from-file" } },
        details: details({
          name: "Tickets from a file",
          version: "0.1.0",
          description: "Tickets from tickets.txt beside it, or a text file of the project's.",
          muster: "0.4.0",
          requires: [
            { program: "awk", found: true },
            { program: "sed", found: true },
          ],
          how_to_get: "awk and sed come with macOS and Linux.",
          tickets: {
            name: "File",
            noun: "ticket",
            command: ["./tickets"],
            settings: [
              { key: "file", label: "File, if not tickets.txt beside it", required: false },
            ],
          },
        }),
      },
    ],
    [
      WEBHOOK_FOLDER,
      { id: "notify-webhook", source: { folder: { path: WEBHOOK_FOLDER } }, details: webhook() },
    ],
  ]);
}
