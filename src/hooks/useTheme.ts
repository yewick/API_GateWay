import { useThemeStore, type Theme } from "../stores/themeStore";

export interface UseThemeReturn {
  theme: Theme;
  isDark: boolean;
  toggleTheme: () => void;
  setTheme: (theme: Theme) => void;
}

export const useTheme = (): UseThemeReturn => {
  const theme = useThemeStore((s) => s.theme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const setTheme = useThemeStore((s) => s.setTheme);

  return {
    theme,
    isDark: theme === "dark",
    toggleTheme,
    setTheme,
  };
};
