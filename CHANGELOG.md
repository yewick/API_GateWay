# Changelog

本项目所有值得注意的变更均记录在此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

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
