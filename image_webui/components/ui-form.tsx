"use client";

import type {
  ChangeEvent,
  InputHTMLAttributes,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

// Native HTML form controls styled to match the dark HeroUI admin look.
// We use native elements because HeroUI v3's Input/TextArea/NumberField have
// very strict prop types (e.g. onChange takes a React.ChangeEvent) that are
// cumbersome to thread through our existing useState setters.

const FIELD_BASE =
  "block w-full rounded-lg border border-white/10 bg-black/30 px-3 py-2 text-sm text-white placeholder:text-white/40 outline-none transition-colors focus:border-blue-500/60 focus:bg-black/40 focus:ring-2 focus:ring-blue-500/20 disabled:opacity-50";

interface FieldProps {
  label?: string;
  description?: string;
  error?: string;
  className?: string;
}

function FieldShell({
  label,
  description,
  error,
  className,
  children,
}: FieldProps & { children: React.ReactNode }) {
  return (
    <label className={"block " + (className ?? "")}>
      {label ? (
        <span className="mb-1.5 block text-xs font-medium text-white/70">
          {label}
        </span>
      ) : null}
      {children}
      {description ? (
        <span className="mt-1 block text-xs text-white/40">{description}</span>
      ) : null}
      {error ? (
        <span className="mt-1 block text-xs text-rose-300">{error}</span>
      ) : null}
    </label>
  );
}

export type InputProps = FieldProps &
  Omit<InputHTMLAttributes<HTMLInputElement>, "onChange"> & {
    value: string | number;
    onValueChange?: (value: string) => void;
  };

export function Input({
  label,
  description,
  error,
  className,
  value,
  onValueChange,
  type = "text",
  ...rest
}: InputProps) {
  return (
    <FieldShell
      label={label}
      description={description}
      error={error}
      className={className}
    >
      <input
        {...rest}
        type={type}
        value={value}
        onChange={(e: ChangeEvent<HTMLInputElement>) =>
          onValueChange?.(e.target.value)
        }
        className={FIELD_BASE}
      />
    </FieldShell>
  );
}

export type TextAreaProps = FieldProps &
  Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "onChange"> & {
    value: string;
    onValueChange: (value: string) => void;
  };

export function TextArea({
  label,
  description,
  error,
  className,
  value,
  onValueChange,
  rows = 4,
  ...rest
}: TextAreaProps) {
  return (
    <FieldShell
      label={label}
      description={description}
      error={error}
      className={className}
    >
      <textarea
        {...rest}
        rows={rows}
        value={value}
        onChange={(e: ChangeEvent<HTMLTextAreaElement>) =>
          onValueChange(e.target.value)
        }
        className={FIELD_BASE + " resize-y leading-relaxed"}
      />
    </FieldShell>
  );
}

export type NumberInputProps = FieldProps &
  Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> & {
    value: number;
    onValueChange: (value: number) => void;
  };

export function NumberInput({
  label,
  description,
  error,
  className,
  value,
  onValueChange,
  min,
  max,
  step,
  ...rest
}: NumberInputProps) {
  return (
    <FieldShell
      label={label}
      description={description}
      error={error}
      className={className}
    >
      <input
        {...rest}
        type="number"
        value={Number.isFinite(value) ? value : ""}
        onChange={(e) => {
          const v = e.target.value;
          onValueChange(v === "" ? 0 : Number(v));
        }}
        min={min}
        max={max}
        step={step}
        className={FIELD_BASE}
      />
    </FieldShell>
  );
}

export type NativeSelectProps = FieldProps &
  Omit<SelectHTMLAttributes<HTMLSelectElement>, "value" | "onChange"> & {
    value: string;
    onValueChange: (value: string) => void;
    options: Array<{ value: string; label: string }>;
  };

export function NativeSelect({
  label,
  description,
  error,
  className,
  value,
  onValueChange,
  options,
  ...rest
}: NativeSelectProps) {
  return (
    <FieldShell
      label={label}
      description={description}
      error={error}
      className={className}
    >
      <select
        {...rest}
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
        className={FIELD_BASE + " appearance-none pr-8"}
        style={{
          backgroundImage:
            'url("data:image/svg+xml;charset=US-ASCII,%3Csvg xmlns=%27http://www.w3.org/2000/svg%27 width=%2712%27 height=%2712%27 viewBox=%270 0 12 12%27 fill=%27none%27%3E%3Cpath d=%27M3 4.5 6 7.5 9 4.5%27 stroke=%27%23999%27 stroke-width=%271.5%27 stroke-linecap=%27round%27/%3E%3C/svg%3E")',
          backgroundRepeat: "no-repeat",
          backgroundPosition: "right 0.75rem center",
        }}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value} className="bg-zinc-900">
            {opt.label}
          </option>
        ))}
      </select>
    </FieldShell>
  );
}