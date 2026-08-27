import { useEffect } from "react";
import "./App.css";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "./components/layout/Layout";
import { ToastProvider } from "./components/ui/Toast";
import { DashboardPage } from "./pages/DashboardPage";
import { ChannelsPage } from "./pages/ChannelsPage";
import { ApiKeysPage } from "./pages/ApiKeysPage";
import { LogsPage } from "./pages/LogsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { UsagePage } from "./pages/UsagePage";
import { KnowledgeBasePage } from "./pages/KnowledgeBasePage";
import { McpPage } from "./pages/McpPage";
import { useThemeStore } from "./stores/themeStore";
import { useSettingsStore } from "./stores/settingsStore";

function App() {
  const setTheme = useThemeStore((s) => s.setTheme);
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  // 初始化主题与设置
  useEffect(() => {
    const stored = useThemeStore.getState().theme;
    setTheme(stored);
    loadSettings();
  }, [setTheme, loadSettings]);

  return (
    <BrowserRouter>
      <ToastProvider />
      <Layout>
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/usage" element={<UsagePage />} />
          <Route path="/channels" element={<ChannelsPage />} />
          <Route path="/api-keys" element={<ApiKeysPage />} />
          <Route path="/logs" element={<LogsPage />} />
          <Route path="/knowledge" element={<KnowledgeBasePage />} />
          <Route path="/mcp" element={<McpPage />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Routes>
      </Layout>
    </BrowserRouter>
  );
}

export default App;
