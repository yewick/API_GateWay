# 前端待办（知识库「预览 → 确认入库」+ MinerU 配置）

> 本清单对应后端已完成的改动，本次**不做前端**。后端已落地：
>
> - MinerU 云服务 PDF 解析（Agent 轻量 API 无 token / Precise API 需 token，见 `YEAPI_MINERU_*` 环境变量）。
> - 所有文档类型统一「解析 → 预览 → 确认入库」两段式流程，上传走后台解析 + 真实进度条（`kb_tasks` 表存进度）。
> - 新接口 / 事件（前端对接依据）：
>
>   | 方法 | 路径 | 说明 |
>   |---|---|---|
>   | POST | `/api/kb/{id}/documents` | 上传（`{filename, content(base64)}`）→ 立即返回 `{document, task_id}`，状态 `parsing` |
>   | GET  | `/api/kb/{id}/documents/{doc_id}` | 单文档 + 最新解析任务 `{document, task}` |
>   | GET  | `/api/kb/{id}/documents/{doc_id}/content` | 导出解析后的 md/纯文本（预览用） |
>   | POST | `/api/kb/{id}/documents/{doc_id}/ingest` | 确认入库（分块→向量化→索引→`ready`） |
>   | GET  | `/api/kb/{id}/documents` | 文档列表（含 `status`） |
>   | DELETE | `/api/kb/{id}/documents/{doc_id}` | 删除文档 |
>   | GET  | `/api/kb/{id}/stats` | 知识库统计 |
>   | GET/POST | `/api/kb/{id}/index` | 索引状态 / 重建 |
>   | GET  | `/api/kb/{id}/search?query=...&top_k=...` | FTS5 关键词搜索 |
>
>   事件（`@tauri-apps/api/event` 的 `listen`）：`document-progress` `{kb_id, doc_id, task_id, stage, progress, done, total}`、
>   `document-parsed` `{kb_id, doc_id, status}`、`document-failed` `{kb_id, doc_id, status, error}`、`document-processed` `{kb_id, doc_id, status, chunk_count}`。
>
>   文档状态机：`parsing` →（解析成功）`awaiting_review` →（确认入库）`processing` → `ready`；任一步失败 → `failed`。

## 1. 数据通道选型（先决策，影响后续所有项）

前端当前**只走 Tauri `invoke`**（`src/lib/api.ts` → `src/lib/invoke-adapter.ts`），而知识库后端是 **HTTP `/api/kb*`**
（注册在 `src-tauri` 的 axum 服务里，没有对应 Tauri command）。二选一：

- **方案 A：新增 KB Tauri command，与现有模式一致。**
  在 `src-tauri` 加 `#[tauri::command]`（包装 `KbRepository` / `processor`），前端继续 `invoke<...>("kb_xxx", { ... })`，
  沿用 `invoke-adapter.ts` 的 `REAL_COMMANDS` + `mock-data.ts` 浏览器回退、`hooks/useKnowledgeBases.ts`（仿 `useChannels.ts`）。
  优点：与全站数据层一致、可 mock；缺点：要写一层 command 后端（但逻辑已都在 repository/processor 里，纯转发）。

- **方案 B：新增 fetch 客户端，直连 HTTP。**
  参考 `src/components/settings/TestConsoleTab.tsx:45` 的 `http://${settings.server_host}:${settings.server_port}`，
  新建 `src/lib/kb-api.ts` 用 `fetch` 调 `/api/kb/...`。优点：零新增后端、接口已就绪；缺点：无 mock、与现有 `invoke` 模式不一致。

> 建议：**MVP 用方案 B**（最快打通），同时把 KB 类型放进 `src/types/index.ts`；后续若要统一走 `invoke` 再补 command。

## 2. KB 基础 UI（当前前端无任何 KB 界面，从零搭）

- `src/App.tsx`（路由块约 30–37 行）：加 `<Route path="/kb" element={<KbPage />} />`。
- `src/lib/constants.ts`（`NAV_ITEMS`，约 95–101 行）：加 `{ path: "/kb", label: "知识库", icon: "BookOpen" }`。
- `src/types/index.ts`：新增 `KnowledgeBase`、`KbDocument`、`KbTask` 等类型（对齐 `src-tauri/.../knowledge/models.rs` 的 JSON 字段）。
- `src/lib/api.ts`（或方案 B 的 `src/lib/kb-api.ts`）：`listKbs / createKb / updateKb / deleteKb / listDocuments / uploadDocument / getDocument / getDocumentContent / ingestDocument / deleteDocument / getStats / buildIndex / searchFts`。
- `src/hooks/useKnowledgeBases.ts`、`src/hooks/useDocuments.ts`：仿 `useChannels.ts`（`@tanstack/react-query` + `refetchInterval`）。
- `src/pages/KbPage.tsx` + `src/components/kb/`（列表 / 详情 / 文档列表）。

## 3. 上传 + 进度条

- 选文件 → base64 → `POST /api/kb/{id}/documents`（`{filename, content}`）→ 拿 `{document, task_id}`。
- 进度来源二选一：
  - **轮询** `GET /api/kb/{id}/documents/{doc_id}` 读 `task.progress / total_items / done_items`（模板 `useDashboard.ts:12` 的 `refetchInterval`）；或
  - **事件** `listen("document-progress")` / `document-parsed` / `document-failed`（`@tauri-apps/api/event`）。
- 渲染：
  - `progress` 为 0–100 且 `total>0`（Precise 有页数）→ **真实进度条**（新建 `components/ui/Progress.tsx`）。
  - `total=0`（Agent / 本地后端无页数）→ **不定进度**（复用 `components/ui/Spinner.tsx`，文案显示 `stage`：提交/上传/解析/下载）。
  - `document-parsed` → 状态 `awaiting_review`；`document-failed` → 状态 `failed` + 展示 `error`。

## 4. 预览 + 确认入库

- 文档列表按 `status` 展示徽标：`parsing`（解析中）/ `awaiting_review`（待确认）/ `ready`（已入库）/ `failed`（失败，含 `error_message`）。
- 「预览」：`GET /api/kb/{id}/documents/{doc_id}/content` 拉 md/纯文本，用 Markdown 渲染组件展示。
  **需引入 `react-markdown`（`package.json` 现无）**；代码文件可用已有的 `react-syntax-highlighter`。
- 「入库」按钮：`POST /api/kb/{id}/documents/{doc_id}/ingest` → 成功后状态 `ready`，刷新 `stats`。
- 「删除」按钮：复用现有 `DELETE /api/kb/{id}/documents/{doc_id}`——**预览不满意时的主动出口**。
- **确认是用户的主动选择，而非被动等过期**：预览后由用户明确三选一——「入库 / 删除 / 重新上传」。
  效果不好 → 应能一键删除或重传；`awaiting_review` 只是临时态，自动清理（第 6 节）只是兜底，**绝不是**唯一出路。

## 5. MinerU 配置 UI

- 当前 token/base_url/model 走环境变量 `YEAPI_MINERU_TOKEN` / `YEAPI_MINERU_BASE_URL` / `YEAPI_MINERU_MODEL`（后端 `MinerUConfig::resolve()` 是唯一扩展点）。
- 设置页（`pages/SettingsPage.tsx` 或新建 `components/settings/MinerUConfigTab.tsx`）加：token、base_url、model_version 三项。
- 后续若要接入，需后端把这三项从环境变量改为可持久化配置（DB / store），前端再读写；本次只做占位说明。

## 6. 自动清理 / 过期机制（待确认，下次实现）

**目标**：`parsing` / `awaiting_review` 长期无人处理的文档自动删除，避免垃圾数据堆积。

**后端**：
- 清理范围：只清 `awaiting_review`（和卡死的 `parsing`，如任务超时仍无结果）——这些文档**从未计入** `doc_count`，删除**无需回减计数**；`failed` 不自动清（用户要看错误原因）。
- 过期判断：复用 `updated_at`（进入 `awaiting_review` 后不再变动）作「待确认起始时间」，无需新增列；若要显式截止时间，再加 migration 新增 `expires_at` 列。
- 触发方式二选一：
  - 惰性清理：`list_documents` / `get_knowledge_base` 时顺带执行 `DELETE FROM kb_documents WHERE status IN ('parsing','awaiting_review') AND updated_at < now - N 天`（外键 `ON DELETE CASCADE` 已级联删 chunks/tasks）。
  - 定时任务：`tauri::async_runtime::spawn` 一个 interval（如每天）跑同一段 DELETE。
- 阈值：加配置项（如 `YEAPI_KB_REVIEW_TTL_DAYS`，默认 7 天）。

**前端**：
- 文档列表对 `awaiting_review` 显示「待确认，N 天后自动清理」徽标 / 倒计时。
- 「待确认」列表页 + 批量删除按钮；删除走现有 `DELETE .../documents/{doc_id}`。
- 定位：自动清理是**兜底**；正常路径是用户在确认步**主动**「入库 / 删除 / 重新上传」（见第 4 节），不鼓励「放着等过期」。

