// A choice of one among a few (design system: Select), on Base UI's
// select: a button showing the chosen one, ← → stepping through them in
// place, and on ⏎, ↓ or a click its list, drawn as a menu is: under the
// button and at least as wide, turned up where the window has no room
// below, closed by ⎋ or a click outside. Inside a dialog it is drawn in
// the dialog, which alone takes the pointer while it is open.
//
//   <Select label="Agent" options={options} value={value} onChange={setValue} />
//   <Select variant="quiet" …/>   (in a toolbar: no box until the pointer is on it)

import { Select as Base } from "@base-ui/react/select";
import { Fragment, type ReactNode, useRef, useState } from "react";
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
  // Where its list is drawn: the dialog it is in, or the window's root.
  const [container, setContainer] = useState<HTMLElement | undefined>(undefined);
  const button = useRef<HTMLButtonElement>(null);
  // Whether the keys moved last, not the pointer: only they are said (onActive).
  const byKeys = useRef(false);
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
  return (
    <Base.Root
      value={value}
      onValueChange={(next) => {
        if (typeof next === "string") onChange(next);
      }}
      open={open}
      onOpenChange={(next) => {
        if (next) setContainer(button.current?.closest("dialog") ?? portalRoot());
        setOpen(next);
      }}
      disabled={disabled}
      modal={false}
    >
      <div className={cn("ui-select", variant === "quiet" && "ui-select--quiet")}>
        <Base.Trigger
          ref={button}
          className={cn(
            "mu-select",
            size === "lg" && "mu-select--lg",
            variant === "quiet" && "mu-select--quiet",
          )}
          aria-label={label}
          onKeyDown={(event) => {
            if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
              event.preventDefault();
              event.stopPropagation();
              step(event.key === "ArrowLeft" ? -1 : 1);
            }
            if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(event.key)) {
              // Its own: the screen behind does not take them too.
              event.stopPropagation();
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
        </Base.Trigger>
      </div>
      <Base.Portal container={container}>
        <Base.Positioner
          className="ui-select-layer"
          alignItemWithTrigger={false}
          side="bottom"
          align="start"
          sideOffset={4}
          collisionPadding={8}
        >
          <Base.Popup
            className="mu-select-menu ui-select-menu"
            onKeyDownCapture={() => {
              byKeys.current = true;
            }}
            // The screen behind hears none of its keys, nor the click Base
            // UI makes of ⏎ on an option (a settings row would take it as its own).
            onKeyDown={(event) => event.stopPropagation()}
            onClick={(event) => event.stopPropagation()}
            onPointerMove={() => {
              byKeys.current = false;
            }}
          >
            <Base.List aria-label={label}>
              {options.map((option) => (
                <Fragment key={option.value}>
                  {option.apart ? <Base.Separator className="mu-select-sep" /> : null}
                  <Base.Item
                    value={option.value}
                    label={option.label}
                    className="mu-select-option"
                    onFocus={() => {
                      if (byKeys.current) onActive?.(option.value);
                    }}
                  >
                    <span className="mu-option-check">{value === option.value ? "✓" : ""}</span>
                    {option.mark ? (
                      <span className="ui-select-mark" aria-hidden="true">
                        {option.mark}
                      </span>
                    ) : null}
                    <Base.ItemText className="mu-cut" render={<span />}>
                      {option.label}
                    </Base.ItemText>
                    {option.note ? (
                      <span className="mu-quiet ui-select-note">{option.note}</span>
                    ) : null}
                  </Base.Item>
                </Fragment>
              ))}
            </Base.List>
          </Base.Popup>
        </Base.Positioner>
      </Base.Portal>
    </Base.Root>
  );
}
