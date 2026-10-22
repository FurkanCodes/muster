// Buttons, key caps, hints, segmented controls (design system: Actions).
// Variants are explicit props with a closed set of values, never stacks of
// booleans (composition: patterns-explicit-variants).

import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";
import { Tooltip } from "./Tooltip";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "quiet" | "danger";
  size?: "sm" | "md" | "lg";
  icon?: IconName;
  /** A key hint after the label, for buttons in a strip of keyboard actions. */
  kbd?: string;
  busy?: boolean;
};

export function Button({
  variant = "quiet",
  size = "md",
  icon,
  kbd,
  busy = false,
  children,
  className = "",
  type = "button",
  ...rest
}: ButtonProps) {
  const sizeClass = size === "md" ? "" : `mu-btn--${size}`;
  return (
    <button
      type={type}
      className={`mu-btn mu-btn--${variant} ${sizeClass} ${className}`}
      aria-busy={busy || undefined}
      {...rest}
    >
      {busy ? <Spinner /> : icon ? <Icon name={icon} /> : null}
      {children}
      {kbd ? <Kbd className="mu-key">{kbd}</Kbd> : null}
    </button>
  );
}

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  icon: IconName;
  label: string;
  size?: "sm" | "md";
  pressed?: boolean;
};

/** A button that is only its icon: its label is its name, and its tooltip (`title`, else the label) says it with its key. */
export function IconButton({
  icon,
  label,
  size = "md",
  pressed,
  className = "",
  title,
  ...rest
}: IconButtonProps) {
  return (
    <Tooltip content={title ?? label}>
      <button
        type="button"
        className={`mu-icon-btn ${size === "sm" ? "mu-icon-btn--sm" : ""} ${className}`}
        aria-label={label}
        aria-pressed={pressed}
        {...rest}
      >
        <Icon name={icon} size={size === "sm" ? 14 : 16} />
      </button>
    </Tooltip>
  );
}

export function Kbd({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <kbd className={`mu-kbd ${className}`}>{children}</kbd>;
}

/** A key cap and what it does. */
export function Hint({ keys, does }: { keys: string; does: string }) {
  return (
    <span className="mu-hint">
      <Kbd>{keys}</Kbd>
      {does}
    </span>
  );
}

/**
 * The working spinner: CSS only, so it costs nothing while hidden; `still`,
 * it shows its first step and draws nothing more, for a mark that stands for
 * work while none is going on (`ARC-25`).
 */
export function Spinner({ still = false }: { still?: boolean }) {
  return (
    <span className={still ? "mu-spin mu-spin--still" : "mu-spin"} aria-hidden="true">
      <span className="mu-spin-strip" />
    </span>
  );
}

export interface Segment<T extends string> {
  value: T;
  label: string;
  /** How many it holds, after its label; none where there are none. */
  count?: number;
  /** The count's tone: `waiting` where they wait on the person. */
  tone?: "waiting";
}

export function Segmented<T extends string>({
  label,
  segments,
  value,
  onChange,
  busy = false,
  className,
}: {
  label: string;
  segments: readonly Segment<T>[];
  /** Where it sits, for its place's own layout (the sidebar's lists' switch fills its width). */
  className?: string;
  value: T;
  onChange: (value: T) => void;
  /** A change is on its way: no second press until it is answered. */
  busy?: boolean;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a row of toggle buttons, not a form; a fieldset would bring a legend and form semantics.
    <div
      className={className ? `mu-seg ${className}` : "mu-seg"}
      role="group"
      aria-label={label}
      aria-busy={busy || undefined}
    >
      {segments.map((segment) => (
        <button
          key={segment.value}
          type="button"
          className="mu-seg-item"
          aria-pressed={value === segment.value}
          disabled={busy}
          onClick={() => onChange(segment.value)}
        >
          {segment.label}
          {segment.count ? (
            <span
              className={
                segment.tone ? `mu-seg-count mu-seg-count--${segment.tone}` : "mu-seg-count"
              }
            >
              {segment.count}
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}
