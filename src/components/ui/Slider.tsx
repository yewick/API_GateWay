interface SliderProps {
  label?: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (value: number) => void;
}

export function Slider({ label, value, min = 0, max = 1, step = 0.05, onChange }: SliderProps) {
  return (
    <div>
      {label && (
        <div className="mb-1 flex items-center justify-between">
          <label className="text-sm font-medium text-text-secondary">{label}</label>
          <span className="text-xs text-text-muted tabular">{value.toFixed(2)}</span>
        </div>
      )}
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full cursor-pointer accent-accent"
      />
    </div>
  );
}
