import { useThemeStore, type Theme, type ResolvedTheme } from "../stores/themeStore";

export interface UseThemeReturn {
  theme: Theme;
  resolved: ResolvedTheme;
  isDark: boolean;
  toggleTheme: () => void;
  setTheme: (theme: Theme) => void;
}

export const useTheme = (): UseThemeReturn => {
  const theme = useThemeStore((s) => s.theme);
  const resolved = useThemeStore((s) => s.resolved);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const setTheme = useThemeStore((s) => s.setTheme);

  return {
    theme,
    resolved,
    isDark: resolved === "dark",
    toggleTheme,
    setTheme,
  };
};
