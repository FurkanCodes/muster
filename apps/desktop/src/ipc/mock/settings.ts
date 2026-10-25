// The mock daemon's settings and plugins: the commands Settings sends,
// answered and refused as the daemon answers them (muster-daemon's
// supervisor/settings.rs, own_settings.rs, looking.rs, naming.rs,
// plugins.rs and config.rs), and what tests drive: an edit by hand, a
// project's settings that do not read, a remote that names no tracker. A
// project's settings as they stand are the tickets mock's
// (`world.ticketSettings`), which answers *only my tickets* for both
// screens; this keeps what they are made from.

import { actionNamed, isPanelName, keymapOf } from "../../keys/keymap";
import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { ErrorCode } from "../generated/ErrorCode";
import type { PluginCommand } from "../generated/PluginCommand";
import type { PluginInfo } from "../generated/PluginInfo";
import type { PluginPreview } from "../generated/PluginPreview";
import type { PluginProblem } from "../generated/PluginProblem";
import type { PreferenceChange } from "../generated/PreferenceChange";
import type { Preferences } from "../generated/Preferences";
import type { ProjectSettings } from "../generated/ProjectSettings";
import type { Reply } from "../generated/Reply";
import type { SettingChange } from "../generated/SettingChange";
import type { TicketsFrom } from "../generated/TicketsFrom";
import type { Toggle } from "../generated/Toggle";
import type { MockBridge } from "./bridge";
import { NOW } from "./now";
import { type Fetchable, fetchable } from "./plugins";

function refused(code: ErrorCode, message: string): BridgeFailure {
  return new BridgeFailure("refused", message, code);
}

/** What a project's settings are made from, beside what they say (`world.ticketSettings`). */
export interface ProjectRecord {
  /** Where its tickets are, as written in the person's settings or its muster.toml. */
  tickets: TicketsFrom | null;
  ticketsSetBy: ProjectSettings["tickets_set_by"];
  /** The tracker its git remote's host comes with; `null` where the remote names none. */
  remote: string | null;
  /** Why its settings do not read, while they do not. */
  broken: string | null;
}

/** kep-site's and kep-api's tickets: one Linear team's, as the Tickets board has them. */
const KEP: TicketsFrom = { tracker: "linear", fields: { team: "KEP" } };

const SAY =
  'choose where its tickets are on the Settings screen, or say with [tickets] tracker = "github", "gitlab", "jira", "linear" or a ticket plugin\'s id in muster.toml';

/** `PluginProblem`'s words. */
function problemWords(problem: PluginProblem): string {
  if (problem === "Changed") {
    return "its manifest changed since you approved it: update it, or link it again, to see what it runs now and approve it";
  }
  if (problem === "NotApproved") {
    return "it was never approved: install or link it to see what it runs and approve it";
  }
  if (problem === "Unknown") {
    return "it names no git repository or folder, and Muster has no plugin of its own by its id";
  }
  return "Missing" in problem
    ? `${problem.Missing.path} is gone`
    : `its manifest does not read: ${problem.Unreadable.message}`;
}

export class MockSettings {
  /** What each project's settings are made from, by its id. */
  readonly projects = new Map<string, ProjectRecord>();
  /** What fetching an address or a folder finds. */
  readonly sources: Map<string, Fetchable> = fetchable();
  /** Previews held until installed, by their id. */
  private readonly held = new Map<string, PluginPreview>();
  private previews = 0;
  /** An edit by hand to Muster's settings file, read on `ReadPreferences`. */
  private edited: Partial<Preferences> | null = null;
  /**
   * Where each agent is installed and its version, as the daemon finds them
   * (`Command::Agents`), by harness; one not here is not found. Tests change it.
   */
  readonly installed: Record<string, { path: string; version: string | null }> = {
    claude: { path: "/opt/homebrew/bin/claude", version: "2.1.303" },
    codex: { path: "/opt/homebrew/bin/codex", version: "0.162.1" },
    gemini: { path: "/Users/you/.npm-global/bin/gemini", version: "0.63.0" },
    opencode: { path: "/opt/homebrew/bin/opencode", version: "1.4.6" },
  };

  constructor(private readonly mock: MockBridge) {
    const names = new Map(mock.world.projects.map((project) => [project.name, project.id]));
    const set = (name: string, record: Partial<ProjectRecord>, prefix?: Toggle) => {
      const id = names.get(name);
      if (!id) return;
      this.projects.set(id, {
        tickets: null,
        ticketsSetBy: "default",
        remote: "github",
        broken: null,
        ...record,
      });
      const said = mock.world.ticketSettings[id];
      if (said && prefix) said.muster_prefix = prefix;
      this.tell(id);
    };
    set("muster", {});
    set("lamphaus", {});
    set("kep-site", { tickets: KEP, ticketsSetBy: "yours" }, { on: true, from: "project" });
    set("kep-api", { tickets: KEP, ticketsSetBy: "yours" });
  }

  // What tests drive.

  /** Muster's own settings, changed elsewhere (the terminal UI): every client is told at once. */
  setPreferences(change: Partial<Preferences>): void {
    this.mock.world.preferences = { ...this.mock.world.preferences, ...change };
    this.mock.publish({ preferences: this.mock.world.preferences });
  }

  /** Muster's own settings file, edited by hand: read once the person says (`ReadPreferences`). */
  editByHand(change: Partial<Preferences>): void {
    this.edited = { ...this.edited, ...change };
  }

  /** The plugins, changed from the command line: every client is told at once. */
  setPlugins(plugins: PluginInfo[]): void {
    this.mock.world.plugins = plugins;
    this.mock.publish({ plugins: [...plugins] });
  }

  /** What a project's settings are made from, changed: read as the screen next asks. */
  setProject(project: string, record: Partial<ProjectRecord>): void {
    const now = this.projects.get(project);
    if (!now) return;
    this.projects.set(project, { ...now, ...record });
    this.tell(project);
  }

  // The commands.

  /** `Command::Agents`: Muster's own agents, then the person's, each as installed or not found. */
  private agentsFound(): Reply {
    const harnesses = [
      "claude",
      "codex",
      "gemini",
      "opencode",
      "copilot",
      ...this.mock.world.preferences.agents.map((agent) => agent.name),
    ];
    return {
      Agents: harnesses.map((harness) => {
        const found = this.installed[harness];
        return { harness, path: found?.path ?? null, version: found?.version ?? null };
      }),
    };
  }

  handle(command: Exclude<Command, "Ping">): Reply | undefined {
    if (command === "ReadPreferences") return this.readPreferences();
    if (command === "Agents") return this.agentsFound();
    if (typeof command === "string") return undefined;
    if ("ChangePreference" in command) return this.changePreference(command.ChangePreference);
    if ("ProjectSettings" in command)
      return { ProjectSettings: this.settingsOf(command.ProjectSettings.project) };
    if ("ChangeSetting" in command) {
      const { project, change } = command.ChangeSetting;
      return this.changeSetting(project, change);
    }
    if ("ListModels" in command) return this.listModels(command.ListModels.agent);
    if ("RemoveProject" in command) return this.removeProject(command.RemoveProject.project);
    if ("Plugin" in command) return this.plugin(command.Plugin);
    return undefined;
  }

  private get file(): string {
    return this.mock.world.preferences.file;
  }

  private readPreferences(): Reply {
    if (this.edited) {
      this.mock.world.preferences = { ...this.mock.world.preferences, ...this.edited };
      this.edited = null;
    }
    this.mock.publish({
      preferences: this.mock.world.preferences,
      plugins: [...this.mock.world.plugins],
    });
    return "Done";
  }

  private changePreference(change: PreferenceChange): Reply {
    const now = this.mock.world.preferences;
    const next: Preferences = { ...now, keys: { ...now.keys }, telling: { ...now.telling } };
    const invalid = (message: string) => refused("bad_request", `${this.file}: ${message}`);
    if (change === "AllKeysBack") next.keys = {};
    else if ("Notch" in change) next.notch = change.Notch;
    else if ("SideBySideFrom" in change) next.side_by_side_from = change.SideBySideFrom;
    else if ("Palette" in change) next.palette = change.Palette;
    else if ("SessionLines" in change) next.session_lines = change.SessionLines;
    else if ("Panels" in change) next.panels = change.Panels;
    else if ("ListFolded" in change) next.list_folded = change.ListFolded;
    else if ("ProjectOrder" in change) next.projects = change.ProjectOrder;
    else if ("NamesByAgent" in change)
      next.naming = { ...now.naming, by_agent: change.NamesByAgent };
    else if ("NamedBy" in change) next.naming = { ...now.naming, ...change.NamedBy };
    else if ("Notifications" in change) next.notifications = change.Notifications;
    else if ("SharedBuilds" in change) next.shared_builds = change.SharedBuilds;
    else if ("AgentsStartAgents" in change)
      next.delegation = { ...next.delegation, allowed: change.AgentsStartAgents };
    else if ("Tell" in change) {
      const key = { question: "questions", finish: "finishes", error: "errors" } as const;
      next.telling[key[change.Tell.kind]] = change.Tell.tell;
    } else if ("Keys" in change) {
      const { action, keys } = change.Keys;
      if (keys !== null && !actionNamed(action) && !isPanelName(action)) {
        throw invalid(`keys.${action} is not something a key does`);
      }
      if (keys === null) delete next.keys[action];
      else next.keys[action] = keys;
      // The file must read with it, its keys checked as the core checks them.
      const problem = keymapOf(next.keys).problem;
      if (problem) throw invalid(problem);
    }
    this.mock.world.preferences = next;
    this.mock.publish({ preferences: next });
    return "Done";
  }

  private record(project: string): ProjectRecord {
    const record = this.projects.get(project);
    if (!record) throw refused("not_found", `no project prj_${project}`);
    return record;
  }

  /** What a project's settings say, as the tickets mock keeps them. */
  private said(project: string): ProjectSettings {
    const said = this.mock.world.ticketSettings[project];
    if (!said) throw refused("not_found", `no project prj_${project}`);
    return said;
  }

  private projectName(project: string): string {
    return this.mock.world.projects.find((found) => found.id === project)?.name ?? project;
  }

  /**
   * Why `project`'s tickets cannot be listed as its tracker's plugin stands,
   * or null where they can (`TicketPlugin::runnable`).
   */
  ticketsRefused(project: string): string | null {
    const record = this.projects.get(project);
    const tracker = record?.tickets?.tracker ?? record?.remote;
    const plugin = this.mock.world.plugins.find((found) => found.id === tracker);
    if (!plugin) return null;
    const why = !plugin.enabled
      ? `is turned off: turn it on on the Settings screen's Plugins section, or with \`muster plugin enable ${plugin.id}\``
      : plugin.problem
        ? `cannot run: ${problemWords(plugin.problem)}`
        : null;
    return why && `its tickets come from the ${this.trackerName(plugin.id)} plugin, which ${why}`;
  }

  /** The tracker named as the daemon names it: its plugin's name, else its id. */
  private trackerName(tracker: string): string {
    return (
      this.mock.world.plugins.find((plugin) => plugin.id === tracker)?.details?.name ?? tracker
    );
  }

  /** Where its tickets come from, in words (`Tickets::written`, `settings_of`). */
  private ticketsFrom(record: ProjectRecord): [string, boolean] {
    const written = record.tickets;
    if (written) {
      const fields = written.fields;
      let said = this.trackerName(written.tracker);
      if (fields.site) said += ` at ${fields.site}`;
      if (fields.jql) said += `, ${fields.jql}`;
      else if (fields.project) said += `, project ${fields.project}`;
      else if (fields.team) said += `, team ${fields.team}`;
      for (const [key, value] of Object.entries(fields)) {
        if (!["site", "jql", "project", "team"].includes(key)) said += `, ${key} ${value}`;
      }
      return [said, false];
    }
    if (record.remote) return [`${this.trackerName(record.remote)}, as its git remote says`, false];
    return [
      `its git remote, git.acme.internal, says nothing of where its tickets are; ${SAY}, or name its host with [pull_requests] host`,
      true,
    ];
  }

  /** Writes what `project`'s settings are made from into what they say. */
  private tell(project: string): void {
    const record = this.projects.get(project);
    const said = this.mock.world.ticketSettings[project];
    if (!record || !said) return;
    const [ticketsFrom, untold] = this.ticketsFrom(record);
    said.tickets_from = ticketsFrom;
    said.tickets_untold = untold;
    said.tickets = record.tickets ? structuredClone(record.tickets) : null;
    said.tickets_set_by = record.ticketsSetBy;
  }

  private settingsOf(project: string): ProjectSettings {
    const record = this.record(project);
    if (record.broken) throw refused("bad_request", record.broken);
    return structuredClone(this.said(project));
  }

  /** `ChangeSetting`; *only my tickets* is the tickets mock's to answer. */
  private changeSetting(project: string, change: SettingChange): Reply | undefined {
    const record = this.record(project);
    const said = this.said(project);
    const invalid = (message: string) => refused("bad_request", `${said.file}: ${message}`);
    if ("OnlyMyTickets" in change) return undefined;
    if ("MusterPrefix" in change) said.muster_prefix = { on: change.MusterPrefix, from: "yours" };
    else if (change.TicketsFrom === null) {
      record.tickets = null;
      record.ticketsSetBy = "default";
    } else {
      const from = change.TicketsFrom;
      const plugin = this.mock.world.plugins.find((found) => found.id === from.tracker);
      if (!plugin) {
        throw invalid(
          `[tickets] tracker = "${from.tracker}" names a ticket plugin, and none called ${from.tracker} is installed: add it on the Settings screen's Plugins section or with \`muster plugin install\`, or choose where its tickets are on the Settings screen`,
        );
      }
      const name = plugin.details?.name ?? plugin.id;
      if (!plugin.enabled || plugin.problem) {
        throw invalid(`its tickets come from the ${name} plugin, which is turned off`);
      }
      const settings = plugin.details?.tickets?.settings ?? [];
      const missing = settings.find((setting) => setting.required && !from.fields[setting.key]);
      if (missing) {
        throw invalid(
          `tickets.tracker is ${from.tracker}, which needs tickets.${missing.key} (${missing.label})`,
        );
      }
      record.tickets = { tracker: from.tracker, fields: { ...from.fields } };
      record.ticketsSetBy = "yours";
    }
    this.tell(project);
    return { ProjectSettings: this.settingsOf(project) };
  }

  private listModels(agent: string): Reply {
    if (agent === "claude") return { Models: ["haiku", "sonnet", "opus"] };
    if (agent === "codex") return { Models: [] };
    const added = this.mock.world.preferences.agents.find((found) => found.name === agent);
    if (added?.asks) return { Models: [] };
    throw refused("not_found", `${agent} cannot be asked to name chats`);
  }

  private removeProject(project: string): Reply {
    const world = this.mock.world;
    const name = this.projectName(project);
    if (!world.projects.some((found) => found.id === project)) {
      throw refused("not_found", `no project prj_${project}`);
    }
    const workspaces = world.workspaces.filter((workspace) => workspace.project === project);
    const worktrees = workspaces.filter((workspace) => workspace.kind.kind === "worktree").length;
    if (worktrees > 0) {
      throw refused("conflict", `${name} has ${worktrees} worktree(s); remove them first`);
    }
    const ids = new Set(workspaces.map((workspace) => workspace.id));
    const sessions = world.sessions.filter((session) => ids.has(session.workspace));
    const running = sessions.filter((session) => session.status.state === "running").length;
    if (running > 0)
      throw refused("conflict", `stop ${name}'s ${running} running session(s) first`);
    world.projects = world.projects.filter((found) => found.id !== project);
    world.workspaces = world.workspaces.filter((workspace) => !ids.has(workspace.id));
    world.sessions = world.sessions.filter((session) => !ids.has(session.workspace));
    this.projects.delete(project);
    delete world.ticketSettings[project];
    delete world.tickets[project];
    this.mock.publish({
      gone: {
        projects: [project],
        workspaces: [...ids],
        sessions: sessions.map((session) => session.id),
      },
    });
    return "Done";
  }

  private found(id: string): PluginInfo {
    const plugin = this.mock.world.plugins.find((candidate) => candidate.id === id);
    if (!plugin) throw refused("not_found", `there is no plugin called ${id}`);
    return plugin;
  }

  private tellPlugins(plugins: PluginInfo[]): void {
    this.mock.world.plugins = plugins;
    this.mock.publish({ plugins: [...plugins] });
  }

  /** Holds what `found` would install, link or update, as `Registry::hold` does. */
  private hold(found: Fetchable, updateOf: string | null): Reply {
    const current = this.mock.world.plugins.find((plugin) => plugin.id === found.id) ?? null;
    const folder = typeof found.source === "object" && "folder" in found.source;
    if (updateOf !== null && updateOf !== found.id) {
      throw refused(
        "conflict",
        `its manifest calls it ${found.id} now, not ${updateOf}; remove ${updateOf} and install it afresh`,
      );
    }
    if (current?.source === "builtin") {
      throw refused(
        "conflict",
        `${found.id} is one of Muster's own: it can be turned off, not replaced or removed`,
      );
    }
    if (updateOf === null && !folder && current) {
      throw refused(
        "conflict",
        `a plugin called ${found.id} is installed already: \`muster plugin update ${found.id}\` updates it, or remove it first`,
      );
    }
    this.previews += 1;
    const preview: PluginPreview = {
      preview: `preview-${this.previews}`,
      kind: folder ? "link" : updateOf !== null ? "update" : "install",
      id: found.id,
      source: found.source,
      details: found.details,
      current,
    };
    this.held.set(preview.preview, preview);
    return { PluginPreview: preview };
  }

  private plugin(command: PluginCommand): Reply {
    const world = this.mock.world;
    if (command === "List") {
      return { Plugins: { plugins: world.plugins, now: NOW, problem: world.preferences.problem } };
    }
    if ("Preview" in command) {
      const origin = command.Preview.origin;
      if ("Folder" in origin) {
        const found = this.sources.get(origin.Folder.path);
        if (!origin.Folder.path.startsWith("/"))
          throw refused("bad_request", `${origin.Folder.path} is not a folder`);
        if (!found) {
          throw refused(
            "bad_request",
            `${origin.Folder.path} has no muster-plugin.toml, which says what a plugin is and runs`,
          );
        }
        return this.hold(found, null);
      }
      const found = this.sources.get(origin.Git.url);
      if (!found) {
        throw refused(
          "bad_request",
          `could not fetch the plugin: git: repository '${origin.Git.url}' not found`,
        );
      }
      return this.hold(found, null);
    }
    if ("Update" in command) {
      const plugin = this.found(command.Update.id);
      const source = plugin.source;
      if (source === "builtin") {
        throw refused(
          "conflict",
          `${plugin.id} is one of Muster's own: it can be turned off, not replaced or removed`,
        );
      }
      const where = "git" in source ? source.git.url : source.folder.path;
      const found = this.sources.get(where);
      if (!found)
        throw refused(
          "bad_request",
          `could not fetch the plugin: git: repository '${where}' not found`,
        );
      return this.hold(found, plugin.id);
    }
    if ("Install" in command) {
      const preview = this.held.get(command.Install.preview);
      if (!preview) {
        throw refused(
          "not_found",
          "that preview is no longer held: it was installed, let go, or kept longer than ten minutes; preview it again",
        );
      }
      this.held.delete(command.Install.preview);
      const what =
        preview.kind === "install" ? "installed" : preview.kind === "update" ? "updated" : "linked";
      const plugin: PluginInfo = {
        id: preview.id,
        source: preview.source,
        enabled: preview.current ? preview.current.enabled : true,
        problem: null,
        details: preview.details,
        changed: { what, by: command.Install.by, at: NOW },
      };
      const others = world.plugins.filter((found) => found.id !== preview.id);
      this.tellPlugins(
        preview.current
          ? world.plugins.map((found) => (found.id === preview.id ? plugin : found))
          : [...others, plugin],
      );
      return "Done";
    }
    if ("SetEnabled" in command) {
      const { id, on, by } = command.SetEnabled;
      const plugin = this.found(id);
      if (on && plugin.problem) {
        throw refused("conflict", `${id} cannot be turned on: ${problemWords(plugin.problem)}`);
      }
      const changed = { what: on ? ("turned_on" as const) : ("turned_off" as const), by, at: NOW };
      this.tellPlugins(
        world.plugins.map((found) =>
          found.id === id ? { ...found, enabled: on, changed } : found,
        ),
      );
      return "Done";
    }
    if ("Remove" in command) {
      const plugin = this.found(command.Remove.id);
      if (plugin.source === "builtin") {
        throw refused(
          "conflict",
          `${plugin.id} is one of Muster's own: it can be turned off, not replaced or removed`,
        );
      }
      this.tellPlugins(world.plugins.filter((found) => found.id !== plugin.id));
      return "Done";
    }
    throw refused("bad_request", "the mock daemon does not do that plugin command yet");
  }
}
