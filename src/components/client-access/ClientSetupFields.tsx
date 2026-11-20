import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import type { SetupModelOption } from "../../types";

export interface ModelFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  customValue: string;
  onCustomChange: (value: string) => void;
  models: SetupModelOption[];
  allowEmpty?: boolean;
  emptyLabel?: string;
  hint?: string;
  /// 为 false 时只允许从给定模型中选择（用于受密钥限制的场景）。
  allowCustom?: boolean;
}

export function ModelField({
  label,
  value,
  onChange,
  customValue,
  onCustomChange,
  models,
  allowEmpty = false,
  emptyLabel = "跟随主模型",
  hint,
  allowCustom = true,
}: ModelFieldProps) {
  const options: { value: string; label: string }[] = [];
  if (allowEmpty) {
    options.push({ value: "", label: emptyLabel });
  }
  options.push(
    ...models.map((item) => ({
      value: item.id,
      label: item.owned_by ? `${item.id} · ${item.owned_by}` : item.id,
    })),
  );
  if (
    allowCustom &&
    value &&
    value !== "__custom__" &&
    !models.some((item) => item.id === value)
  ) {
    options.unshift({ value, label: `${value}（当前配置）` });
  }
  if (allowCustom) {
    options.push({ value: "__custom__", label: "自定义模型名…" });
  }

  return (
    <div className="space-y-2">
      <Select
        label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        options={options}
        hint={hint}
      />
      {value === "__custom__" && (
        <Input
          value={customValue}
          onChange={(event) => onCustomChange(event.target.value)}
          placeholder="例如 deepseek-v4-pro"
        />
      )}
    </div>
  );
}

/// 把已有配置值映射成「下拉选中项 + 自定义输入」。
export function splitModelValue(
  value: string | null | undefined,
  models: SetupModelOption[],
): [string, string] {
  if (!value) return ["", ""];
  if (models.some((item) => item.id === value)) return [value, ""];
  return ["__custom__", value];
}

export const aliasValue = (selection: string, custom: string) =>
  selection === "__custom__" ? custom.trim() : selection;
