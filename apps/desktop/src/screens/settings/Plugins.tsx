// Settings › Plugins (board: SettingsPlugins, `app::plugins`): Muster's own
// plugins and the person's, each with its version, where it is from,
// whether it runs and why not, or what it brings; beside them everything
// the selected one runs. Space turns one on or off, refused with why where
// it cannot run; ⏎ on one that changed shows what it runs now to approve.

import { useShallow } from "zustand/react/shallow";
import { useNow } from "../../app/clock";
import { useDaemon } from "../../app/context";
import type { PluginInfo } from "../../ipc/generated/PluginInfo";
import { useKeyLabel, useKeymap } from "../../keys/useKeymap";
import { Banner, BannerBody, BannerLead } from "../../ui/Banner";
import { Button } from "../../ui/controls";
import { Heading, HeadingActions, HeadingLead, HeadingTitle } from "../../ui/Heading";
import { Section, SectionTitle } from "../../ui/Section";
import { Switch } from "../../ui/Switch";
import { useSettings } from "./context";
import { rowKey, rowName } from "./model";
import { PluginFacts } from "./PluginFacts";
import { brings, pluginName, pluginOrder, runs, sourceText, wantsApproval } from "./pluginWords";
import { SettingRow, Unreadable } from "./Row";
import {
  capital,
  changeWords,
  oneLine,
  problemText,
  problemWord,
  sentence,
  sourceWord,
} from "./words";

function usePlugin(id: string): PluginInfo | undefined {
  return useDaemon((daemon) => daemon.plugins.find((plugin) => plugin.id === id));
}

function PluginItem({ id }: { id: string }) {
  const { state, actions } = useSettings();
  const plugin = usePlugin(id);
  const map = useKeymap();
  const now = useNow();
  const row = rowKey("plugins", `plugin.${id}`);
  if (!plugin) return null;
  const busy = state.busy.has(row);
  const name = pluginName(plugin);
  const commandLine = plugin.changed?.by === "command_line" ? plugin.changed : null;
  const what = plugin.problem
    ? `Off: ${problemWord(plugin.problem)}`
    : commandLine
      ? changeWords(commandLine, now)
      : brings(plugin, map);
  return (
    <SettingRow
      id={row}
      className="mu-plugin"
      act={(how) => {
        if (how === "click") return;
        if (how === "enter" && wantsApproval(plugin)) actions.updatePlugin();
        else actions.togglePlugin(id);
      }}
    >
      <button
        type="button"
        className="mu-plugin-open"
        aria-current={state.selected?.key === row ? "true" : undefined}
        onClick={() => actions.select(row)}
      >
        <span className="mu-plugin-name">
          {name}
          {plugin.details ? (
            <span className="mu-plugin-version">{oneLine(plugin.details.version)}</span>
          ) : null}
          <span className="mu-label">{sourceWord(plugin.source)}</span>
        </span>
        <span className={`mu-plugin-what ${plugin.problem ? "is-off" : ""}`}>
          {busy ? "…" : what}
        </span>
      </button>
      <Switch
        on={runs(plugin)}
        label={name}
        onChange={() => actions.togglePlugin(id)}
        busy={busy}
        title={
          plugin.problem && wantsApproval(plugin) ? "Approve it again before it can run" : undefined
        }
      />
    </SettingRow>
  );
}

function AddItem() {
  const { actions } = useSettings();
  const add = useKeyLabel("add_plugin");
  return (
    <SettingRow
      id={rowKey("plugins", "add_plugin")}
      press="acts"
      className="mu-plugin"
      act={actions.addPlugin}
    >
      <button
        type="button"
        className="mu-plugin-open"
        title={`Add a plugin (${add})`}
        onClick={actions.addPlugin}
      >
        <span className="mu-plugin-name">+ Add a plugin…</span>
        <span className="mu-plugin-what">from a git repository's address or a folder of yours</span>
      </button>
    </SettingRow>
  );
}

/** What plugins are and how one is added, where none is selected (`section_text`). */
function About() {
  const add = useKeyLabel("add_plugin");
  const update = useKeyLabel("update_plugin");
  const remove = useKeyLabel("remove_plugin");
  return (
    <section className="app-plugin-details" aria-label="Plugins">
      <span className="mu-meta">
        Plugins bring panels beside a session, sources of tickets, and hooks that hear what happens:
        programs of their own, in any language, which the daemon runs apart from Muster, never
        inside it.
      </span>
      <span className="mu-meta">
        {add} adds one from a git repository's address, or links a folder of yours where it is.
        Before anything is added you see everything it would run, the programs it needs and the
        secrets it asks for, and say yes or no. On a plugin, space turns it on or off, {update}{" "}
        updates it and {remove} removes it.
      </span>
    </section>
  );
}

function Details({ id }: { id: string }) {
  const { actions } = useSettings();
  const plugin = usePlugin(id);
  const now = useNow();
  const update = useKeyLabel("update_plugin");
  const remove = useKeyLabel("remove_plugin");
  if (!plugin) return <About />;
  const name = pluginName(plugin);
  const details = plugin.details;
  const own = plugin.source === "builtin";
  const state = plugin.problem
    ? plugin.enabled
      ? "Off until you approve it again:"
      : "Off, and cannot be turned on:"
    : null;
  return (
    <section className="app-plugin-details" aria-labelledby="plugin-details-title">
      <div className="app-plugin-details-head">
        <h3 id="plugin-details-title" className="app-plugin-details-title">
          {name}
          {details ? <span className="mu-plugin-version">{oneLine(details.version)}</span> : null}
          <span className="mu-label">{sourceWord(plugin.source)}</span>
        </h3>
        {details?.description ? (
          <span className="mu-meta">{oneLine(details.description)}</span>
        ) : null}
      </div>
      {plugin.problem ? (
        <Banner>
          <BannerBody>
            <BannerLead>{state}</BannerLead> {sentence(problemText(plugin.problem))}.
          </BannerBody>
        </Banner>
      ) : (
        <span className="mu-meta">{runs(plugin) ? "On: it runs." : "Off."}</span>
      )}
      {own ? null : (
        <div className="app-plugin-actions">
          {wantsApproval(plugin) ? (
            <Button variant="primary" title="Review and approve (⏎)" onClick={actions.updatePlugin}>
              Review and approve
            </Button>
          ) : null}
          <Button variant="quiet" title={`Update (${update})`} onClick={actions.updatePlugin}>
            Update
          </Button>
          <Button variant="danger" title={`Remove (${remove})`} onClick={actions.removePlugin}>
            Remove
          </Button>
        </div>
      )}
      {details ? (
        <PluginFacts details={details} from={sourceText(plugin.source)}>
          {plugin.changed ? (
            <>
              <dt>Last change</dt>
              <dd>{capital(changeWords(plugin.changed, now))}</dd>
            </>
          ) : null}
        </PluginFacts>
      ) : (
        <span className="mu-meta">Its manifest does not read, so what it runs is not known.</span>
      )}
    </section>
  );
}

export function Plugins() {
  const { state, actions } = useSettings();
  const problem = useDaemon((daemon) => daemon.preferences?.problem ?? null);
  const ordered = useDaemon(
    useShallow((daemon) =>
      pluginOrder(daemon.plugins).map((plugin) => `${plugin.source === "builtin"}:${plugin.id}`),
    ),
  );
  const add = useKeyLabel("add_plugin");
  const readKey = useKeyLabel("read_settings");
  const own = ordered.filter((entry) => entry.startsWith("true:")).map((entry) => entry.slice(5));
  const theirs = ordered
    .filter((entry) => entry.startsWith("false:"))
    .map((entry) => entry.slice(6));
  const selected = state.selected?.section === "plugins" ? rowName(state.selected.key) : "";
  const shown = selected.startsWith("plugin.") ? selected.slice("plugin.".length) : null;
  return (
    <div className="app-plugins">
      <Heading className="app-settings-heading">
        <HeadingTitle>Plugins</HeadingTitle>
        <HeadingLead>
          Muster's own and the ones you added. Each runs only what it showed you when you approved
          it.
        </HeadingLead>
        <HeadingActions>
          <Button
            variant="quiet"
            icon="plus"
            title={`Add a plugin (${add})`}
            onClick={actions.addPlugin}
          >
            Add a plugin
          </Button>
        </HeadingActions>
      </Heading>
      {problem ? <Unreadable problem={problem} readKey={readKey} /> : null}
      <div className="app-plugins-body">
        <div className="app-plugins-list">
          <Section>
            <SectionTitle>Muster's own</SectionTitle>
            {own.map((id) => (
              <PluginItem key={id} id={id} />
            ))}
          </Section>
          <Section className="app-plugins-yours">
            <SectionTitle>Yours</SectionTitle>
            {theirs.length === 0 ? (
              <p className="mu-footnote app-settings-footnote">None yet.</p>
            ) : (
              theirs.map((id) => <PluginItem key={id} id={id} />)
            )}
            <AddItem />
          </Section>
        </div>
        {shown ? <Details id={shown} /> : <About />}
      </div>
      <p className="mu-footnote app-settings-footnote">
        Muster keeps them in its own settings, <code>[plugins.&lt;id&gt;]</code>, and marks here a
        change made with <code>muster plugin</code>.
      </p>
    </div>
  );
}
