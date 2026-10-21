// A labelled field for what the person types (design system: TextField):
// the label tied to its input, the input, and a hint, which an error takes
// the place of, in `waiting`, saying what to do. The consumer shows an error
// only once the person has tried.

import {
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
  useId,
} from "react";
import "./text-field.css";

interface FieldProps {
  label: ReactNode;
  hint?: ReactNode;
  error?: string | null;
  /** Set in `font-mono`, for a path or a branch. */
  mono?: boolean;
}

function Field({
  id,
  label,
  hint,
  error,
  children,
}: Omit<FieldProps, "mono"> & { id: string; children: ReactNode }) {
  return (
    <div className={`mu-field ${error ? "has-error" : ""}`}>
      <label className="mu-field-label" htmlFor={id}>
        {label}
      </label>
      {children}
      {error || hint ? (
        <span className="mu-field-hint" id={`${id}-hint`}>
          {error || hint}
        </span>
      ) : null}
    </div>
  );
}

function inputClass(mono: boolean | undefined, area: boolean): string {
  return `mu-field-input ${area ? "mu-field-input--area" : ""} ${mono ? "mu-mono ui-field-mono" : ""}`;
}

/** One line. */
export function TextField({
  label,
  hint,
  error,
  mono,
  children,
  ...input
}: FieldProps & InputHTMLAttributes<HTMLInputElement> & { children?: ReactNode }) {
  const id = useId();
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <input
        {...input}
        id={id}
        className={inputClass(mono, false)}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${id}-hint` : undefined}
      />
      {children}
    </Field>
  );
}

/** Lines, growing downwards: a first prompt. */
export function TextArea({
  label,
  hint,
  error,
  mono,
  ...area
}: FieldProps & TextareaHTMLAttributes<HTMLTextAreaElement>) {
  const id = useId();
  return (
    <Field id={id} label={label} hint={hint} error={error}>
      <textarea
        {...area}
        id={id}
        className={inputClass(mono, true)}
        aria-invalid={error ? true : undefined}
        aria-describedby={error || hint ? `${id}-hint` : undefined}
      />
    </Field>
  );
}
