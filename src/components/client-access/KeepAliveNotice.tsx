import { Info, Settings2, X } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "../ui/Button";
import { clientSetupApi } from "../../lib/api";
import { useClientSetupStatus } from "../../hooks/useClientSetup";
import { useSettingsStore } from "../../stores/settingsStore";

/// 首次接入后的常驻提示，只出现一次；不默认勾选任何系统项。
export function KeepAliveNotice() {
  const navigate = useNavigate();
  const settings = useSettingsStore((state) => state.settings);
  const updateSettings = useSettingsStore((state) => state.updateSettings);
  const codex = useClientSetupStatus("codex");
  const claude = useClientSetupStatus("claude");

  const anyApplied = Boolean(codex.data?.applied || claude.data?.applied);
  const needsPrompt =
    !settings.client_setup_keep_alive_prompted &&
    (!settings.auto_start || !settings.close_to_tray);

  if (!anyApplied || !needsPrompt) return null;

  const dismiss = async () => {
    try {
      await clientSetupApi.dismissKeepAliveHint();
    } catch {
      // 即使落库失败也先隐藏，避免反复打扰。
    }
    updateSettings({ client_setup_keep_alive_prompted: true });
  };

  return (
    <div className="mb-4 flex items-start gap-3 rounded-xl border border-info/30 bg-info/10 px-4 py-3 text-info">
      <Info size={18} className="mt-0.5 shrink-0" />
      <div className="min-w-0 flex-1 text-xs leading-relaxed">
        YeAPI 关闭后本地网关会停止，已接入的客户端将无法连接。可在设置中开启
        「开机自启」与「关闭到托盘」，让网关常驻；两项默认关闭，由你决定。
      </div>
      <Button
        size="sm"
        variant="secondary"
        onClick={() => navigate("/settings")}
      >
        <Settings2 size={14} />
        去设置
      </Button>
      <button
        type="button"
        aria-label="知道了，不再提示"
        title="知道了，不再提示"
        onClick={dismiss}
        className="p-1 rounded-md text-info/80 hover:text-info hover:bg-info/10"
      >
        <X size={16} />
      </button>
    </div>
  );
}
