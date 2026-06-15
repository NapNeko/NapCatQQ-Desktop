# NapCatQQ Desktop 文档

## 目录结构

```
docs/
├── user/              # 用户文档（进 git，对外公开）
│   ├── README.md      # 快速开始
│   ├── installation.md
│   ├── configuration.md
│   └── troubleshooting.md
│
└── dev/               # 开发文档（不进 git，内部使用）
    ├── troubleshooting/   # 问题排查记录
    ├── architecture/      # 架构设计文档
    └── workflow/          # 开发流程文档
```

## 文档分类

### 用户文档 (`docs/user/`)
- **进 git 仓库**，对外公开
- 面向最终用户：安装、配置、使用指南
- 语言：简体中文为主
- 保持简洁、易懂

### 开发文档 (`docs/dev/`)
- **不进 git 仓库**（已在 .gitignore）
- 面向开发者：问题排查、架构设计、开发流程
- 包含内部调试信息、临时笔记、问题记录
- 可以包含敏感信息（IP、配置等）

## 维护原则

1. 用户文档保持最新，随版本更新
2. 开发文档按需记录，不强制完整
3. 问题排查记录及时归档到 `dev/troubleshooting/`
4. 废弃文档及时删除，不要留空文件
