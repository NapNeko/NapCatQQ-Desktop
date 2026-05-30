# 项目当前状态 (活文档)

> 这是项目的"此刻"快照,每次会话开头由 SessionStart hook 自动注入。
> 它只记录当前状态,不堆历史。想随时更新打 `/handoff`,我也会在里程碑节点主动更新。
> 死知识(踩坑/能力/架构详解)在 docs/context/,硬约束在 .claude/CLAUDE.md。

更新时间: 2026-05-30 | 分支: rust-migration | 工作区: clean

---

## 一句话现状

后端 Rust 迁移主体完成 (M0-M6.5),前端正在从 Fluent v9 推倒重写到 Tailwind v4 + Radix。
最近一轮做完了组件页单机重构 + Docker 集成 + 远端服务器编辑/免密,已提交,**尚未真机点验**。

## 当前主线 / 下一步

1. **真机点验上一轮改动** (最高优先,见下方点验清单) — 需要腾讯云 Linux 远端 + 本机 Windows
2. 前端推倒重写剩余 Step:Step 6 RemoteHostPanel → Step 8 BotConfig → Step 9 BotLog → Step 10 删旧 Fluent 树
3. M7 真机回归 + 自更新链路点验

## 手头改到一半的东西

(无 — 工作区干净,上一轮已全部提交)

## 待真机点验清单 (上一轮组件页/Docker/远端改动)

1. 冷启动直接进组件页,远端那几行是否已探好 (启动预热生效)
2. 本机视图无 Docker 行/无部署按钮;切腾讯云有 Docker 行 + 框架行部署按钮
3. 删光远端 → 侧边栏 Docker 项消失;加回来又出现
4. 部署对话框:端口用途说明、改宿主机端口、加自定义端口、删端口
5. 远端卡片:编辑改地址保存 (确认下次连接用新地址);密码档案点「配置免密」输密码 → 成功后档案变密钥认证、之后免密
6. 添加服务器勾「自动配置免密」→ 填密码添加 → 看是否自动推密钥成功

## 验证基线 (上次绿态)

- `npm run verify` 全绿 (ts-bindings + typecheck + cargo check --workspace)
- `cargo test -p ncd-runtime` 全绿 (266 测试,含 ssh_keygen 2 个新单测)
- 唯一 warning 是 pre-existing 的 ncd-deploy 里 kill_calls dead_code,无关

## 关键文件锚点 (上一轮)

后端: server_manager.rs (单飞连接/setup_key_auth) · ssh_keygen.rs · remote/linux.rs (SFTP复用) · commands/{components,servers,host_resolve}.rs
前端: modules/{components,docker,remote}/ · hooks/{components,docker,remote}/ · core/domain/{errors,docker,components}.ts · core/services/{docker,server}.service.ts

## 里程碑级待办池 (不急,排在主线后)

- 前端推倒重写 Step 6 RemoteHostPanel.next.tsx (拆 4 个子组件 + 接全局 InfoBar)
- 前端推倒重写 Step 8-9:BotConfigPage / BotLogPage / Step 10 收尾清 DEPRECATED
- M7 真机回归 (本地 Windows + 远端 Linux + 自更新链路)
- BotManager 重构:移除 backend-specific 字段,为 ncd-backend-* 拆分铺路 (独立 spec,配套 R11)
- ncd-test-support/fixtures/legacy/*.json 字段补全到能覆盖 config_migration.rs 30+ 字段场景
- 代码格式恢复:cargo fmt --all + prettier --write src-ui (脚本灾难遗留)
- 代码注释里 spec 引用文本清理 (task X.Y / design §X.Y 等),只能用 AST 工具不能 regex
- napcat-webui-login spec 文档修订 (requirements 10.6 / design §13/§17.3 / tasks task 14)

## 整体里程碑状态 (后端全 done,前端进行中)

M0-M6.5 后端全部 done (579 测试);M-net/M-snow/M-ui/M-toast done;前端 step 0-5 + Step 7 done;
Step 6 (RemoteHostPanel) 是下一个前端主线;Step 8/9/10 + M7 真机回归未开始。
