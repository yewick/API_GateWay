# Changelog

本项目所有值得注意的变更均记录在此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- **客户端接入页改版**：参考 cc-switch 的列表式布局，页面顶部展示「当前接入」与「路由服务 · 运行中」状态条；接入目标拆为 Codex CLI、Claude Code CLI、Claude Desktop 三张卡片，支持搜索、测试连接、编辑、立即修复、复制网关地址与打开配置目录。编辑改为弹窗，已有接入默认复用原密钥；Claude 的 CLI 与桌面端可分别断开，互不影响另一个目标。选择已有密钥时，模型下拉只显示该密钥允许的模型；打开编辑时不再把已下线的旧模型预填为自定义模型，选择不可用模型会直接提示并禁用接入。
- **Claude Fable 档位**：Claude Code CLI 增加 `ANTHROPIC_DEFAULT_FABLE_MODEL` / `_NAME` 映射，`/model` 切换到 Fable 时会使用配置的真实模型；桌面端 `claude-fable-5` 路由也改为优先使用 Fable 映射（留空跟随主模型）。旧接入记录缺少该字段时自动回退主模型。
- **客户端接入启动自检（开箱即用）**：应用启动、网关重启与渠道变更后自动检查客户端配置，只在「配置仍由 YeAPI 持有」时幂等修复——端口/地址变化自动改写 Codex、Claude Code 与桌面端的 base_url，Codex 模型目录缺失或过期时自动重建，向导新建的托管密钥丢失/过期时自动轮换并重写配置；在密钥页主动删除的 Key 会留下墓碑，不会被自动复活。检测到 cc-switch 等外部工具改写配置时只提示，不自动覆盖，需点「重新接入」手动接管（先备份）。
- **网关状态可见**：新增 `server_status` 命令与 `server-status-changed` 事件；绑定失败不再静默，页面顶部会显示错误与「重试」。网关改为先完成绑定再启动、优雅停止后重启，并加入单实例插件避免双开抢端口。
- **常驻提示**：首次接入后提示一次「关闭到托盘 / 开机自启」，默认不勾选、可在设置中调整；新增「设置 → 通用设置 → 客户端接入 → 启动时自动修复」开关。
- **客户端接入向导（Codex）**：新增「客户端接入」页面，可新建 / 选择 YeAPI 密钥并一键写入 `~/.codex/config.toml`（专用 `yeapi` provider、`responses` 协议、`experimental_bearer_token`），无需登录 OpenAI 账号。支持配置预览、连接测试、断开恢复与首次写入前备份；不修改 `auth.json`，并按本机 ChatGPT 登录态自动设置 `requires_openai_auth`。接入前校验模型是否命中启用渠道，并生成 `yeapi-model-catalog.json` 供 Codex 模型菜单使用。
- **客户端接入向导（Claude Code）**：在「客户端接入」页新增 Claude Code 标签，可同时接入 Claude Code CLI 与 Claude 桌面端。CLI 侧增量维护 `~/.claude/settings.json` 的 `env`（`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_BASE_URL`、`ANTHROPIC_MODEL` 及 Opus / Sonnet / Haiku 别名），保留其余设置与键序；桌面端侧新建并切换名为 `YeAPI` 的第三方推理档案，断开时恢复原档案。支持配置预览、Anthropic `/v1/messages` 连接测试、断开恢复与首次写入前备份。

### Fixed

- 修复安全扫描误报：`tool.remote_script_exec` 改为检测真正的 `curl|wget … | bash/sh` 管道执行（排除 `||`），`network.tracking_pixel` 只匹配真实 1x1 图片 / 追踪 URL，`noreply@` 等系统邮箱不计入个人信息，Info 级重复发现每个规则最多保留 3 条；`off` / `audit` 模式不再被 `block_on_critical` 或自定义阻断规则升级为阻断。
- 修复凭证检测漏报：密钥样式字符串紧跟在中文等非分隔文本之后（如「我的apikey是sk-…」）时不会被识别；改为边界感知的全文扫描，覆盖 `sk-` / `sk-ant-` / `Bearer` / JWT 等前缀。
- 修复限额密钥在 Claude Code 等客户端下因单次 `max_tokens` 较大被误判为配额不足的问题：配额预检改为「估算请求输入 + 输出预算封顶」，把上游 `max_tokens` 收敛到剩余预算，仅在余额不足以覆盖输入与最小输出时返回 429；同时为 `/v1/messages`、`/v1/responses` 的拒绝路径补写失败日志。
- 修复 `/v1/responses` 流式事件不符合 Responses 规范导致 Codex 丢弃文本 / 工具调用的问题：补齐 `response.output_item.added/done`、`response.content_part.added/done`、`response.output_text.done`、`response.function_call_arguments.done`，并在 `response.completed` 中回填完整文本与工具调用参数。
- 修复 Responses 请求的 `input` 为字符串、或消息项省略 `type` 时被丢弃、上游报 `Empty input messages` 的问题。
- 修复连续 / 并行 `function_call` 被拆成多条 assistant 消息、导致上游报 `tool_calls must be followed by tool messages` 的问题：连续的 `function_call` 现在合并进同一条 assistant 消息。
- 修复 DeepSeek thinking 模型在工具调用轮次报 `reasoning_content must be passed back` 的问题：历史中存在 `tool_calls` 时，DeepSeek 适配器会对本次请求关闭 thinking；纯对话轮次不受影响。
- 修复无 `call_id` 或对不上当前 `tool_calls` 的 `function_call_output`（例如 Codex app 工具输出）被写成孤立 `tool` 消息、上游报 `role 'tool' must be a response to a preceding message with 'tool_calls'` 的问题：无法关联的输出改为带标签的 user 文本。
- 修复流式响应在等待上游首包期间不发送任何字节、导致客户端约 30 秒流超时（`stream disconnected before completion: error sending request`）的问题：新增立即发送和每 10 秒一次的 SSE keep-alive，并让流式上游请求不复用空闲连接。

## [0.1.6] - 2026-09-01

### Added

- **父子块（Parent/Child）检索**：RAG 检索时自动补全父块上下文，并新增对应测试脚本。
- 知识库入库失败可重试；索引自动构建并回写失败状态。

### Fixed

- 修复关键词检索失败与问答卡死。
- 修复索引自动创建相关问题。

### Changed

- 文档：新增 macOS 首次打开 Gatekeeper 绕过说明（终端方式）。

## [0.1.5] - 2026-08-28

自 `v0.1.0` 以来的重大版本：新增知识库 / RAG、MCP server 与新协议层。

### Added

- **知识库 / RAG 子系统**（`src-tauri/src/services/knowledge/`）
  - 多格式文档解析：txt / md / 代码 / PDF / docx / xlsx / pptx / csv / html。
  - 文档切块、向量化（embedding）与 HNSW 索引构建。
  - 知识库检索与 RAG 问答。
  - MinerU 云解析（agent 与 precise 两种模式）。
  - 对应的数据库迁移（`010_knowledge_base.sql` ~ `017_knowledge_base_fts_bigram.sql`）与前端知识库页面。
- **MCP server**（`src-tauri/src/services/mcp/`）：后端 MCP 服务与前端工具展示 / 测试台。
- **新协议层**（`src-tauri/src/protocol/`）：兼容 Claude / Gemini 风格的「new tunnel」接口（`anthropic.rs`、`responses.rs`）。
- **服务注册框架**（service registration）。
- **前端增强**：Dashboard 模式分布与配额环、日志筛选、设置页、渠道页重构、知识库交互页面与 hooks、Markdown/Slider/Table 组件。

### Changed

- 后端索引框架完善（index frame）。
- 检索 margin 调整。
- 移除本地开发工具目录（Claude skills 与 VS Code 配置）。

### Fixed

- 修复 request_logs 占位符 off-by-one 导致的仪表盘 / 日志 / 用量不更新。
- 修复 RAG 文档插入、MinerU 选路、KB / RAG 日志与 UI 问题。
- 修复 git 代码导入、RAG OAuth token、rag+embed token 用量日志。

## [0.1.0] - 2026-08-17

首个发布版本，提供 LLM API Gateway 基础能力（多供应商代理、路由、负载均衡、审计）。
