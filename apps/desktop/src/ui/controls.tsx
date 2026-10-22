// Buttons, key caps, hints, segmented controls (design system: Actions),
// on Base UI's button and toggle group. Variants are explicit props with a
// closed set of values, never stacks of booleans (composition:
// patterns-explicit-variants).

import { Button as BaseButton } from "@base-ui/react/button";
import { Toggle } from "@base-ui/react/toggle";
import { ToggleGroup } from "@base-ui/react/toggle-group";
import type { ButtonHTMLAttributes, ComponentProps } from "react";
import { Icon, type IconName } from "./Icon";
import { nativeButton, type PartProps, usePart } from "./part";
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
    <BaseButton
      render={nativeButton}
      type={type}
      className={`mu-btn mu-btn--${variant} ${sizeClass} ${className}`}
      aria-busy={busy || undefined}
      {...rest}
    >
      {busy ? <Spinner /> : icon ? <Icon name={icon} /> : null}
      {children}
      {kbd ? <Kbd className="mu-key">{kbd}</Kbd> : null}
    </BaseButton>
  );
}

/**
 * Something pressed that has a look of its own, given by the place it is in
 * (a head's link, a row's way in): Base UI's button, with no look of the
 * design system's.
 */
export function Pressable({ type = "button", ...rest }: ComponentProps<typeof BaseButton>) {
  return <BaseButton render={nativeButton} type={type} {...rest} />;
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
      <BaseButton
        render={nativeButton}
        type="button"
        className={`mu-icon-btn ${size === "sm" ? "mu-icon-btn--sm" : ""} ${className}`}
        aria-label={label}
        aria-pressed={pressed}
        {...rest}
      >
        <Icon name={icon} size={size === "sm" ? 14 : 16} />
      </BaseButton>
    </Tooltip>
  );
}

export function Kbd(props: PartProps<"kbd">) {
  return usePart("kbd", props, { className: "mu-kbd" });
}

/** A key cap and what it does. */
/** A key and what it does: `⏎ answer`; several keys that do the same, each in its cap. */
export function Hint({
  keys,
  does,
  ...props
}: PartProps<"span"> & { keys: string | readonly string[]; does: string }) {
  return usePart("span", props, {
    className: "mu-hint",
    children: (
      <>
        {typeof keys === "string" ? (
          <Kbd>{keys}</Kbd>
        ) : (
          keys.map((key) => <Kbd key={key}>{key}</Kbd>)
        )}
        {does}
      </>
    ),
  });
}

/**
 * The working spinner: CSS only, so it costs nothing while hidden; `still`,
 * it shows its first step and draws nothing more, for a mark that stands for
 * work while none is going on (`ARC-25`).
 */
export function Spinner({ still = false, ...props }: PartProps<"span"> & { still?: boolean }) {
  return usePart("span", props, {
    className: still ? "mu-spin mu-spin--still" : "mu-spin",
    "aria-hidden": true,
    children: <span className="mu-spin-strip" />,
  });
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
    <ToggleGroup
      className={className ? `mu-seg ${className}` : "mu-seg"}
      aria-label={label}
      aria-busy={busy || undefined}
      value={[value]}
      onValueChange={(pressed) => {
        // One is always pressed: pressing it again keeps it.
        const next = pressed[0] as T | undefined;
        if (next && next !== value) onChange(next);
      }}
      // ← → are the screen's (a setting steps on them), not a walk along the buttons.
      onKeyDown={(event) => {
        if (event.key.startsWith("Arrow")) event.preventBaseUIHandler();
      }}
    >
      {segments.map((segment) => (
        <Toggle
          key={segment.value}
          value={segment.value}
          render={nativeButton}
          className="mu-seg-item"
          disabled={busy}
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
        </Toggle>
      ))}
    </ToggleGroup>
  );
}

/** Room taking what is left in a row, pushing what follows to its end; where it `dragsWindow`, the window moves by it. */
export function Spacer({
  dragsWindow = false,
  ...props
}: PartProps<"span"> & { dragsWindow?: boolean }) {
  return usePart("span", props, {
    className: "mu-spacer",
    ...(dragsWindow ? { "data-tauri-drag-region": true } : {}),
  });
}
