// A labelled field for what the person types (design system: TextField):
// the label tied to the input, then a hint, or what went wrong in its place.
// One line (TextField) or lines growing downwards (TextArea); a path or a
// branch in mono; what belongs under the input, as the folders under the
// path the person types, given as children.

import {
  type InputHTMLAttributes,
  type ReactNode,
  type Ref,
  type TextareaHTMLAttributes,
  useId,
} from "react";
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
 * A label over a control, and a hint or what went wrong under it. With an
 * `id` the label is the input's own; without one it names a control that
 * carries its own label (a Select, the agents' choice).
 */
export function Field({
  id,
  label,
  hint,
  error,
  children,
}: Pick<Said, "label" | "hint" | "error"> & { id?: string; children: ReactNode }) {
  const said = error || hint;
  return (
    <div className={`mu-field ${error ? "has-error" : ""}`}>
      {id ? (
        <label className="mu-field-label" htmlFor={id}>
          {label}
        </label>
      ) : (
        <span className="mu-field-label">{label}</span>
      )}
      {children}
      {said ? (
        <span
          id={id ? `${id}-hint` : undefined}
          className="mu-field-hint"
          role={error ? "alert" : undefined}
        >
          {said}
        </span>
      ) : null}
    </div>
  );
}

/** An input on its own, drawn as a field's (its label is elsewhere, or its placeholder says it). */
export function Input({
  mono,
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement> & { mono?: boolean; ref?: Ref<HTMLInputElement> }) {
  return <input {...rest} className={`${inputClass(false, mono)} ${className ?? ""}`} />;
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
  const id = useId();
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <input
        id={id}
        className={inputClass(false, mono)}
        autoComplete="off"
        spellCheck={false}
        value={value}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${id}-hint` : undefined}
        onChange={(event) => onChange(event.target.value)}
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
  const id = useId();
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <textarea
        id={id}
        className={inputClass(true, mono)}
        value={value}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${id}-hint` : undefined}
        onChange={(event) => onChange(event.target.value)}
        {...rest}
      />
    </Field>
  );
}
