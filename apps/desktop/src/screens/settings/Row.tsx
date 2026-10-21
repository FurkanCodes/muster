// The parts every page of Settings is made of (design system:
// SettingsPage): the page with its head and footnote, its groups, and a
// row, which the whole of is the target of its control, selected with the
// keys or a press, and answering ⏎, space and ← → while selected.

import { type ReactNode, useEffect, useId, useLayoutEffect, useRef } from "react";
import { type RowBehaviour, useSettings } from "./context";
import { sentence } from "./words";

/** What inside a row is a control of its own, which takes its own presses. */
const CONTROL = "button, input, select, textarea, a[href], [role='switch'], [role='option']";

function inControl(target: EventTarget, row: Element): boolean {
  const control = target instanceof Element ? target.closest(CONTROL) : null;
  return control !== null && control !== row && row.contains(control);
}

export function SettingRow({
  id,
  level = "top",
  press = "selects",
  off,
  act,
  step,
  back,
  className = "mu-setting",
  children,
}: {
  /** The row's key in the list of settings. */
  id: string;
  /** A row that belongs to the one above it is set in. */
  level?: "top" | "sub";
  /** What a press on the row, outside its controls, does: a switch's whole row is its target. */
  press?: "selects" | "acts";
  /** Why the row does nothing now (its parent is off), where it does nothing. */
  off?: string;
  act?: RowBehaviour["act"];
  step?: (by: number) => void;
  back?: () => void;
  /** The design system's row: `mu-setting`, `mu-keyrow`, `mu-agentrow`. */
  className?: string;
  children: ReactNode;
}) {
  const { state, actions, meta } = useSettings();
  const selected = state.selected?.key === id;
  const element = useRef<HTMLDivElement>(null);
  const behaviour = useRef<RowBehaviour>({});
  const registry = meta.registry;
  const focusFollows = meta.focusFollows;

  useLayoutEffect(() => {
    behaviour.current = off ? {} : { act, step, back };
  });

  useEffect(() => {
    const rows = registry.current;
    rows.set(id, behaviour);
    return () => {
      if (rows.get(id) === behaviour) rows.delete(id);
    };
  }, [id, registry]);

  // Selected: in view, and the focus with it where the keys moved it from a control.
  useEffect(() => {
    const row = element.current;
    if (!selected || !row) return;
    row.scrollIntoView?.({ block: "nearest" });
    if (focusFollows.current) {
      // The row itself, so ↑↓ go on moving; a field's own input, to type in.
      focusFollows.current = false;
      (row.querySelector<HTMLElement>("input") ?? row).focus();
    }
  }, [selected, focusFollows]);

  const sub = level === "sub" ? "mu-setting--sub" : "";
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the whole row is its control's target, as the design system's settings row is; its keys come through the screen's listener.
    // biome-ignore lint/a11y/useKeyWithClickEvents: ⏎ and space reach the selected row through the screen's listener.
    <div
      ref={element}
      className={`${className} ${sub} ${selected ? "is-selected" : ""} ${off ? "app-setting--off" : ""}`}
      data-row={id}
      tabIndex={-1}
      aria-disabled={off ? true : undefined}
      title={off}
      onMouseDown={() => actions.select(id)}
      onFocus={() => {
        if (!selected) actions.select(id);
      }}
      onClick={(event) => {
        if (press === "acts" && !inControl(event.target, event.currentTarget)) {
          behaviour.current.act?.("click");
        }
      }}
    >
      {children}
    </div>
  );
}

/** Opens the Select in `row`, as ⏎ on a selected row does: the menu takes the keys. */
export function openSelect(row: string): void {
  const button = document.querySelector<HTMLButtonElement>(`[data-row="${row}"] .mu-select`);
  if (!button || button.disabled) return;
  button.focus();
  button.click();
}

/** A setting's name, a Label where it is set elsewhere than the person's file, and what it does now. */
export function SettingText({
  name,
  label,
  does,
}: {
  name: ReactNode;
  label?: string;
  does?: ReactNode;
}) {
  return (
    <span className="mu-setting-text">
      {label ? (
        <span className="mu-setting-name">
          {name}
          <span className="mu-label">{label}</span>
        </span>
      ) : (
        <span>{name}</span>
      )}
      {does ? <span className="mu-meta">{does}</span> : null}
    </span>
  );
}

export function Control({ children }: { children: ReactNode }) {
  return <span className="mu-setting-control">{children}</span>;
}

export function Group({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section className="mu-settings-group" aria-labelledby={id}>
      <h3 className="mu-settings-group-title" id={id}>
        {title}
      </h3>
      {children}
    </section>
  );
}

export function Page({
  title,
  swatch,
  lead,
  aside,
  children,
}: {
  title: string;
  /** A project's colour, beside its name. */
  swatch?: number;
  lead: ReactNode;
  /** At the head's end: a button for the whole page. */
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="mu-settings">
      <div className={`mu-settings-head ${aside ? "app-settings-head--aside" : ""}`}>
        <div className="app-settings-head-text">
          <h2 className="mu-settings-title">
            {swatch ? <span className={`mu-swatch mu-p${swatch}`} /> : null}
            {title}
          </h2>
          <span className="mu-meta">{lead}</span>
        </div>
        {aside}
      </div>
      {children}
    </div>
  );
}

/** Where the page's settings are saved. */
export function Footnote({ children }: { children: ReactNode }) {
  return <p className="mu-footnote app-settings-footnote">{children}</p>;
}

/** Muster's own settings do not read: the rest are its defaults meanwhile. */
export function Unreadable({ problem, readKey }: { problem: string; readKey: string }) {
  return (
    <div className="mu-banner app-settings-banner" role="status">
      <span className="mu-mark mu-mark--waiting" aria-hidden="true">
        ▲
      </span>
      <span className="app-settings-banner-text">
        <span className="mu-banner-lead">Muster's own settings do not read:</span>{" "}
        {sentence(problem)}. Until they do, Muster's defaults hold. Fix the file, then press{" "}
        {readKey}.
      </span>
    </div>
  );
}
