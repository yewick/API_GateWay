// ===== Invoke 适配器 =====
// 在 Tauri invoke 与 Mock 数据之间分发命令调用。
//
// 架构：
//   - REAL_COMMANDS 集合中的命令 → 真实 Tauri 后端
//   - 其余命令 → mock-data.ts 中的内存数据
//   - 浏览器开发模式（无 window.__TAURI_INTERNALS__）→ 全部走 Mock
//
// 当后端某个命令上线后，将其名称添加到 REAL_COMMANDS，即可从 Mock 无缝切换到真实后端。

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { mockHandlers, transformChannelFromBackend } from "./mock-data";

// 当前已注册到 Rust 后端的真实命令
export const REAL_COMMANDS = new Set<string>([
  "greet",
  "get_channels",
  "create_channel",
  "test_channel",
]);

// 环境检测：是否运行在 Tauri WebView 中
const isTauri = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

// ---------- 转换层 ----------
// Rust 后端的 Channel 将 models/config/model_mapping 以 JSON 字符串返回，
// 这里将其转换为前端期望的类型化结构。反向（前端 → 后端）在 create_channel 时处理。
const transformResponse = <T>(cmd: string, data: T): T => {
  if (cmd === "get_channels") {
    return (Array.isArray(data) ? data.map(transformChannelFromBackend) : data) as T;
  }
  if (cmd === "create_channel") {
    return transformChannelFromBackend(data) as T;
  }
  // Rust 后端 TestResult 使用 message 字段，前端类型为 error_message，此处做字段映射
  if (cmd === "test_channel" && data && typeof data === "object") {
    const d = data as Record<string, unknown>;
    if ("message" in d && !("error_message" in d)) {
      const { message, ...rest } = d;
      return { ...rest, error_message: message } as T;
    }
  }
  return data;
};

// ---------- 序列化层 ----------
// 前端类型与后端 CreateChannelInput 已对齐（models 为数组、config/model_mapping 为对象），
// 保留此钩子以便未来字段结构变化时在此处转换。
const serializeRequest = (_cmd: string, args: any): any => args;

export const addRealCommand = (cmd: string) => {
  REAL_COMMANDS.add(cmd);
};

export const removeRealCommand = (cmd: string) => {
  REAL_COMMANDS.delete(cmd);
};

export const invoke = <T>(cmd: string, args?: unknown): Promise<T> => {
  // 浏览器模式：所有命令走 Mock
  if (!isTauri()) {
    const handler = mockHandlers[cmd];
    if (!handler) {
      console.warn(`[invoke-adapter] 未知命令 ${cmd}，无 Mock 处理器，返回 null`);
      return Promise.resolve(null as T);
    }
    return handler(args);
  }

  // Tauri 模式：真实命令走后端，其余走 Mock
  if (REAL_COMMANDS.has(cmd)) {
    return tauriInvoke<T>(cmd, serializeRequest(cmd, args)).then((data) =>
      transformResponse<T>(cmd, data),
    );
  }

  const handler = mockHandlers[cmd];
  if (!handler) {
    console.warn(`[invoke-adapter] 命令 ${cmd} 未注册到后端，且无 Mock 处理器`);
    return Promise.resolve(null as T);
  }
  return handler(args);
};

export { tauriInvoke };
