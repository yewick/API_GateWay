import type { SelectHTMLAttributes } from "react";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> {
  label?: string;
  options: SelectOption[];
  error?: string;
  hint?: string;
  placeholder?: string;
}

export function Select({
  label,
  options,
  error,
  hint,
  placeholder,
  className = "",
  ...rest
}: SelectProps) {
  return (
    <div>
      {label && (
        <label className="block mb-1.5 text-sm font-medium text-text-secondary">
          {label}
        </label>
      )}
      <select
        {...rest}
        className={`w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg
          text-text-primary outline-none transition-colors cursor-pointer
          focus:border-accent focus:ring-1 focus:ring-accent/40 ${className}`}
      >
        {placeholder && (
          <option value="" disabled>
            {placeholder}
          </option>
        )}
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      {error && <p className="mt-1 text-xs text-danger">{error}</p>}
      {!error && hint && <p className="mt-1 text-xs text-text-muted">{hint}</p>}
    </div>
  );
}
