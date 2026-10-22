// The command palette (⌘K, design system: CommandPalette): type to go to a
// session, a ticket or a setting, or to do an action on what is selected;
// ⏎ does the one lit. In its other mode (`h`) it lists every key of the
// screen in view, as the TUI's help does: typing narrows them, ⏎ or ⎋
// closes it.

import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { useActions, useApp, useUi } from "../app/context";
import { statusKind } from "../lib/status";
import { Hint, Kbd, Spacer } from "../ui/controls";
import { Icon } from "../ui/Icon";
import { Loading } from "../ui/Loading";
import { AgentLogo, StatusMark } from "../ui/status";
import { catalogue, type PaletteItem, type Ticket } from "./catalogue";
import { helpFor } from "./help";
import { labelOf } from "./keymap";
import { found } from "./match";
import "./flows.css";

const PAGE = 10;

/** The palette's own modal: the window behind is inert while it is up. */
function useModal(onClose: () => void) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    const previous = document.activeElement;
    dialog?.showModal();
    dialog?.querySelector<HTMLInputElement>("input")?.focus();
    return () => {
      dialog?.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  return {
    ref,
    onCancel: (event: { preventDefault(): void }) => {
      event.preventDefault();
      onClose();
    },
  };
}

export default function Palette() {
  const mode = useUi((state) => state.paletteMode);
  return mode === "commands" ? <Commands /> : <EveryKey changes={mode === "changes"} />;
}

/** The tickets of the projects that are repositories, asked for once the person types. */
function useTickets(asked: boolean): { tickets: Ticket[]; asking: boolean } {
  const app = useApp();
  const [tickets, setTickets] = useState<Ticket[] | null>(null);
  useEffect(() => {
    if (!asked) return;
    let current = true;
    const state = app.daemon.store.getState();
    const projects = Object.values(state.projects).filter((project) =>
      Object.values(state.workspaces).some(
        (workspace) =>
          workspace.project === project.id &&
          workspace.kind.kind === "checkout" &&
          workspace.kind.branch !== "",
      ),
    );
    // Asked of the daemon once per palette; a project whose tickets it cannot
    // reach shows none here, and says why on the Tickets screen.
    void Promise.allSettled(
      projects.map(async (project) => {
        const reply = await app.bridge.command({ ListIssues: { project: project.id } });
        return typeof reply === "object" && "Issues" in reply
          ? reply.Issues.issues.map((issue) => ({
              project: project.id,
              projectName: project.name,
              issue,
            }))
          : [];
      }),
    ).then((results) => {
      if (current)
        setTickets(
          results.flatMap((result) => (result.status === "fulfilled" ? result.value : [])),
        );
    });
    return () => {
      current = false;
    };
  }, [app, asked]);
  return { tickets: tickets ?? [], asking: asked && tickets === null };
}

function Commands() {
  const app = useApp();
  const actions = useActions();
  const [typed, setTyped] = useState("");
  const [at, setAt] = useState(0);
  const id = useId();
  const close = () => actions.openPalette(false);
  const modal = useModal(close);
  const { tickets, asking } = useTickets(typed.trim() !== "");
  const items = catalogue(app, typed, tickets);
  const lit = Math.min(at, items.length - 1);
  const chosen = items[lit];
  const help = labelOf(app.daemon.store.getState().preferences, "help");

  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    close();
    item.run();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const by = { ArrowDown: 1, ArrowUp: -1, PageDown: PAGE, PageUp: -PAGE }[event.key];
    if (by !== undefined) {
      event.preventDefault();
      setAt(Math.max(0, Math.min(items.length - 1, lit + by)));
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(chosen);
    }
  };
  useEffect(() => {
    if (chosen) document.getElementById(`${id}-${chosen.id}`)?.scrollIntoView({ block: "nearest" });
  }, [id, chosen]);

  return (
    <dialog
      ref={modal.ref}
      className="mu-palette ui-palette"
      aria-label="Command palette"
      onCancel={modal.onCancel}
    >
      <div className="mu-palette-field">
        <Icon name="search" size={16} />
        <input
          className="mu-palette-input"
          role="combobox"
          aria-label="Go to or do"
          aria-expanded="true"
          aria-controls={`${id}-list`}
          aria-autocomplete="list"
          aria-activedescendant={chosen ? `${id}-${chosen.id}` : undefined}
          placeholder="Go to a session, ticket or setting, or do something"
          autoComplete="off"
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
            setAt(0);
          }}
          onKeyDown={onKeyDown}
        />
      </div>
      <div className="mu-palette-list" role="listbox" id={`${id}-list`} aria-label="Results">
        {items.map((item, index) => (
          <Item
            key={item.id}
            id={`${id}-${item.id}`}
            item={item}
            heading={items[index - 1]?.group !== item.group ? item.group : null}
            typed={typed}
            lit={index === lit}
            onPoint={() => setAt(index)}
            onRun={() => run(item)}
          />
        ))}
        {items.length === 0 && !asking ? (
          <p className="mu-meta ui-palette-none">
            No action, session, ticket or setting holds “{typed.trim()}”.
          </p>
        ) : null}
        {asking ? <Loading label="Asking for the tickets" layout="rule" /> : null}
      </div>
      <div className="mu-palette-foot">
        <Hint keys="↑↓" does="move" />
        <Hint keys="⏎" does="do it" />
        <Hint keys="⎋" does="close" />
        <Spacer />
        {help !== "none" ? <Hint keys={help.split(" ")[0] ?? help} does="every key" /> : null}
      </div>
    </dialog>
  );
}

function Lit({ text, typed }: { text: string; typed: string }) {
  const wanted = typed.trim();
  const at = wanted ? found(text, wanted) : null;
  if (at === null) return text;
  return (
    <>
      {text.slice(0, at)}
      <mark>{text.slice(at, at + wanted.length)}</mark>
      {text.slice(at + wanted.length)}
    </>
  );
}

function Item({
  id,
  item,
  heading,
  typed,
  lit,
  onPoint,
  onRun,
}: {
  id: string;
  item: PaletteItem;
  heading: string | null;
  typed: string;
  lit: boolean;
  onPoint: () => void;
  onRun: () => void;
}) {
  return (
    <>
      {heading ? (
        <div className="mu-palette-group" role="presentation">
          {heading}
        </div>
      ) : null}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the field keeps the keys and lights an option by aria-activedescendant; ↑↓ and ⏎ there do what a click does here. */}
      <div
        id={id}
        role="option"
        aria-selected={lit}
        tabIndex={-1}
        className={`mu-palette-item ${lit ? "is-active" : ""}`}
        onPointerMove={lit ? undefined : onPoint}
        onClick={onRun}
      >
        {item.lead.kind === "session" ? (
          <>
            <StatusMark kind={statusKind(item.lead.session)} />
            <AgentLogo harness={item.lead.session.harness} size="sm" />
          </>
        ) : item.lead.kind === "ticket" ? (
          <span className="mu-mono mu-quiet">{item.lead.key}</span>
        ) : null}
        <span className="mu-cut">
          <Lit text={item.label} typed={typed} />
        </span>
        {item.meta ? <span className="mu-meta ui-palette-meta">{item.meta}</span> : null}
        {item.keys && item.keys !== "none" ? <Kbd>{item.keys}</Kbd> : null}
      </div>
    </>
  );
}

/** Every key of the screen in view, or with `changes` of the changes beside it. */
function EveryKey({ changes }: { changes: boolean }) {
  const app = useApp();
  const actions = useActions();
  const screen = useUi((state) => state.screen);
  const [typed, setTyped] = useState("");
  const list = useRef<HTMLDivElement>(null);
  const close = () => actions.openPalette(false);
  const modal = useModal(close);
  const sections = helpFor(
    changes ? "changes" : screen,
    app.daemon.store.getState().preferences,
    typed,
  );
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const by = { ArrowDown: 32, ArrowUp: -32, PageDown: 320, PageUp: -320 }[event.key];
    if (by !== undefined) {
      event.preventDefault();
      list.current?.scrollBy({ top: by });
    } else if (event.key === "Enter") {
      event.preventDefault();
      close();
    }
  };
  return (
    <dialog
      ref={modal.ref}
      className="mu-palette ui-palette"
      aria-label="Every key"
      onCancel={modal.onCancel}
    >
      <div className="mu-palette-field">
        <Icon name="search" size={16} />
        <input
          className="mu-palette-input"
          aria-label="Find a key"
          placeholder="Find a key, or what it does"
          autoComplete="off"
          spellCheck={false}
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={onKeyDown}
        />
      </div>
      <div className="mu-palette-list" ref={list}>
        {sections.map((section) => (
          <section key={section.heading} aria-label={section.heading}>
            <h3 className="mu-palette-group ui-palette-heading">{section.heading}</h3>
            <ul className="ui-palette-keys">
              {section.entries.map((entry) => (
                <li key={`${entry.keys} ${entry.what}`} className="mu-palette-item">
                  <span className="mu-cut">{entry.what}</span>
                  <Kbd>{entry.keys}</Kbd>
                </li>
              ))}
            </ul>
          </section>
        ))}
        {sections.length === 0 ? (
          <p className="mu-meta ui-palette-none">No key or what it does holds “{typed.trim()}”.</p>
        ) : null}
      </div>
      <div className="mu-palette-foot">
        <Hint keys="↑↓" does="scroll" />
        <Hint keys={["⏎", "⎋"]} does="close" />
      </div>
    </dialog>
  );
}
