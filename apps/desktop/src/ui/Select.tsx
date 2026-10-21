import { useId, useRef, useState } from "react";
import { Icon } from "./Icon";
import "./select.css";

export interface SelectOption {
  value: string;
  label: string;
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
  /** The option the keys moved to, before it is chosen: a sound is heard as it is chosen. */
  onActive?: (value: string) => void;
  /** ← → step it the caller's way, as far as either end or round, in place of round. */
  onStep?: (by: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState(0);
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
  return (
    <div className="ui-select">
      <button
        ref={button}
        type="button"
        className={`mu-select ${size === "lg" ? "mu-select--lg" : ""}`}
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        disabled={disabled}
        onClick={() => {
          setAt(
            Math.max(
              0,
              options.findIndex((o) => o.value === value),
            ),
          );
          setOpen(!open);
        }}
        onKeyDown={(event) => {
          if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
            event.preventDefault();
            event.stopPropagation();
            step(event.key === "ArrowLeft" ? -1 : 1);
          }
          if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(event.key)) {
            event.preventDefault();
            event.stopPropagation();
            setAt(
              Math.max(
                0,
                options.findIndex((o) => o.value === value),
              ),
            );
            setOpen(true);
          }
        }}
      >
        <span className="mu-select-value">
          <span>{chosen?.label}</span>
          {chosen?.note ? <span className="mu-quiet">{chosen.note}</span> : null}
        </span>
        <Icon name="chevron" />
      </button>
      {open ? (
        <div
          className="mu-menu ui-select-menu"
          role="listbox"
          id={id}
          aria-label={label}
          tabIndex={0}
          ref={(node) => node?.focus()}
          aria-activedescendant={`${id}-${at}`}
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
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
              const option = options[at];
              if (option) onChange(option.value);
              setOpen(false);
              button.current?.focus();
            }
          }}
        >
          {options.map((option, index) => [
            option.apart ? (
              // biome-ignore lint/a11y/useSemanticElements: the design system's menu separator is a hairline div, as its boards draw it.
              <div key={`apart-${option.value}`} className="mu-menu-sep" role="separator" />
            ) : null,
            <div
              key={option.value}
              id={`${id}-${index}`}
              role="option"
              aria-selected={value === option.value}
              className={`mu-menu-item ${index === at ? "is-active" : ""}`}
              tabIndex={-1}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
                button.current?.focus();
              }}
              onKeyDown={() => {}}
            >
              <span className="mu-option-check">{value === option.value ? "✓" : ""}</span>
              {option.label}
              {option.note ? <span className="mu-quiet">{option.note}</span> : null}
            </div>,
          ])}
        </div>
      ) : null}
    </div>
  );
}
