import { useThemeStore } from "../stores/themeStore";

export interface UseThemeReturn {
  theme: "dark" | "light";
  isDark: boolean;
  toggleTheme: () => void;
  setTheme: (theme: "dark" | "light") => void;
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
