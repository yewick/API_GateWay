import { Toggle } from "../ui/Toggle";
import { useSettingsStore } from "../../stores/settingsStore";

export function GeneralTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <div className="space-y-6 max-w-xl">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">通用设置</h4>
        <p className="text-xs text-text-muted mb-4">应用行为相关配置</p>
      </div>

      <div className="space-y-4 bg-bg-tertiary/50 rounded-lg p-4 divide-y divide-border-primary/50">
        <Toggle
          checked={settings.minimize_to_tray}
          onChange={(v) => updateSettings({ minimize_to_tray: v })}
          label="最小化到托盘"
          description="点击最小化按钮时，隐藏窗口到系统托盘"
        />
        <Toggle
          checked={settings.close_to_tray}
          onChange={(v) => updateSettings({ close_to_tray: v })}
          label="关闭到托盘"
          description="点击关闭按钮时，隐藏窗口到系统托盘而非退出"
        />
        <Toggle
          checked={settings.auto_start}
          onChange={(v) => updateSettings({ auto_start: v })}
          label="开机自启"
          description="系统启动时自动运行 YeAPI"
        />
      </div>
    </div>
  );
}
