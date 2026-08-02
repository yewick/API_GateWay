import { Moon, Sun, MonitorSmartphone } from "lucide-react";
import { useSettingsStore } from "../../stores/settingsStore";
import { useTheme } from "../../hooks/useTheme";
import { LANGUAGES } from "../../lib/constants";

export function UISettingsTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const { theme, isDark, setTheme } = useTheme();

  const selectTheme = (t: "dark" | "light") => {
    setTheme(t);
    updateSettings({ ui_theme: t });
  };

  const themes = [
    { value: "dark" as const, label: "深色模式", icon: Moon, desc: "Clash Verge 风格深色主题", active: theme === "dark" },
    { value: "light" as const, label: "浅色模式", icon: Sun, desc: "清爽明亮浅色主题", active: theme === "light" },
  ];

  return (
    <div className="space-y-6 max-w-xl">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">界面设置</h4>
        <p className="text-xs text-text-muted mb-4">主题与语言偏好</p>
      </div>

      {/* 主题选择 */}
      <div>
        <label className="block mb-2 text-sm font-medium text-text-secondary">
          界面主题
        </label>
        <div className="grid grid-cols-2 gap-4">
          {themes.map((t) => {
            const Icon = t.icon;
            return (
              <button
                key={t.value}
                type="button"
                onClick={() => selectTheme(t.value)}
                className={`relative p-4 rounded-xl border-2 text-left transition-all ${
                  t.active
                    ? "border-accent bg-accent/5"
                    : "border-border-primary bg-bg-tertiary hover:border-text-muted"
                }`}
              >
                <div className="flex items-center gap-2.5 mb-2">
                  <Icon
                    size={20}
                    className={t.active ? "text-accent" : "text-text-secondary"}
                  />
                  <span className="text-sm font-medium text-text-primary">
                    {t.label}
                  </span>
                </div>
                <p className="text-xs text-text-muted">{t.desc}</p>
                {t.active && (
                  <span className="absolute top-3 right-3 w-2 h-2 rounded-full bg-accent" />
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* 当前状态预览 */}
      <div className="flex items-center gap-3 p-3 bg-bg-tertiary rounded-lg">
        <MonitorSmartphone size={18} className="text-text-secondary" />
        <span className="text-sm text-text-secondary">
          当前模式：
          <span className="font-medium text-text-primary">
            {isDark ? "深色" : "浅色"}
          </span>
          ，点击左侧主题卡片或顶栏按钮即可切换
        </span>
      </div>

      {/* 语言选择 */}
      <div>
        <label className="block mb-2 text-sm font-medium text-text-secondary">
          界面语言
        </label>
        <select
          value={settings.ui_language}
          onChange={(e) => {
            updateSettings({ ui_language: e.target.value });
          }}
          className="w-full px-3 py-2 text-sm bg-bg-tertiary border border-border-primary rounded-lg text-text-primary outline-none focus:border-accent cursor-pointer"
        >
          {LANGUAGES.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
