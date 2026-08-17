# YeAPI

> 本地 LLM API 网关 —— 一个跑在桌面上的聚合网关，把多家大模型渠道收拢成**一个 OpenAI 兼容的本地 API 入口**，自带负载均衡、密钥管理、安全审计与数据脱敏。

YeAPI 是一个基于 [Tauri v2](https://tauri.app) 的桌面应用：前端用 React 管理配置，后端用 Rust 在本地起一个 HTTP 服务（默认 `127.0.0.1:8777`）。你在界面里配好渠道和密钥后，任何兼容 OpenAI 协议的客户端（ChatBox、Cherry Studio、代码编辑器插件等）把 `base_url` 指向它，就能统一调度你所有的大模型账号。

---

## 下载安装

> 各平台安装包（macOS / Windows / Linux）请到 [Releases 最新版](https://github.com/yewick/API_GateWay/releases/latest) 下载。

[![GitHub release](https://img.shields.io/github/v/release/yewick/API_GateWay)](https://github.com/yewick/API_GateWay/releases/latest)

## 功能特性

- **多渠道聚合**：OpenAI / DeepSeek / Claude / Gemini / 通义千问 / 智谱 GLM / Kimi / 豆包 / Ollama / 自定义（OpenAI 兼容），统一适配、统一入口。
- **智能调度**：按**优先级 + 组内权重**构建故障转移队列，主渠道失败自动降级到下一个，直到成功或耗尽。
- **密钥管理**：面向下游签发 API Key，支持配额（token 上限）、过期时间、允许模型/渠道白名单。
- **安全审计引擎**：请求/响应双向扫描，检测凭证泄露、Unicode 隐写、工具/网络风险；支持审计、告警、脱敏、阻断四种策略，命中高危可返回 `451`。
- **数据脱敏**：转发前自动打码疑似密钥（`sk-…`、`AKIA…`、GitHub Token 等）。
- **请求日志**：每次请求全量留痕，含 token 用量、耗时、命中渠道、风险等级与安全发现。
- **桌面能力**：系统托盘、关闭/最小化到托盘、开机自启、多主题（深色 / 浅色 / 墨纸 / 跟随系统）。
- **跨平台**：macOS / Windows / Linux 三平台构建，GitHub Actions 打 tag 自动发版。

---

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri v2（Rust + WebView） |
| 前端 | React 19 · TypeScript · Vite 7 · Tailwind CSS 4 · Zustand · TanStack Query · react-router-dom 7 |
| 本地 HTTP 服务 | Axum 0.8 · tokio · tower-http（CORS） |
| 上游请求 | reqwest（支持流式 SSE） |
| 数据存储 | SQLite（sqlx） · tauri-plugin-store（配置） |
| 桌面插件 | store / sql / autostart / dialog / notification / clipboard / opener / shell |

---

## 快速开始

### 环境要求

- **Node.js** ≥ 20.19（Vite 7 要求），推荐 22+
- **Rust** stable（Tauri v2 要求）
- **平台依赖**（仅 Linux 需要）：`libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`libssl-dev`、`libsqlite3-dev` 等（CI 里已配好）

### 安装依赖

```bash
npm install
```

### 开发调试

```bash
npm run tauri dev
```

前端 Vite 跑在 `http://localhost:1420`，Rust 后端随窗口启动，本地 HTTP 服务默认监听 `127.0.0.1:8777`。

### 构建打包

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`（`.app` / `.dmg` / `.msi` / `.exe` / `.deb` 等）。

---

## 使用指南

### 1. 添加渠道

进入「渠道」页，点「新增渠道」：

- **类型**：选择厂商（OpenAI、DeepSeek、Claude、Gemini、通义千问……或「自定义」填 OpenAI 兼容地址）。
- **Base URL** 与 **API Key**：填入上游账号信息。
- **模型**：该渠道支持的模型列表；留空表示支持所有模型。
- **优先级 / 权重**：优先级高的渠道先被尝试；同优先级内按权重随机分配流量。
- **模型映射**：可选，把对外暴露的模型名映射到上游真实模型名（如 `gpt-4o → gpt-4o-2024-11-20`）。

保存后可点「测试」验证连通性。

### 2. 创建密钥

进入「密钥」页，为下游客户端签发一个 API Key，可设置：

- **允许模型 / 允许渠道**：白名单。
- **配额**：token 总量上限，`0` 表示不限。
- **过期时间**：到期后自动拒绝（401）。

### 3. 调用网关

把客户端的 `base_url` 指向 `http://127.0.0.1:8777/v1`，API Key 填你在「密钥」页创建的那把：

```bash
# 健康检查
curl http://127.0.0.1:8777/health

# 非流式对话
curl http://127.0.0.1:8777/v1/chat/completions \
  -H "Authorization: Bearer <你的密钥>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","messages":[{"role":"user","content":"你好"}]}'

# 流式对话（SSE）
curl -N http://127.0.0.1:8777/v1/chat/completions \
  -H "Authorization: Bearer <你的密钥>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","stream":true,"messages":[{"role":"user","content":"你好"}]}'

# 列出可用模型
curl http://127.0.0.1:8777/v1/models \
  -H "Authorization: Bearer <你的密钥>"
```

模型名以「渠道声明的模型 + 模型映射的 key」为准，可用 `/v1/models` 查。

### 4. 查看日志与用量

每次请求都会在「日志」页留痕，可看到命中渠道、token 用量、耗时、风险等级与安全发现；「用量」页提供按日统计；「仪表盘」汇总今日请求量 / token / 活跃渠道 / 平均延迟。

---

## HTTP API

网关暴露 **OpenAI 兼容** 接口：

| 方法 | 路径 | 状态 | 说明 |
|---|---|---|---|
| `GET` | `/health` | ✅ | 健康检查，返回运行状态 / 端口 / 地址 |
| `GET` | `/v1/models` | ✅ | 列出所有启用渠道支持的模型 |
| `POST` | `/v1/chat/completions` | ✅ | 对话补全，支持流式与非流式 |
| `POST` | `/v1/completions` | ⏳ | 占位，未实现 |
| `POST` | `/v1/embeddings` | ⏳ | 占位，未实现 |
| `POST` | `/v1/images/generations` | ⏳ | 占位，未实现 |
| `POST` | `/v1/audio/transcriptions` | ⏳ | 占位，未实现 |
| `POST` | `/v1/audio/speech` | ⏳ | 占位，未实现 |

鉴权方式：`Authorization: Bearer <密钥>`。

---

## 核心概念

### 渠道（Channel）

一个上游大模型账号的封装。字段：类型、Base URL、API Key、支持的模型、优先级、权重、模型映射、状态（启用/禁用）。

### 适配器（Adaptor）

策略模式封装各厂商的协议差异，统一转成内部 `ProxyRequest`。内置 `openai` / `claude` / `gemini` / `deepseek` / `custom` 五个实现；其余 OpenAI 兼容厂商（千问、智谱、Kimi、豆包、Ollama）走 `custom` 兜底。

### 调度与故障转移

```
请求模型 gpt-4o
   │
   ↓ 过滤：仅启用 且 支持该模型 的渠道
┌──────────────────────────────────────────┐
│ 优先级 10：[OpenAI-A(w:5), OpenAI-B(w:3)] │  ← 先试这组，组内按 5:3 权重随机
│ 优先级 5 ：[DeepSeek(w:1)]                │  ← 上面全失败再试
│ 优先级 1 ：[Zhipu(w:1)]                   │  ← 最后兜底
└──────────────────────────────────────────┘
   │
   ↓ 生成有序故障转移队列
[OpenAI-A/B(随机序), DeepSeek, Zhipu]
   │
   ↓ 按顺序尝试，失败(非 2xx/连接错误)则记日志并继续，直到成功或耗尽
```

重试次数由「设置 → 重试」控制（默认启用，最多 3 次），全部失败返回 `502`。

---

## 安全引擎

每个请求在**鉴权后、转发前**过一遍安全扫描，可选的响应侧也会扫描。

### 风险等级（RiskLevel）

| 等级 | 示例 |
|---|---|
| `clean` 无风险 | — |
| `info` 提示 | 外部 URL |
| `low` 低 | 邮箱、手机号 |
| `medium` 中 | 本地路径、零宽字符 |
| `high` 高 | 密钥、敏感路径、IP 探测 |
| `critical` 严重 | 私钥、数据外传命令 |

### 扫描维度

- **凭证泄露**：`sk-` / `AKIA` / GitHub Token / 数据库连接串等。
- **Unicode 隐写**：零宽字符、双向控制符、变体选择符、同形异义字。
- **网络风险**：IP 探测服务、外传地址。
- **工具/命令风险**：危险命令与工具调用。

### 安全模式（`security.mode`）

| 模式 | 行为 |
|---|---|
| `off` / `audit` | 仅审计记录，全部放行（默认） |
| `warn` | `medium` 及以上标记告警，仍放行 |
| `redact` | `high` 及以上先脱敏再转发 |
| `block` | `high` 及以上直接阻断，返回 `451` |

另有兜底开关：`block_on_critical` 在任何模式下都把 `critical` 强制阻断；**自定义规则**的 `action=block` 也会无条件阻断。

### 规则

- **内置规则**：随应用内置，可单独启用/禁用或调整严重级别。
- **自定义规则**：正则/关键词匹配，可指定严重级别与动作（`warn` / `block`），支持黑名单与白名单豁免。

### 数据脱敏

开启后，转发前会把疑似密钥替换为打码形式（如 `sk-abc****wxyz`），并在日志里保留 `forward_body` 快照以供审计。

---

## 设置项

配置持久化在 Tauri Store（`settings.json`），通过「设置」页修改：

| 分组 | 键 | 默认 | 说明 |
|---|---|---|---|
| 服务器 | `server.host` | `127.0.0.1` | 本地 HTTP 服务监听地址 |
| 服务器 | `server.port` | `8777` | 监听端口（修改后自动重启服务） |
| 界面 | `ui.theme` | `dark` | 主题：`dark` / `light` / `paper` / `system` |
| 界面 | `ui.language` | `zh-CN` | 语言 |
| 界面 | `ui.minimize_to_tray` | `true` | 最小化隐藏到托盘 |
| 界面 | `ui.close_to_tray` | `false` | 关闭按钮隐藏到托盘 |
| 界面 | `ui.auto_start` | `false` | 开机自启 |
| 重试 | `retry.enabled` | `true` | 启用故障转移重试 |
| 重试 | `retry.times` | `3` | 最大重试次数 |
| 安全 | `security.enabled` / `security.mode` / `security.scan_*` / `security.redact_secrets` / `security.block_on_critical` | 见上表 | 安全引擎总开关与各维度开关 |

---

## 桌面能力

- **系统托盘**：左键点击恢复窗口；右键菜单提供「显示 / 退出」。
- **关闭/最小化到托盘**：关闭或最小化窗口时隐藏到托盘而非退出（可在设置中开关）。
- **开机自启**：macOS 走 LaunchAgent。
- **macOS Dock 点击恢复**：应用在托盘时点 Dock 图标可唤回窗口。

---

## 项目结构

```
ye_api/
├── src/                        # 前端（React + TypeScript）
│   ├── components/             # 组件（channels / api-keys / logs / dashboard / settings / common / layout / ui）
│   ├── hooks/                  # 自定义 Hooks（useTheme 等）
│   ├── lib/                    # api.ts 命令层 / constants.ts 常量 / invoke-adapter.ts
│   ├── pages/                  # 页面（Dashboard / Usage / Channels / ApiKeys / Logs / Settings）
│   ├── stores/                 # Zustand（settingsStore / themeStore / uiStore）
│   ├── styles/  types/         # 全局样式 / 类型定义
│   └── App.tsx  main.tsx       # 路由与入口
│
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs              # Tauri Builder、命令注册、AppState、托盘初始化
│   │   ├── main.rs             # 入口（调用 lib::run）
│   │   ├── adaptor/            # 渠道适配器（策略模式）
│   │   ├── commands/           # Tauri 命令层（前端调用入口）
│   │   ├── core/               # dispatcher 调度器 / proxy 转发代理
│   │   ├── db/                 # SQLite 连接 / 模型 / 仓库
│   │   ├── security/           # scanner 扫描 / decision 决策 / redact 脱敏 / rules 规则
│   │   ├── server/             # 本地 HTTP 服务（mod 启动 / router 路由 / handlers 处理）
│   │   ├── tray.rs             # 系统托盘 + 窗口事件
│   │   └── utils/              # id / time 工具
│   ├── migrations/             # SQLite 迁移脚本
│   ├── icons/                  # 全平台应用图标
│   ├── Cargo.toml
│   └── tauri.conf.json         # 应用配置（窗口 / 打包元数据）
│
├── .github/workflows/release.yml   # 三平台构建 + 自动发版
├── package.json
└── vite.config.ts
```

---

## 跨平台构建与发版

### 本地构建

```bash
npm run tauri build
```

> Rust 是编译型语言，本地只能打出当前平台的包。Windows / Linux 的安装包需在对应系统或 CI 上构建。

### CI 自动发版

已配置 `.github/workflows/release.yml`，触发方式：

1. **打 tag**：`git tag v0.1.0 && git push origin v0.1.0`
2. 或到 GitHub Actions 页面 **Run workflow** 手动触发。

矩阵并行构建 macOS（Apple Silicon + Intel）、Windows、Linux，产物自动挂到 Release 草稿（`releaseDraft: true`，人工确认后发布）。

发版前请同步三处版本号，保持一致：`src-tauri/tauri.conf.json`、`package.json`、`src-tauri/Cargo.toml`。

---

## 常见问题（FAQ）

**Q：macOS 打开应用提示「已损坏，无法打开」或「无法验证开发者」？**
当前 macOS 包为默认 ad-hoc 签名（未做 Apple 公证）。首次打开请绕过 Gatekeeper：

```bash
xattr -cr /Applications/YeAPI.app
```

或在「系统设置 → 隐私与安全性」中点击「仍要打开」。

**Q：修改了端口/主机地址，为什么没生效？**
保存服务器设置后，网关会**自动重启本地服务**，无需手动重启应用。稍等片刻再用新端口访问。

**Q：端口被占用了怎么办？**
在「设置 → 服务器」改一个空闲端口，保存后自动重启即可。

**Q：请求返回 451 是什么原因？**
说明被安全引擎阻断（命中高危/严重风险，或某条 `block` 规则）。到「日志」页查看该请求的风险摘要与具体发现项，可调整安全模式或对应规则。

**Q：客户端怎么连？**
把客户端的 `base_url` 设为 `http://127.0.0.1:8777/v1`，API Key 填在「密钥」页创建的那把即可。多数客户端需要自选「OpenAI 兼容」模式。

**Q：返回 502 说明什么？**
所有匹配渠道都失败了（非 2xx 或连接错误）。检查渠道是否启用、上游 API Key 是否有效、网络是否可达，日志里会有每次尝试的详细错误。
