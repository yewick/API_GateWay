# 前端工程化结构

```
src/
├── assets/                   # 静态资源
├── components/               # 公共组件
│   ├── layout/              # 布局组件
│   │   ├── Layout.tsx       # 整体布局
│   │   └── Sidebar.tsx      # 侧边栏导航
│   └── ui/                  # UI 基础组件
├── hooks/                    # 自定义 Hooks
├── lib/                      # 工具库
│   ├── api.ts               # API 调用层
│   └── constants.ts         # 常量定义
├── pages/                    # 页面视图
│   ├── DashboardPage.tsx    # 仪表盘
│   ├── ChannelsPage.tsx     # 渠道管理
│   ├── ApiKeysPage.tsx      # 密钥管理
│   ├── LogsPage.tsx         # 请求日志
│   ├── UsagePage.tsx        # 用量统计
│   └── SettingsPage.tsx     # 设置中心
├── styles/                   # 全局样式
├── types/                    # TypeScript 类型定义
│   └── index.ts             # 所有接口类型
├── App.tsx                   # 根组件（路由配置）
├── main.tsx                  # 入口文件
└── App.css                   # 应用样式
```

# 后端工程化结构
```
src-tauri/
├── src/
│   ├── lib.rs              # 库入口（Tauri Builder 配置）
│   ├── main.rs             # 主入口
│   ├── adaptor/            # 渠道适配器（策略模式）
│   │   ├── mod.rs          # Adaptor trait + 工厂函数
│   │   ├── openai.rs       # OpenAI 适配器
│   │   ├── claude.rs       # Claude 适配器
│   │   ├── gemini.rs       # Gemini 适配器
│   │   ├── deepseek.rs     # DeepSeek 适配器
│   │   └── custom.rs       # 自定义渠道适配器
│   ├── commands/           # Tauri 命令层（前端调用入口）
│   │   ├── mod.rs
│   │   ├── channel.rs      # 渠道 CRUD 命令
│   │   ├── api_key.rs      # 密钥 CRUD 命令
│   │   ├── log.rs          # 日志查询命令
│   │   ├── stats.rs        # 统计命令
│   │   ├── settings.rs     # 设置命令
│   │   ├── server.rs       # 服务器状态命令
│   │   ├── security.rs     # 安全规则命令
│   │   └── import_export.rs # 导入导出命令
│   ├── core/               # 核心业务逻辑
│   │   ├── mod.rs
│   │   ├── dispatcher.rs   # 负载均衡调度器
│   │   ├── proxy.rs        # API 转发代理
│   │   └── security/       # 安全审计（子模块）
│   ├── db/                 # 数据层
│   │   ├── mod.rs          # Database 连接池
│   │   ├── models.rs       # 数据模型定义
│   │   └── repository.rs   # 数据仓库（CRUD）
│   ├── security/           # 安全审计引擎
│   │   ├── mod.rs          # 安全类型定义 + 策略模式
│   │   ├── scanner.rs      # 风险扫描器
│   │   ├── redact.rs       # 数据脱敏
│   │   └── rules.rs        # 安全规则管理
│   ├── server/             # HTTP 服务器
│   │   ├── mod.rs          # 服务器启动
│   │   ├── router.rs       # 路由定义
│   │   └── handlers.rs     # 请求处理器
│   └── utils/              # 工具函数
│       ├── id.rs           # ID 生成
│       └── time.rs         # 时间工具
├── migrations/             # 数据库迁移
│   ├── 001_init.sql        # 初始建表
│   ├── 002_add_request_body.sql
│   ├── 003_security_audit.sql
│   └── 004_security_rules.sql
├── Cargo.toml              # Rust 依赖配置
├── tauri.conf.json         # Tauri 应用配置
└── build.rs                # 构建脚本
```

## 优先级 + 组内权重负载均衡
- 优先级：高到低
- 组内权重：按比例分配

```javascript
请求 gpt-4o
   │
   ↓ 过滤：状态启用 且 支持该模型
┌─────────────────────────────┐
│ 优先级 10: [OpenAI-A(w:5), OpenAI-B(w:3)]  ← 先尝试这组，组内 5:3 概率
│ 优先级 5:  [DeepSeek(w:1)]                 ← 上面全失败后尝试
│ 优先级 1:  [Zhipu(w:1)]                   ← 最后兜底
└─────────────────────────────┘
   │
   ↓ 输出有序队列
[OpenAI-A/B(随机序), OpenAI-B/A, DeepSeek, Zhipu]
   │
   ↓ Proxy 层按顺序尝试，直到成功或耗尽
```

```typescript
┌─────────────────────────────────────────────────────────────┐
│ 1. 解析请求模型（body.model）                                 │
│ 2. 安全扫描请求体 ──→ Block? ──是──→ 记日志 + 返回 451        │
│                          │否                                 │
│ 3. 脱敏处理（如启用）                                          │
│ 4. 查询启用渠道（Repository）                                  │
│ 5. 调度器构建故障转移队列（Dispatcher）                         │
│ 6. for 渠道 in 队列.take(最大尝试次数):                        │
│      ├─ 渠道转配置（channel_to_config）                        │
│      ├─ 获取适配器（get_adaptor）                              │
│      ├─ 转发请求 ──→ 成功? ──是──→ 响应安全扫描                │
│      │                       │      记录日志 + 扣配额 + 返回   │
│      │                       否                                │
│      └─ 记录失败日志，继续下一个                                │
│ 7. 全部失败 → 返回 502                                        │
└─────────────────────────────────────────────────────────────┘
```

## 分层设计：

```TypeScript
┌──────────────────────────────────────────────┐
│ 前端（TypeScript）                            │
│   allowed_models: string[]                   │
├──────────────────────────────────────────────┤
│ 命令层 DTO（commands/api_key.rs）             │
│   ApiKeyDto { allowed_models: Vec<String> }  │  ← From<ApiKey> 转换
├──────────────────────────────────────────────┤
│ 存储层 Model（db/models.rs）                  │
│   ApiKey { allowed_models: String }          │  ← JSON 字符串
├──────────────────────────────────────────────┤
│ 数据库（SQLite）                              │
│   allowed_models TEXT                        │
└──────────────────────────────────────────────┘
```