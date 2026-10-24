// A project's page of Settings (board: SettingsProject): where its tickets
// come from, with what that tracker asks for; only my tickets; branches
// with muster/; and taking it out of Muster. Its settings are read from the
// daemon (`ProjectSettings`) as the screen is shown and on `r`, and a change
// goes into the person's own settings for it, never its repository.

import { useRef, useState } from "react";
import { useActions, useBridge, useDaemon } from "../../app/context";
import type { ProjectId } from "../../ipc/generated/ProjectId";
import type { ProjectSettings } from "../../ipc/generated/ProjectSettings";
import type { SettingSource } from "../../ipc/generated/SettingSource";
import { useKeyLabel } from "../../keys/useKeymap";
import { errorText, oneLine } from "../../lib/text";
import { projectColour } from "../../state/selectors";
import { Banner, BannerBody, BannerLead } from "../../ui/Banner";
import { Button } from "../../ui/controls";
import { Dialog, DialogText } from "../../ui/Dialog";
import { Loading } from "../../ui/Loading";
import { Select, type SelectOption } from "../../ui/Select";
import { Switch } from "../../ui/Switch";
import { Input } from "../../ui/TextField";
import { type Draft, useSettings } from "./context";
import { projectSection, rowKey, type Section } from "./model";
import { Control, Footnote, Group, openSelect, Page, SettingRow, SettingText } from "./Row";
import {
  chosenTracker,
  type Field,
  fieldsFor,
  missingField,
  offeredTrackers,
  ticketsFrom,
} from "./trackers";
import { listed, sentence } from "./words";

/** Where a setting is set, where it is not the person's own file. */
function setIn(from: SettingSource): string | undefined {
  return from === "project" ? "from muster.toml" : undefined;
}

/** The Select's value for following the git remote. */
const REMOTE = "";

/** The trackers whose tickets are a repository's own, never shared between projects. */
const OWN_ISSUES = new Set(["github", "gitlab"]);

function sameTickets(a: ProjectSettings, b: ProjectSettings): boolean {
  const x = a.tickets;
  const y = b.tickets;
  if (!x || !y || x.tracker !== y.tracker || OWN_ISSUES.has(x.tracker)) return false;
  const keys = new Set([...Object.keys(x.fields), ...Object.keys(y.fields)]);
  return [...keys].every((key) => x.fields[key] === y.fields[key]);
}

function worktreesWord(count: number): string {
  return count === 1 ? "1 worktree" : `${count} worktrees`;
}

function FieldRow({
  section,
  field,
  tracker,
  onType,
  onKeep,
  onGiveUp,
}: {
  section: Section;
  field: Field;
  tracker: string;
  onType: (text: string) => void;
  onKeep: () => void;
  onGiveUp: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const id = `${section}-${field.setting.key}`;
  return (
    <SettingRow
      id={rowKey(section, `field.${field.setting.key}`)}
      press="acts"
      level="sub"
      act={() => input.current?.focus()}
    >
      <SettingText
        htmlFor={id}
        name={oneLine(field.setting.label)}
        does={field.setting.required ? `${tracker} asks for it.` : "If you like."}
      />
      <Control>
        <Input
          ref={input}
          id={id}
          className="app-settings-field"
          value={field.text}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => onType(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              onKeep();
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              onGiveUp();
              input.current?.blur();
            }
          }}
        />
      </Control>
    </SettingRow>
  );
}

function RemoveProject({ project, name }: { project: ProjectId; name: string }) {
  const bridge = useBridge();
  const ui = useActions();
  const [asking, setAsking] = useState(false);
  const worktrees = useDaemon(
    (daemon) =>
      Object.values(daemon.workspaces).filter(
        (workspace) => workspace.project === project && workspace.kind.kind === "worktree",
      ).length,
  );
  const blocked =
    worktrees > 0
      ? `It has ${worktreesWord(worktrees)}. Remove ${worktrees === 1 ? "it" : "them"} first.`
      : undefined;
  const ask = () => {
    if (blocked) ui.tell(blocked, true);
    else setAsking(true);
  };
  const remove = () => {
    setAsking(false);
    bridge.command({ RemoveProject: { project } }).then(
      () => ui.tell("Muster no longer tracks the project; its folder is untouched."),
      (error: unknown) => ui.tell(errorText(error), true),
    );
  };
  return (
    <Group title="This project">
      <SettingRow id={rowKey(projectSection(project), "remove")} act={ask}>
        <SettingText
          name={`Remove ${name} from Muster`}
          does="Forgets it and what Muster knows of it. The folder and its repository stay as they are."
        />
        <Control>
          <Button variant="danger" disabled={Boolean(blocked)} title={blocked} onClick={ask}>
            Remove…
          </Button>
        </Control>
      </SettingRow>
      {blocked ? (
        <Footnote tone="warn">
          It has {worktreesWord(worktrees)}, so it cannot be removed yet. Remove{" "}
          {worktrees === 1 ? "the worktree" : "them"} first.
        </Footnote>
      ) : null}
      {asking ? (
        <Dialog
          title={`Remove ${name} from Muster?`}
          onClose={() => setAsking(false)}
          onAccept={remove}
          actions={
            <>
              <Button
                variant="secondary"
                size="lg"
                title="Keep it (⎋)"
                data-dialog-initial
                onClick={() => setAsking(false)}
              >
                Keep it
              </Button>
              <Button variant="danger" size="lg" title="Remove it (⌘⏎)" onClick={remove}>
                Remove {name}
              </Button>
            </>
          }
        >
          <DialogText>
            Muster forgets it and what it knows of it. The folder and its repository stay as they
            are.
          </DialogText>
        </Dialog>
      ) : null}
    </Group>
  );
}

function Loaded({
  project,
  name,
  settings,
}: {
  project: ProjectId;
  name: string;
  settings: ProjectSettings;
}) {
  const { state, actions, meta } = useSettings();
  const ui = useActions();
  const plugins = useDaemon((daemon) => daemon.plugins);
  const section = projectSection(project);
  const row = (key: string) => rowKey(section, key);
  const offered = offeredTrackers(plugins);
  const draft = state.drafts.get(project);
  const tracker = chosenTracker(state.projects.get(project), draft);
  const fields = fieldsFor(offered, state.projects, project, tracker, draft);
  const trackerName = (id: string | null) =>
    id === null ? "Its git remote" : (offered.find((one) => one.id === id)?.name ?? id);

  const save = (chosen: string | null, kept: readonly Field[]) =>
    actions.setting(
      project,
      row("tickets_from"),
      { TicketsFrom: ticketsFrom(chosen, kept) },
      (now) => `${name}'s tickets now come from ${now.tickets_from}.`,
    );
  /** Keeps the choice, or says what it still needs and puts the cursor there. */
  const keep = (chosen: string | null, kept: readonly Field[], next: Draft) => {
    const missing = missingField(kept);
    if (!missing) {
      save(chosen, kept);
      return;
    }
    actions.setDraft(project, next);
    ui.tell(`${trackerName(chosen)} needs its ${oneLine(missing.setting.label)}.`, true);
    meta.focusFollows.current = true;
    actions.select(row(`field.${missing.setting.key}`));
  };
  const typedNow = (): Draft["typed"] =>
    tracker === null
      ? (draft?.typed ?? {})
      : {
          ...draft?.typed,
          [tracker]: Object.fromEntries(fields.map((field) => [field.setting.key, field.text])),
        };
  const chooseTracker = (chosen: string | null) => {
    const next: Draft = { tracker: chosen, typed: typedNow() };
    keep(chosen, fieldsFor(offered, state.projects, project, chosen, next), next);
  };
  const stepTracker = (by: number) => {
    const choices = [null, ...offered.map((one) => one.id)];
    const at = choices.indexOf(tracker);
    const next = choices[Math.min(choices.length - 1, Math.max(0, at + by))];
    if (next !== undefined && next !== tracker) chooseTracker(next);
  };
  const type = (key: string, text: string) => {
    if (tracker === null) return;
    const typed = typedNow();
    actions.setDraft(project, {
      tracker,
      typed: { ...typed, [tracker]: { ...typed[tracker], [key]: text } },
    });
  };

  const untold = settings.tickets_untold && tracker === null;
  const options: SelectOption[] = [
    { value: REMOTE, label: untold ? "Choose…" : "Its git remote" },
    ...offered.map((one) => ({ value: one.id, label: oneLine(one.name) })),
  ];
  if (tracker !== null && !offered.some((one) => one.id === tracker)) {
    options.push({ value: tracker, label: oneLine(tracker), note: "not on" });
  }
  const toggleMine = () =>
    actions.setting(
      project,
      row("only_mine"),
      { OnlyMyTickets: !settings.only_mine.on },
      (now) =>
        `Tickets for ${name} now lists ${now.only_mine.on ? "only the tickets assigned to you" : "every open ticket"}.`,
    );
  const prefix = settings.muster_prefix;
  const togglePrefix = () =>
    actions.setting(project, row("muster_prefix"), { MusterPrefix: !prefix?.on }, (now) =>
      now.muster_prefix?.on
        ? `New branches in ${name} start with muster/.`
        : `New branches in ${name} go without muster/: issue-42-fix-the-login.`,
    );

  return (
    <>
      <Group title="Tickets">
        <SettingRow
          id={row("tickets_from")}
          act={() => openSelect(row("tickets_from"))}
          step={stepTracker}
        >
          <SettingText
            name="Tickets from"
            label={setIn(settings.tickets_set_by)}
            does={
              <span className={untold ? "app-settings-untold" : undefined}>
                {oneLine(settings.tickets_from)}
              </span>
            }
          />
          <Control>
            <span className="app-settings-select--wide">
              <Select
                label="Tickets from"
                size="md"
                value={tracker ?? REMOTE}
                options={options}
                onChange={(value) => chooseTracker(value === REMOTE ? null : value)}
                onStep={stepTracker}
                disabled={state.busy.has(row("tickets_from"))}
              />
            </span>
          </Control>
        </SettingRow>
        {tracker !== null
          ? fields.map((field) => (
              <FieldRow
                key={field.setting.key}
                section={section}
                field={field}
                tracker={oneLine(trackerName(tracker))}
                onType={(text) => type(field.setting.key, text)}
                onKeep={() => keep(tracker, fields, { tracker, typed: typedNow() })}
                onGiveUp={() => actions.setDraft(project, null)}
              />
            ))
          : null}
        <SettingRow id={row("only_mine")} press="acts" act={toggleMine}>
          <SettingText
            name="Only my tickets"
            label={setIn(settings.only_mine.from)}
            does={
              settings.only_mine.on
                ? "Lists only the open tickets assigned to you. Off, every open ticket."
                : "Lists every open ticket. On, only those assigned to you."
            }
          />
          <Control>
            <Switch
              on={settings.only_mine.on}
              label="Only my tickets"
              onChange={toggleMine}
              busy={state.busy.has(row("only_mine"))}
            />
          </Control>
        </SettingRow>
      </Group>

      {prefix ? (
        <Group title="Branches">
          <SettingRow id={row("muster_prefix")} press="acts" act={togglePrefix}>
            <SettingText
              name="Branches with muster/"
              label={setIn(prefix.from)}
              does={
                prefix.on ? (
                  <>
                    New branches read <code>muster/issue-42-fix-the-login</code>. Off,{" "}
                    <code>issue-42-fix-the-login</code>.
                  </>
                ) : (
                  <>
                    New branches read <code>issue-42-fix-the-login</code>. On,{" "}
                    <code>muster/issue-42-fix-the-login</code>.
                  </>
                )
              }
            />
            <Control>
              <Switch
                on={prefix.on}
                label="Branches with muster/"
                onChange={togglePrefix}
                busy={state.busy.has(row("muster_prefix"))}
              />
            </Control>
          </SettingRow>
        </Group>
      ) : null}
    </>
  );
}

export function Project({ project }: { project: ProjectId }) {
  const { state } = useSettings();
  const name = useDaemon((daemon) => oneLine(daemon.projects[project]?.name ?? ""));
  const root = useDaemon((daemon) => daemon.projects[project]?.root ?? "");
  const colour = useDaemon((daemon) => projectColour(daemon, project));
  const names = useDaemon((daemon) => daemon.projects);
  const readKey = useKeyLabel("read_settings");
  const read = state.projects.get(project);
  const settings = read?.state === "loaded" ? read.settings : null;
  const sharing = settings
    ? [...state.projects]
        .filter(
          ([other, otherRead]) =>
            other !== project &&
            otherRead.state === "loaded" &&
            sameTickets(settings, otherRead.settings),
        )
        .map(([other]) => oneLine(names[other]?.name ?? ""))
    : [];

  return (
    <Page
      title={name}
      swatch={colour}
      lead={
        <>
          <span className="mu-mono">{oneLine(root)}</span>
          {sharing.length > 0
            ? `. It shares its tickets with ${listed(sharing)}, whose tickets come from the same place.`
            : null}
        </>
      }
    >
      {read?.state === "failed" ? (
        <Banner className="app-settings-banner">
          <BannerBody>
            <BannerLead>Its settings cannot be read:</BannerLead> {sentence(read.error)}. Fix the
            file it names, then press {readKey}.
          </BannerBody>
        </Banner>
      ) : null}
      {settings ? (
        <Loaded project={project} name={name} settings={settings} />
      ) : read?.state === "failed" ? null : (
        <Loading text="Loading settings" />
      )}
      <RemoveProject project={project} name={name} />
      <Footnote>
        Saved in your settings for this project
        {settings ? (
          <>
            , in <code>{settings.file}</code>
          </>
        ) : null}
        . Never in its repository. The repository's <code>muster.toml</code> sets what you have not.
      </Footnote>
    </Page>
  );
}
