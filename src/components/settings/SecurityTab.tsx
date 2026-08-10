import { Select } from "../ui/Select";
import { Toggle } from "../ui/Toggle";
import { useSettingsStore } from "../../stores/settingsStore";

const MODE_OPTIONS = [
  { value: "audit", label: "审计模式 (audit)" },
  { value: "warn", label: "警告模式 (warn)" },
  { value: "redact", label: "脱敏模式 (redact)" },
  { value: "block", label: "阻断模式 (block)" },
];

const MODE_HINTS: Record<string, string> = {
  audit: "仅记录安全事件，不干预请求/响应",
  warn: "Medium 以上风险标记警告，前端可提示但放行",
  redact: "High 以上风险先脱敏密钥后再转发",
  block: "High 以上风险直接拒绝，返回 451 状态码",
};

export function SecurityTab() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  return (
    <div className="space-y-6 max-w-xl">
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">安全审计</h4>
        <p className="text-xs text-text-muted mb-4">
          配置 LLM 请求/响应的内容安全扫描策略。扫描在 Proxy 层实时执行。
        </p>
      </div>

      {/* 总开关 */}
      <div className="space-y-4 bg-bg-tertiary/50 rounded-lg p-4 divide-y divide-border-primary/50">
        <Toggle
          checked={settings.security_enabled}
          onChange={(v) => updateSettings({ security_enabled: v })}
          label="启用安全审计"
          description="关闭后所有安全检测停止，请求直接放行"
        />

        {/* 策略模式 */}
        <div className="pt-3">
          <Select
            label="策略模式"
            options={MODE_OPTIONS}
            value={settings.security_mode}
            onChange={(e) => updateSettings({ security_mode: e.target.value })}
            hint={MODE_HINTS[settings.security_mode] || ""}
          />
        </div>

        {/* 策略矩阵速查 */}
        <div className="pt-3">
          <p className="text-xs font-medium text-text-secondary mb-2">策略矩阵</p>
          <div className="overflow-x-auto">
            <table className="w-full text-xs border border-border-primary rounded-lg overflow-hidden">
              <thead>
                <tr className="bg-bg-tertiary text-text-secondary">
                  <th className="px-2 py-1.5 text-left">Mode</th>
                  <th className="px-2 py-1.5 text-center">Clean/Low</th>
                  <th className="px-2 py-1.5 text-center">Medium</th>
                  <th className="px-2 py-1.5 text-center">High</th>
                  <th className="px-2 py-1.5 text-center">Critical</th>
                </tr>
              </thead>
              <tbody className="text-text-primary">
                {["audit", "warn", "redact", "block"].map((mode) => (
                  <tr key={mode} className={`border-t border-border-primary/50 ${settings.security_mode === mode ? "bg-accent/10" : ""}`}>
                    <td className="px-2 py-1.5 font-mono">{mode}</td>
                    <td className="px-2 py-1.5 text-center text-green-400">Allow</td>
                    <td className="px-2 py-1.5 text-center">
                      {mode === "warn" ? (
                        <span className="text-amber-400">Warn</span>
                      ) : (
                        <span className="text-green-400">Allow</span>
                      )}
                    </td>
                    <td className="px-2 py-1.5 text-center">
                      {mode === "block" ? (
                        <span className="text-red-400">Block</span>
                      ) : mode === "redact" ? (
                        <span className="text-amber-400">Redact</span>
                      ) : mode === "warn" ? (
                        <span className="text-amber-400">Warn</span>
                      ) : (
                        <span className="text-green-400">Allow</span>
                      )}
                    </td>
                    <td className="px-2 py-1.5 text-center">
                      {settings.security_block_on_critical ? (
                        <span className="text-red-400">Block*</span>
                      ) : (
                        <span className="text-red-400">
                          {mode === "block" ? "Block" : mode === "redact" ? "Redact" : mode === "warn" ? "Warn" : "Allow"}
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {settings.security_block_on_critical && (
            <p className="text-xs text-amber-400 mt-1">* block_on_critical 已开启，Critical 在任何模式下均强制阻断</p>
          )}
          <p className="text-xs text-text-muted mt-1">当前模式高亮为蓝色</p>
        </div>
      </div>

      {/* 扫描范围 */}
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">扫描范围</h4>
        <p className="text-xs text-text-muted mb-4">控制对请求和响应的哪些维度进行检测</p>

        <div className="space-y-4 bg-bg-tertiary/50 rounded-lg p-4 divide-y divide-border-primary/50">
          <Toggle
            checked={settings.security_scan_request}
            onChange={(v) => updateSettings({ security_scan_request: v })}
            label="扫描请求"
            description="对入站请求体进行安全扫描"
          />
          <Toggle
            checked={settings.security_scan_response}
            onChange={(v) => updateSettings({ security_scan_response: v })}
            label="扫描响应"
            description="对上游 LLM 返回的响应体进行安全扫描"
          />
          <Toggle
            checked={settings.security_scan_tools}
            onChange={(v) => updateSettings({ security_scan_tools: v })}
            label="工具/命令检测"
            description="检测 curl/wget/bash -c 等命令行特征，以及敏感文件+网络外发的组合行为"
          />
          <Toggle
            checked={settings.security_scan_network}
            onChange={(v) => updateSettings({ security_scan_network: v })}
            label="网络风险检测"
            description="检测公网 IP 探测服务、webhook/ngrok/pastebin 等可疑域名"
          />
          <Toggle
            checked={settings.security_scan_unicode}
            onChange={(v) => updateSettings({ security_scan_unicode: v })}
            label="Unicode 隐写检测"
            description="检测零宽字符、Bidi 方向控制、变体选择符（Trojan Source 攻击）"
          />
        </div>
      </div>

      {/* 兜底策略 */}
      <div>
        <h4 className="text-sm font-semibold text-text-primary mb-1">兜底策略</h4>
        <p className="text-xs text-text-muted mb-4">覆盖所有模式的全局安全策略</p>

        <div className="space-y-4 bg-bg-tertiary/50 rounded-lg p-4 divide-y divide-border-primary/50">
          <Toggle
            checked={settings.security_redact_secrets}
            onChange={(v) => updateSettings({ security_redact_secrets: v })}
            label="强制脱敏"
            description="任何模式下检测到密钥/Token 时，自动替换为 [REDACTED]"
          />
          <Toggle
            checked={settings.security_block_on_critical}
            onChange={(v) => updateSettings({ security_block_on_critical: v })}
            label="Critical 强制阻断"
            description="私钥泄露、数据外传命令等严重风险无视模式设置直接阻断"
          />
        </div>
      </div>
    </div>
  );
}
