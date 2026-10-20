/**
 * Form primitives: a labelled field plus the text, number, checkbox and select
 * controls the modes use. The field reserves a help/error slot (§2.2).
 */

import type {
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
} from "react";

import { cx } from "@/lib/cx";

export interface FieldProps {
  label: string;
  /** The `id` of the control this label describes. */
  htmlFor?: string;
  /** Helper text shown below the control. */
  help?: string;
  /** Error text; replaces the helper and is announced. */
  error?: string | null;
  children: ReactNode;
  className?: string;
}

/** A label, a control and a help/error slot. */
export function Field({
  label,
  htmlFor,
  help,
  error,
  children,
  className,
}: FieldProps) {
  return (
    <div className={cx("field", className)}>
      <label className="field__label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
      {error ? (
        <p className="field__error" role="alert">
          <span aria-hidden="true">✕ </span>
          {error}
        </p>
      ) : (
        help && <p className="field__help">{help}</p>
      )}
    </div>
  );
}

/** A single-line text input. */
export function TextInput({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cx("input", className)} {...rest} />;
}

/** A numeric input. */
export function NumberInput({
  className,
  ...rest
}: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input type="number" className={cx("input", "input--number", className)} {...rest} />
  );
}

/** A labelled checkbox. */
export interface CheckboxProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "type"> {
  label: string;
}

export function Checkbox({ label, className, id, ...rest }: CheckboxProps) {
  return (
    <label className={cx("checkbox", className)} htmlFor={id}>
      <input type="checkbox" id={id} className="checkbox__input" {...rest} />
      <span className="checkbox__label">{label}</span>
    </label>
  );
}

/** A select control. */
export function Select({
  className,
  children,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={cx("select", className)} {...rest}>
      {children}
    </select>
  );
}
