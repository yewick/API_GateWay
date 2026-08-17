import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Settings } from "../types";
import { DEFAULT_SETTINGS } from "../lib/constants";
import { settingsApi } from "../lib/api";
import { useThemeStore, type Theme } from "./themeStore";

// 后端服务器最后一次启动所用的监听参数（非持久化，用于判断保存后是否需重启）
let lastServerConfig: { port: number; host: string } = {
  port: DEFAULT_SETTINGS.server_port,
  host: DEFAULT_SETTINGS.server_host,
};

interface SettingsStore {
  settings: Settings;
  loaded: boolean;
  updateSettings: (partial: Partial<Settings>) => void;
  saveSettings: () => Promise<void>;
  loadSettings: () => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>()(
  persist(
    (set, get) => ({
      settings: { ...DEFAULT_SETTINGS },
      loaded: false,

      updateSettings: (partial) => {
        set((state) => ({ settings: { ...state.settings, ...partial } }));
        // 主题变更时同步到 themeStore 和 DOM
        if (partial.ui_theme) {
          useThemeStore.getState().setTheme(partial.ui_theme as Theme);
        }
        if (partial.ui_language && typeof document !== "undefined") {
          document.documentElement.lang = partial.ui_language;
        }
      },

      saveSettings: async () => {
        const { settings } = get();
        try {
          await settingsApi.save(settings);
          // 服务地址/端口变化时，重启本地 HTTP 服务使其生效
          if (
            settings.server_port !== lastServerConfig.port ||
            settings.server_host !== lastServerConfig.host
          ) {
            await settingsApi.restartServer();
            lastServerConfig = { port: settings.server_port, host: settings.server_host };
          }
        } catch (e) {
          console.warn("[settings] 后端保存失败，已保留本地设置", e);
        }
      },

      loadSettings: async () => {
        try {
          const remote = await settingsApi.get();
          if (remote && remote.server_port !== undefined) {
            lastServerConfig = { port: remote.server_port, host: remote.server_host };
            // 主题以 themeStore 持久化值为准，不在此覆盖，避免与用户选择冲突
            const { ui_theme: _uiTheme, ui_language, ...rest } = remote;
            if (ui_language && typeof document !== "undefined") {
              document.documentElement.lang = ui_language;
            }
            set((state) => ({
              settings: { ...state.settings, ...rest, ui_theme: state.settings.ui_theme },
              loaded: true,
            }));
          }
        } catch (e) {
          console.warn("[settings] 读取后端设置失败，使用默认值", e);
          set({ loaded: true });
        }
      },
    }),
    {
      name: "yeapi-settings",
      // 只持久化 settings 字段（通过 getItem 自定义实现，保持简单直接持久化整个 state 中的 settings）
      partialize: (state) => ({ settings: state.settings }),
    },
  ),
);
