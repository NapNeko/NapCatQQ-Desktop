# NapCatQQ Desktop 文档

## 目录结构

```
docs/
├── context/           # Agent/开发活上下文（进 git；codemap 等）
│   └── codemap.md     # 功能域 → 代码落点
├── releases/          # 发版策展正文（进 git；CI / 本地 preview）
│   ├── README.md      # 写法与 pnpm release:notes:* 流程
│   └── vX.Y.Z.md      # Desktop；watch-vX.Y.Z.md 为 ncd-watch
├── user/              # 用户文档（进 git，对外公开）
│   └── README.md
└── dev/               # 开发文档（不进 git，内部使用）
    ├── troubleshooting/
    ├── architecture/
    ├── workflow/
    └── archive/
```

## 文档分类

### 上下文文档 (docs/context/)
- **进 git 仓库**
- 给开发者与 AI 助手的活地图：功能域 → 代码落点（codemap.md）
- 旧实现对照根：仓库外/本地 .references/NapCatQQ-Desktop-main

### 用户文档 (`docs/user/`)
- **进 git 仓库**，对外公开
- 面向最终用户：安装、配置、使用指南
- 语言：简体中文为主
- 保持简洁、易懂

### 发版说明 (`docs/releases/`)
- **进 git 仓库**
- 每版用户向更新内容（策展）；安装/资源表由 `scripts/release-notes.mjs` 拼装
- 本地：`pnpm run release:notes:draft` → 编辑 → `pnpm run release:notes:preview`
- CI Release 工作流 `render` 优先读本目录，避免把原始 git log 当更新日志

### 开发文档 (`docs/dev/`)
- **不进 git 仓库**（已在 .gitignore）
- 面向开发者：问题排查、架构设计、开发流程
- 包含内部调试信息、临时笔记、问题记录
- 不保存凭据、访问令牌或其他敏感数据；忽略规则不等于安全保护，无法限制文件访问或阻止其他方式上传。

### 归档文档 (`docs/dev/archive/`)
- **不进 git 仓库**
- 存放已完成或已废弃的历史设计文档
- 按类别子目录组织（如 `bugfix/`、`feature/` 等）
- 归档文档保留供未来参考和演进追踪，不再日常更新
- 在对应上层目录的 README 中保留索引和简要说明

### 本地 Agent 工作记录

- 工作计划、会话日志、临时报告、排查记录及一次性设计草案只留本地，优先放在已忽略的 `docs/dev/`；工具已有的 `work/`、`docs/superpowers/` 也只作本地记录使用。
- `docs/context/` 中有意共享的代码地图、能力速查、前端约定和踩坑教训继续入库；只维护可复用的项目知识。
- `.gitignore` 不会让已跟踪文件自动退出版本控制。确认属于本地记录并检查引用后，用 `git rm --cached -- <具体文件路径>` 取消跟踪，保留本地文件，再补精确忽略规则。用途不明的文件先确认。
- 提交前检查 `git status --short` 和 `git diff --cached --name-status`，按具体路径暂存正式文档，不对本地记录使用 `git add -f`。

## 维护原则

1. 用户文档保持最新，随版本更新
2. 开发文档按需记录，不强制完整
3. 问题排查记录及时归档到 `dev/troubleshooting/`
4. 架构设计完成后，考虑是否需要归档至 `dev/archive/<category>/`
5. 废弃文档及时删除，不要留空文件
6. 归档时更新对应目录的 README 索引，说明归档原因和位置
