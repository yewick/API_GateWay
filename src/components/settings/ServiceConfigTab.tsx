import { Input } from "../ui/Input";
import { useSettingsStore } from "../../stores/settingsStore";

export function ServiceConfigTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <div className="space-y-4 max-w-xl">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">服务配置</h4>
        <p className="text-xs text-text-muted mb-4">
          配置网关本地 HTTP 服务的监听参数。修改后需重启服务生效。
        </p>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <Input
          label="服务端口"
          type="number"
          value={String(settings.server_port)}
          onChange={(e) =>
            updateSettings({ server_port: parseInt(e.target.value, 10) || 8777 }) // 默认端口 8777
          }
          hint="网关 HTTP 服务监听端口"
        />
        <Input
          label="监听地址"
          value={settings.server_host}
          onChange={(e) => updateSettings({ server_host: e.target.value })}
          hint="127.0.0.1 仅本地访问"
        />
      </div>
    </div>
  );
}
