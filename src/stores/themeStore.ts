import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "dark" | "light" | "paper";

// 主题循环顺序（侧边栏主题按钮按此顺序切换）
const THEME_ORDER: Theme[] = ["dark", "light", "paper"];

interface ThemeStore {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

// 应用主题到 DOM
const applyTheme = (theme: Theme) => {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme);
  }
};

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      theme: "dark",
      setTheme: (theme) => {
        applyTheme(theme);
        set({ theme });
      },
      toggleTheme: () => {
        const next = THEME_ORDER[(THEME_ORDER.indexOf(get().theme) + 1) % THEME_ORDER.length];
        get().setTheme(next);
      },
    }),
    {
      name: "yeapi-theme",
      onRehydrateStorage: () => (state) => {
        if (state) applyTheme(state.theme);
      },
    },
  ),
);

// 初始化时应用主题
if (typeof window !== "undefined") {
  applyTheme(useThemeStore.getState().theme);
}
