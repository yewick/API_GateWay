import { Moon, Sun, FileText, MonitorSmartphone } from "lucide-react";
import { useTheme } from "../../hooks/useTheme";
import type { Theme } from "../../stores/themeStore";

interface ThemeToggleProps {
  showLabel?: boolean;
  className?: string;
}

const THEME_META: Record<Theme, { icon: typeof Sun; label: string }> = {
  dark: { icon: Moon, label: "深色模式" },
  light: { icon: Sun, label: "浅色模式" },
  paper: { icon: FileText, label: "墨纸模式" },
  system: { icon: MonitorSmartphone, label: "跟随系统" },
};

export function ThemeToggle({ showLabel = false, className = "" }: ThemeToggleProps) {
  const { theme, toggleTheme } = useTheme();
  const { icon: Icon, label } = THEME_META[theme];

  return (
    <button
      onClick={toggleTheme}
      className={`inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-text-secondary
        hover:text-text-primary hover:bg-bg-hover transition-colors ${className}`}
      aria-label={`当前主题：${label}，点击切换`}
      title={`当前主题：${label}，点击切换`}
    >
      <Icon size={18} />
      {showLabel && <span>{label}</span>}
    </button>
  );
}
