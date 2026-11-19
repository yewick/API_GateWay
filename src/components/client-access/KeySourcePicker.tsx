import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Tabs } from "../ui/Tabs";
import type { ApiKey } from "../../types";

type KeyMode = "new" | "existing";

interface KeySourcePickerProps {
  mode: KeyMode;
  onModeChange: (mode: KeyMode) => void;
  name: string;
  onNameChange: (name: string) => void;
  keys: ApiKey[];
  keyId: string;
  onKeyIdChange: (id: string) => void;
  namePlaceholder?: string;
  nameHint?: string;
}

const maskKey = (key: string): string => {
  if (key.length <= 8) return key;
  return `${key.slice(0, 6)}…${key.slice(-4)}`;
};

export function KeySourcePicker({
  mode,
  onModeChange,
  name,
  onNameChange,
  keys,
  keyId,
  onKeyIdChange,
  namePlaceholder = "Codex",
  nameHint = "接入时自动创建；明文只写入 Codex 配置，不在界面回显。",
}: KeySourcePickerProps) {
  return (
    <div className="space-y-3">
      <Tabs
        tabs={[
          { key: "new", label: "新建专用 Key" },
          { key: "existing", label: "选择已有 Key" },
        ]}
        activeKey={mode}
        onChange={(key) => onModeChange(key as KeyMode)}
      />

      {mode === "new" ? (
        <Input
          label="密钥名称"
          value={name}
          onChange={(event) => onNameChange(event.target.value)}
          placeholder={namePlaceholder}
          hint={nameHint}
        />
      ) : keys.length === 0 ? (
        <p className="text-xs text-text-muted">
          还没有可用密钥。请切换到「新建专用 Key」，或先到「密钥」页创建。
        </p>
      ) : (
        <Select
          label="选择密钥"
          value={keyId}
          onChange={(event) => onKeyIdChange(event.target.value)}
          placeholder="请选择"
          options={keys.map((key) => ({
            value: key.id,
            label: `${key.name} · ${maskKey(key.key)}`,
          }))}
        />
      )}
    </div>
  );
}
