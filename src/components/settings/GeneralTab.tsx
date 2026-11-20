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

      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">客户端接入</h4>
        <p className="text-xs text-text-muted mb-4">
          网关停止后，已接入的 Codex / Claude Code 将无法连接，建议保持 YeAPI 常驻。
        </p>
        <div className="bg-bg-tertiary/50 rounded-lg p-4">
          <Toggle
            checked={settings.client_setup_auto_repair}
            onChange={(v) => updateSettings({ client_setup_auto_repair: v })}
            label="启动时自动修复客户端接入"
            description="自动更新网关地址、重建 Codex 模型目录、轮换失效的托管密钥；不会覆盖其他工具（如 cc-switch）写入的配置"
          />
        </div>
      </div>
    </div>
  );
}
