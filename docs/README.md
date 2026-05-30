# docs 导航

NapCatQQ-Desktop 从 Python/PySide6 迁移到 Rust + Tauri + React。本目录只保留当前活跃文档,历史文档全部归到 archive/。

## 活跃文档

| 文档 | 用途 | 入库 |
| :--- | :--- | :--- |
| `rust_migration_blueprint_local.md` | 架构权威蓝图 v2 | 否(gitignore,本地参考) |
| `context/capabilities.md` | 后端各 crate 已就绪能力速查,规划前先查避免重发明 | 是 |
| `context/frontend.md` | 前端分层铁律 + 推倒重写 playbook + hook/store 速查 | 是 |
| `context/lessons.md` | 历史踩坑教训,规划新功能前自查 | 是 |
| `tauri_reference_projects.md` | Tauri 参考项目 + 本地开发链状态 | 否(gitignore,本地参考) |

注:`context/` 三件套是从原 `.claude/CLAUDE.md` 拆出的按需文档。项目硬约束在 `.claude/CLAUDE.md`,当前进度/待办看 `.claude/STATE.md`。

## 历史归档(archive/)

只读,不再维护。需要追溯背景时去查。

| 归档目录 | 内容 | 入库 |
| :--- | :--- | :--- |
| `archive/legacy-python-specs/` | 旧 Python 时代开发规范(PEP8/命名/资源/版本控制)+ 旧 daemon 部署指南 | 是 |
| `archive/legacy-release-chain/` | 旧 Python 发布链文档(RELEASE_WORKFLOW / AI_CHANGELOG_USAGE / CHANGELOG) | 是 |
| `archive/v3-planning/` | AI 早期 v3 重构规划(进度已过期,架构内容被蓝图取代) | 否(gitignore) |
| `archive/snowluma-ssh-specs/` | 旧 Python 实现期的 SnowLuma/SSH 需求 + 执行计划 + smoke 指南 | 是 |
| `archive/remote-ssh-docs/` | 更早的远端 SSH 方案规划与验收 | 是 |
| `archive/daemon-v1/` | daemon v1 说明 | 是 |
| `archive/v1-remote-ui/` | v1 远端 UI 说明 | 是 |
| `archive/rust_migration_blueprint_v1_*.md` | 蓝图 v1 归档 | 否(gitignore) |

## 已知隐患(待重写)

旧发布链(`.github/workflows/release.yml` + `pyproject.toml` 版本读取 + 已归档的 CHANGELOG/RELEASE_WORKFLOW)是 Python 时代产物,Rust 迁移后尚未重写。release.yml:92 仍硬引用 `docs/CHANGELOG.md`(现已移到 archive/legacy-release-chain/)。重写 Rust 发布链时一并修正这个路径引用。
