import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "dark" | "light" | "paper" | "system";
export type ResolvedTheme = "dark" | "light" | "paper";

// 主题循环顺序（侧边栏主题按钮按此顺序切换）
const THEME_ORDER: Theme[] = ["dark", "light", "paper", "system"];

interface ThemeStore {
  theme: Theme;
  resolved: ResolvedTheme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

// 系统深浅色偏好
const systemPrefersDark = (): boolean =>
  typeof window !== "undefined" &&
  window.matchMedia("(prefers-color-scheme: dark)").matches;

// "system" 解析为具体的 dark/light，其余主题原样返回
const resolveTheme = (theme: Theme): ResolvedTheme =>
  theme === "system" ? (systemPrefersDark() ? "dark" : "light") : theme;

// 应用主题到 DOM（永远写入解析后的具体主题值）
const applyTheme = (resolved: ResolvedTheme) => {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", resolved);
  }
};

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      theme: "dark",
      resolved: "dark",
      setTheme: (theme) => {
        const resolved = resolveTheme(theme);
        applyTheme(resolved);
        set({ theme, resolved });
      },
      toggleTheme: () => {
        const next = THEME_ORDER[(THEME_ORDER.indexOf(get().theme) + 1) % THEME_ORDER.length];
        get().setTheme(next);
      },
    }),
    {
      name: "yeapi-theme",
      // 只持久化偏好 theme，resolved 由 theme 派生，避免过期
      partialize: (state) => ({ theme: state.theme }),
      onRehydrateStorage: () => (state) => {
        if (state) {
          const resolved = resolveTheme(state.theme);
          applyTheme(resolved);
          useThemeStore.setState({ resolved });
        }
      },
    },
  ),
);

// 监听系统深浅色变化：当主题为 "system" 时实时跟随
if (typeof window !== "undefined") {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", () => {
    const { theme } = useThemeStore.getState();
    if (theme === "system") {
      useThemeStore.getState().setTheme("system");
    }
  });
}

// 初始化时应用主题
if (typeof window !== "undefined") {
  const { theme } = useThemeStore.getState();
  applyTheme(resolveTheme(theme));
}
