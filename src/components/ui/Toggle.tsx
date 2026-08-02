interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  description?: string;
  disabled?: boolean;
}

export function Toggle({ checked, onChange, label, description, disabled }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`group flex items-center justify-between w-full py-1 ${
        disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"
      }`}
    >
      {(label || description) && (
        <span className="text-left">
          {label && <span className="block text-sm text-text-primary">{label}</span>}
          {description && (
            <span className="block text-xs text-text-muted mt-0.5">{description}</span>
          )}
        </span>
      )}
      <span
        className={`relative inline-flex h-5.5 w-10 flex-shrink-0 items-center rounded-full transition-colors ml-4 ${
          checked ? "bg-accent" : "bg-bg-tertiary border border-border-primary"
        }`}
      >
        <span
          className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
            checked ? "translate-x-5" : "translate-x-1"
          }`}
        />
      </span>
    </button>
  );
}
