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
