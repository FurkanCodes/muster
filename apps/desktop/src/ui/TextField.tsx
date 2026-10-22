// A labelled field for what the person types (design system: TextField):
// the label tied to the input, then a hint, or what went wrong in its place.
// One line (TextField) or lines growing downwards (TextArea); a path or a
// branch in mono; what belongs under the input, as the folders under the
// path the person types, given as children. On Base UI's field and input,
// which tie the label, the hint and what went wrong to the input.

import { Field as BaseField } from "@base-ui/react/field";
import { Input as BaseInput } from "@base-ui/react/input";
import type { InputHTMLAttributes, ReactNode, Ref, TextareaHTMLAttributes } from "react";
import "./text-field.css";

interface Said {
  label: ReactNode;
  value: string;
  onChange: (value: string) => void;
  hint?: ReactNode;
  /** What went wrong, said in place of the hint; never before the person tried. */
  error?: string | null;
  /** Set in `font-mono`, for a path or a branch. */
  mono?: boolean;
}

/**
 * A label over a control, and a hint or what went wrong under it. Around
 * its own input (`labels`) the label is the input's; around a control that
 * carries its own label (a Select, the agents' choice) it only shows.
 */
export function Field({
  labels = false,
  label,
  hint,
  error,
  children,
}: Pick<Said, "label" | "hint" | "error"> & { labels?: boolean; children: ReactNode }) {
  const said = error || hint;
  return (
    <BaseField.Root
      className={`mu-field ${error ? "has-error" : ""}`}
      invalid={error ? true : undefined}
    >
      {labels ? (
        <BaseField.Label className="mu-field-label">{label}</BaseField.Label>
      ) : (
        <span className="mu-field-label">{label}</span>
      )}
      {children}
      {said ? (
        <BaseField.Description
          className="mu-field-hint"
          role={error ? "alert" : undefined}
          render={<span />}
        >
          {said}
        </BaseField.Description>
      ) : null}
    </BaseField.Root>
  );
}

/** What the person types into with a look of its own, given by its place (the palette's search): Base UI's input, with no look of the design system's. */
export function PlainInput(
  props: InputHTMLAttributes<HTMLInputElement> & { ref?: Ref<HTMLInputElement> },
) {
  return <BaseInput {...props} />;
}

/** Lines the person types with a look of their own (a project's prompt): Base UI's input as a text area. */
export function PlainTextArea({
  ref,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { ref?: Ref<HTMLTextAreaElement> }) {
  return <BaseInput render={<textarea ref={ref} {...props} />} />;
}

/** An input on its own, drawn as a field's (its label is elsewhere, or its placeholder says it). */
export function Input({
  mono,
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement> & { mono?: boolean; ref?: Ref<HTMLInputElement> }) {
  return <BaseInput {...rest} className={`${inputClass(false, mono)} ${className ?? ""}`} />;
}

const inputClass = (area: boolean, mono: boolean | undefined) =>
  `mu-field-input ${area ? "mu-field-input--area" : ""} ${mono ? "ui-field-mono" : ""}`;

export function TextField({
  label,
  value,
  onChange,
  hint,
  error,
  mono,
  children,
  ...rest
}: Said &
  Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "value"> & { children?: ReactNode }) {
  return (
    <Field labels label={label} hint={hint} error={error}>
      <BaseField.Control
        className={inputClass(false, mono)}
        autoComplete="off"
        spellCheck={false}
        value={value}
        onValueChange={(next) => onChange(next)}
        {...rest}
      />
      {children}
    </Field>
  );
}

export function TextArea({
  label,
  value,
  onChange,
  hint,
  error,
  mono,
  ...rest
}: Said & Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "onChange" | "value">) {
  return (
    <Field labels label={label} hint={hint} error={error}>
      <BaseField.Control
        render={<textarea {...rest} />}
        className={inputClass(true, mono)}
        value={value}
        onValueChange={(next) => onChange(next)}
      />
    </Field>
  );
}
