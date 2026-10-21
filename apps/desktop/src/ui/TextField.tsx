// A labelled field for what the person types (design system: TextField):
// the label tied to the input, then a hint, or what went wrong in its place.

import { type InputHTMLAttributes, useId } from "react";

type FieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "value"> & {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
  /** What went wrong, said in place of the hint; never before the person tried. */
  error?: string | null;
};

export function TextField({ label, value, onChange, hint, error, ...rest }: FieldProps) {
  const id = useId();
  const said = error || hint;
  return (
    <div className={`mu-field ${error ? "has-error" : ""}`}>
      <label className="mu-field-label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="mu-field-input"
        autoComplete="off"
        spellCheck={false}
        value={value}
        aria-invalid={error ? true : undefined}
        aria-describedby={said ? `${id}-hint` : undefined}
        onChange={(event) => onChange(event.target.value)}
        {...rest}
      />
      {said ? (
        <span id={`${id}-hint`} className="mu-field-hint" role={error ? "alert" : undefined}>
          {said}
        </span>
      ) : null}
    </div>
  );
}
