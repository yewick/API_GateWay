import { Input } from "../ui/Input";
import { Toggle } from "../ui/Toggle";
import { useSettingsStore } from "../../stores/settingsStore";

export function RetryStrategyTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <div className="space-y-6 max-w-xl">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">重试策略</h4>
        <p className="text-xs text-text-muted mb-4">
          当上游请求失败时，网关自动重试的配置
        </p>
      </div>

      <div className="space-y-4 bg-bg-tertiary/50 rounded-lg p-4">
        <Toggle
          checked={settings.retry_enabled}
          onChange={(v) => updateSettings({ retry_enabled: v })}
          label="启用自动重试"
          description="请求失败（如 5xx、网络错误）时自动重试"
        />
      </div>

      <div className={settings.retry_enabled ? "" : "opacity-50 pointer-events-none"}>
        <Input
          label="最大重试次数"
          type="number"
          min={0}
          max={10}
          value={String(settings.retry_times)}
          onChange={(e) => {
            const v = parseInt(e.target.value, 10);
            updateSettings({ retry_times: Number.isNaN(v) ? 0 : Math.min(10, Math.max(0, v)) });
          }}
          hint="建议 0-5 次，过多的重试会放大上游压力"
        />
      </div>
    </div>
  );
}
