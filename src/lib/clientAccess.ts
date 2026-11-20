import type { ClientIssue, ClientSetupStatus } from "../types";

export type TargetId = "codex" | "claude-cli" | "claude-desktop";

export type TargetState =
  | "applied"
  | "drift"
  | "conflict"
  | "not_applied"
  | "not_installed";

export interface TargetChip {
  label: string;
  tone: "neutral" | "info" | "success" | "warning" | "danger" | "outline";
}

export interface TargetView {
  id: TargetId;
  client: "codex" | "claude";
  name: string;
  description: string;
  installed: boolean;
  applied: boolean;
  state: TargetState;
  stateLabel: string;
  configPath: string;
  gatewayUrl: string;
  model: string | null;
  mapping: string | null;
  keyHint: string | null;
  keyValid: boolean;
  appliedAt: string | null;
  issues: ClientIssue[];
  chips: TargetChip[];
}

const hasIssue = (issues: ClientIssue[], code: string) =>
  issues.some((issue) => issue.code === code);

const stateChip = (state: TargetState): TargetChip => {
  switch (state) {
    case "applied":
      return { label: "已接入", tone: "success" };
    case "drift":
      return { label: "配置已变化", tone: "warning" };
    case "conflict":
      return { label: "需处理", tone: "danger" };
    case "not_installed":
      return { label: "未检测到", tone: "outline" };
    default:
      return { label: "未接入", tone: "outline" };
  }
};

const stateLabel = (state: TargetState): string => {
  switch (state) {
    case "applied":
      return "使用中";
    case "drift":
      return "配置已变化";
    case "conflict":
      return "需要接管";
    case "not_installed":
      return "未检测到";
    default:
      return "未接入";
  }
};

const cliIssues = (issues: ClientIssue[], cliApplied: boolean) =>
  issues.filter((issue) => {
    if (issue.code === "desktop_not_active") return false;
    if (issue.code === "external_conflict") return cliApplied;
    if (issue.code === "gateway_changed") return cliApplied;
    return true;
  });

const desktopIssues = (issues: ClientIssue[], detected: boolean) =>
  issues.filter((issue) => {
    if (!detected) return false;
    if (
      issue.code === "config_missing" ||
      issue.code === "parse_error" ||
      issue.code === "path_mismatch"
    ) {
      return false;
    }
    return true;
  });

function codexView(status?: ClientSetupStatus): TargetView {
  const issues = status?.issues ?? [];
  const applied = Boolean(status?.applied);
  const installed = Boolean(status?.installed);
  const state: TargetState = status?.external_conflict
    ? "conflict"
    : applied
      ? "applied"
      : status?.drift
        ? "drift"
        : installed
          ? "not_applied"
          : "not_installed";
  return {
    id: "codex",
    client: "codex",
    name: "Codex CLI",
    description: "写入 ~/.codex/config.toml，使用 Responses 协议",
    installed,
    applied,
    state,
    stateLabel: stateLabel(state),
    configPath: status?.config_path ?? "~/.codex/config.toml",
    gatewayUrl: status?.gateway_base_url ?? "",
    model: status?.current_model ?? null,
    mapping: null,
    keyHint: status?.applied_api_key_hint ?? null,
    keyValid: Boolean(status?.applied_api_key_valid),
    appliedAt: status?.applied_at ?? null,
    issues,
    chips: [
      { label: "CLI", tone: "outline" },
      { label: "Responses", tone: "outline" },
      stateChip(state),
    ],
  };
}

function claudeCliView(status?: ClientSetupStatus): TargetView {
  const detail = status?.claude;
  const cliApplied = Boolean(detail?.cli_applied);
  const issues = cliIssues(status?.issues ?? [], cliApplied);
  const applied = cliApplied && Boolean(status?.applied);
  const state: TargetState = !cliApplied
    ? status?.installed
      ? "not_applied"
      : "not_installed"
    : hasIssue(issues, "external_conflict")
      ? "conflict"
      : applied
        ? "applied"
        : status?.drift
          ? "drift"
          : "not_applied";
  const env = detail?.env_models;
  const mapping =
    env && (env.opus || env.sonnet || env.haiku || env.fable)
      ? [
          env.opus ? `Opus→${env.opus}` : null,
          env.sonnet ? `Sonnet→${env.sonnet}` : null,
          env.haiku ? `Haiku→${env.haiku}` : null,
          env.fable ? `Fable→${env.fable}` : null,
        ]
          .filter(Boolean)
          .join(" · ")
      : null;
  return {
    id: "claude-cli",
    client: "claude",
    name: "Claude Code CLI",
    description: "写入 ~/.claude/settings.json，使用 Messages 协议",
    installed: Boolean(status?.installed),
    applied,
    state,
    stateLabel: stateLabel(state),
    configPath: status?.config_path ?? "~/.claude/settings.json",
    gatewayUrl: status?.gateway_base_url ?? "",
    model: status?.current_model ?? null,
    mapping,
    keyHint: status?.applied_api_key_hint ?? null,
    keyValid: Boolean(status?.applied_api_key_valid),
    appliedAt: status?.applied_at ?? null,
    issues,
    chips: [
      { label: "CLI", tone: "outline" },
      { label: "Messages", tone: "outline" },
      stateChip(state),
    ],
  };
}

function claudeDesktopView(status?: ClientSetupStatus): TargetView {
  const detail = status?.claude?.desktop;
  const detected = Boolean(detail?.detected);
  const applied = Boolean(detail?.applied);
  const issues = desktopIssues(status?.issues ?? [], detected);
  const state: TargetState = !detected
    ? "not_installed"
    : hasIssue(issues, "desktop_not_active")
      ? "conflict"
      : applied
        ? "applied"
        : detail?.drift
          ? "drift"
          : "not_applied";
  const env = status?.claude?.env_models;
  const mapping =
    env && (env.opus || env.sonnet || env.haiku || env.fable)
      ? [
          env.opus ? `Opus→${env.opus}` : null,
          env.sonnet ? `Sonnet→${env.sonnet}` : null,
          env.haiku ? `Haiku→${env.haiku}` : null,
          env.fable ? `Fable→${env.fable}` : null,
        ]
          .filter(Boolean)
          .join(" · ")
      : null;
  return {
    id: "claude-desktop",
    client: "claude",
    name: "Claude Desktop",
    description: "写入桌面端第三方推理档案，使用模型映射",
    installed: detected,
    applied,
    state,
    stateLabel: stateLabel(state),
    configPath: detail?.meta_path ?? "未检测到第三方推理配置",
    gatewayUrl: detail?.applied_base_url ?? status?.gateway_base_url ?? "",
    model: null,
    mapping,
    keyHint: status?.applied_api_key_hint ?? null,
    keyValid: Boolean(status?.applied_api_key_valid),
    appliedAt: status?.applied_at ?? null,
    issues,
    chips: [
      { label: "桌面端", tone: "outline" },
      { label: "模型映射", tone: "info" },
      stateChip(state),
    ],
  };
}

export function buildTargetViews(
  codex?: ClientSetupStatus,
  claude?: ClientSetupStatus,
): TargetView[] {
  return [codexView(codex), claudeCliView(claude), claudeDesktopView(claude)];
}

export function matchesTarget(view: TargetView, query: string): boolean {
  const keyword = query.trim().toLowerCase();
  if (!keyword) return true;
  return [
    view.name,
    view.description,
    view.configPath,
    view.gatewayUrl,
    view.model ?? "",
    view.mapping ?? "",
    view.stateLabel,
    ...view.chips.map((chip) => chip.label),
  ]
    .join(" ")
    .toLowerCase()
    .includes(keyword);
}

export const highestSeverity = (issues: ClientIssue[]): "info" | "warning" | "danger" =>
  issues.some((issue) => issue.severity === "danger")
    ? "danger"
    : issues.some((issue) => issue.severity === "warning")
      ? "warning"
      : "info";
