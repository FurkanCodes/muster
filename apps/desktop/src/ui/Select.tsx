// A choice of one among a few (design system: Select): a button showing the
// chosen one, ← → stepping through them in place, and on ⏎, ↓ or a click
// its list, drawn as a menu is, on Radix's popover: under the button and at
// least as wide, turned up where the window has no room below, closed by ⎋,
// a click outside or the focus leaving. Inside a dialog it is drawn in the
// dialog, which alone takes the pointer while it is open.
//
//   <Select label="Agent" options={options} value={value} onChange={setValue} />
//   <Select variant="quiet" …/>   (in a toolbar: no box until the pointer is on it)

import * as Popover from "@radix-ui/react-popover";
import { type ReactNode, useId, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { Icon } from "./Icon";
import { portalRoot } from "./portal";
import "./select.css";

export interface SelectOption {
  value: string;
  label: string;
  /** Drawn before its label, in the button too: an agent's logo. */
  mark?: ReactNode;
  note?: string;
  /** A line above it, setting it apart from the options before it. */
  apart?: boolean;
}
export function Select({
  label,
  options,
  value,
  onChange,
  disabled = false,
  size = "lg",
  variant = "field",
  onActive,
  onStep,
}: {
  label: string;
  options: readonly SelectOption[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  /** A dialog's is large; a settings row's is the default size (design system: Select). */
  size?: "md" | "lg";
  /** A form's field, boxed; or a toolbar's, quiet, as wide as its choice (its note only in its list). */
  variant?: "field" | "quiet";
  /** The option the keys moved to, before it is chosen: a sound is heard as it is chosen. */
  onActive?: (value: string) => void;
  /** ← → step it the caller's way, as far as either end or round, in place of round. */
  onStep?: (by: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState(0);
  // Where its list is drawn: the dialog it is in, or the window's root.
  const [container, setContainer] = useState<HTMLElement | undefined>(undefined);
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const chosen = options.find((o) => o.value === value) ?? options[0];
  const step = (by: number) => {
    if (onStep) {
      onStep(by);
      return;
    }
    const index = options.findIndex((o) => o.value === value);
    const next = options[(index + by + options.length) % options.length];
    if (next) onChange(next.value);
  };
  const show = (next: boolean) => {
    if (next) {
      setAt(
        Math.max(
          0,
          options.findIndex((o) => o.value === value),
        ),
      );
      setContainer(button.current?.closest("dialog") ?? portalRoot());
    }
    setOpen(next);
  };
  const choose = (option: SelectOption | undefined) => {
    if (option) onChange(option.value);
    setOpen(false);
    button.current?.focus();
  };
  return (
    <Popover.Root open={open} onOpenChange={show}>
      <div className={cn("ui-select", variant === "quiet" && "ui-select--quiet")}>
        <Popover.Anchor asChild>
          <button
            ref={button}
            type="button"
            className={cn(
              "mu-select",
              size === "lg" && "mu-select--lg",
              variant === "quiet" && "mu-select--quiet",
            )}
            aria-label={label}
            aria-haspopup="listbox"
            aria-expanded={open}
            aria-controls={open ? id : undefined}
            disabled={disabled}
            onClick={() => show(!open)}
            onKeyDown={(event) => {
              if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
                event.preventDefault();
                event.stopPropagation();
                step(event.key === "ArrowLeft" ? -1 : 1);
              }
              if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(event.key)) {
                event.preventDefault();
                event.stopPropagation();
                show(true);
              }
            }}
          >
            <span className="mu-select-value">
              {chosen?.mark ? (
                <span className="ui-select-mark" aria-hidden="true">
                  {chosen.mark}
                </span>
              ) : null}
              <span className="mu-cut">{chosen?.label}</span>
              {chosen?.note && variant === "field" ? (
                <span className="mu-quiet">{chosen.note}</span>
              ) : null}
            </span>
            <Icon name="chevron" />
          </button>
        </Popover.Anchor>
      </div>
      <Popover.Portal container={container}>
        <Popover.Content
          className="mu-select-menu ui-select-menu"
          role="listbox"
          id={id}
          aria-label={label}
          tabIndex={0}
          side="bottom"
          align="start"
          sideOffset={4}
          collisionPadding={8}
          aria-activedescendant={`${id}-${at}`}
          // The list holds the keys while open, its options never the focus.
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            (event.currentTarget as HTMLElement).focus();
          }}
          onCloseAutoFocus={(event) => event.preventDefault()}
          // Its own ⎋ closes it and gives the focus back, below.
          onEscapeKeyDown={(event) => event.preventDefault()}
          // A press on its own button is the button's: it closes it.
          onInteractOutside={(event) => {
            if (button.current?.contains(event.target as Node)) event.preventDefault();
          }}
          onKeyDown={(event) => {
            event.stopPropagation();
            if (event.key === "Escape") {
              event.preventDefault();
              setOpen(false);
              button.current?.focus();
            }
            if (["ArrowDown", "ArrowUp"].includes(event.key)) {
              event.preventDefault();
              const next =
                (at + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length;
              setAt(next);
              const option = options[next];
              if (option) onActive?.(option.value);
            }
            if (["Enter", " "].includes(event.key)) {
              event.preventDefault();
              choose(options[at]);
            }
          }}
        >
          {options.map((option, index) => [
            option.apart ? (
              // biome-ignore lint/a11y/useSemanticElements: the design system's menu separator is a hairline div, as its boards draw it.
              <div key={`apart-${option.value}`} className="mu-select-sep" role="separator" />
            ) : null,
            <div
              key={option.value}
              id={`${id}-${index}`}
              role="option"
              aria-selected={value === option.value}
              className={cn("mu-select-option", index === at && "is-active")}
              tabIndex={-1}
              onPointerMove={() => setAt(index)}
              onClick={() => choose(option)}
              onKeyDown={() => {}}
            >
              <span className="mu-option-check">{value === option.value ? "✓" : ""}</span>
              {option.mark ? (
                <span className="ui-select-mark" aria-hidden="true">
                  {option.mark}
                </span>
              ) : null}
              <span className="mu-cut">{option.label}</span>
              {option.note ? <span className="mu-quiet ui-select-note">{option.note}</span> : null}
            </div>,
          ])}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
