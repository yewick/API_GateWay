import { RefreshCw } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";

/**
 * 检查更新（暂停维护中）。
 * 保留按钮位，但不可点击；hover 提示「暂停维护中…」。
 * 待配置真实 updater 端点与签名公钥后，再恢复状态机与弹窗。
 */
export function UpdateChecker({ collapsed = false }: { collapsed?: boolean }) {
  return (
    <Tooltip content="暂停维护中…" className="w-full">
      <button
        type="button"
        disabled
        aria-label="检查更新（暂停维护中）"
        className={`flex items-center rounded-lg text-text-muted cursor-not-allowed opacity-60 ${
          collapsed ? "justify-center w-full px-0 py-2" : "w-full px-3 py-2 gap-2.5"
        }`}
      >
        <RefreshCw size={16} className="flex-shrink-0" />
        {!collapsed && <span className="text-sm font-medium">检查更新</span>}
      </button>
    </Tooltip>
  );
}
