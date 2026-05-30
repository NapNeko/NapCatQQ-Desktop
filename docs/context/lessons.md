# 历史踩坑教训

> 从原 .claude/CLAUDE.md §6 + §2.5 拆出。规划新功能 / 新模块前自查,新坑随时往这里补。
> 硬约束在 .claude/CLAUDE.md,能力速查在 capabilities.md / frontend.md。

---

## 0. 历史架构错误（不准再犯，原 §2.5）

1. 计划用 `serde_json::Value` 透传 BotConfig — 应建强类型 `BotConfig` struct
2. 把业务逻辑直接塞 Tauri command — 应走 Manager 编排层
3. 没看 `crates/ncd-runtime/src/` 现状就开始设计 — 应先 ls 看现有 trait
4. 准备重新发明状态机 — 应复用现有 `BotActor`（6 状态）
5. 让前端直接调 IPC 传 dict — 应用 ts-rs 派生强类型契约

每次规划新模块前回头逐条 self-check。

---

## 1. napcat-webui-login（9 个 Pit，全部已修）

| Pit | 现象 | 教训 / 落地铁律 |
| :-: | :--- | :--- |
| 1 | requirements 写"isOnline 时启用 WebUI 按钮"反向锁死（用户要先打开 WebUI 才能扫码） | spec 必须分清"物理资源就绪"和"业务状态就绪"两层。修法：`webuiAvailable` 只看 `webuiPort && webuiToken` |
| 2 | 5 个并行 sub-agent 写文件，task 6.1 给 task 2.1 的文件塞占位 trait 互相覆盖 | R6：sub-agent prompt 必须写"只能写文件 X" |
| 3 | 6.2 / 6.4 / 6.6 长任务被 cancellation 中断，工作量翻倍 | R6：拆细粒度，实现 + 测试分两步 |
| 4 | 前端 `bot.state === 'Running'` 永远 false，因为后端 serde 输出 snake_case | R1：所有跨边界 enum / struct 走 ts-rs 派生 + `npm run ts-bindings` |
| 5 | `NapCatWebuiAvailable` 被 serde rename_all snake_case 切成 `nap_cat_webui_available`，5 个事件错位；同时 `BotLogAppended` 的 `tauri_event_name` 写成 `"log_appended"` 漏前缀，反向漂移 | R2 / R3：连续大写显式 `#[serde(rename = "...")]`；`tauri_event_name` 与 serde `kind` 单一字面量来源 |
| 6 | Tauri webview 不支持 `<a target="_blank">`，按钮点了没反应 | R4：装 `tauri-plugin-opener` 走 IPC 让 Rust 端调系统 API |
| 7 | npm 11.15 `arborist` 在 pnpm hoisted node_modules 上 NPE | 用 lockfile 文件名确认包管理器，不要混用。或删 `node_modules` + `package-lock.json` 全清 |
| 8 | `tauri-plugin-opener` 装包后还要 capabilities 显式授权 | R5：装包 → `Builder.plugin(...)` → `capabilities/*.json` 加 `opener:default` + `opener:allow-open-url` |
| 9 | Bot 状态 ≠ 登录态。Poller 第一次轮询前 1-2 秒 UI 空窗，看起来"啥也没启动" | UI 给 N+1 个反馈（N 状态 + "正在加载"）。补"等待登录态"徽章覆盖 `null && webuiAvailable` 过渡 |

工程化补强（已落地）：
- 强制走 ts-rs：所有 `BotConfig` / `*Snapshot` / `*Status` / `Domain*` 加 `TS` 派生 + 自动导出。`types.ts` 只 re-export
- CI 步骤：`npm run verify` 跑 `cargo test export_bindings_` 生成 TS，`git diff --exit-code src-ui/core/ipc/generated/` 检查未提交
- 跨边界字面量测试：`crates/ncd-runtime/src/events.rs` 里 `every_domain_event_variant_is_listed_in_frontend_events_ts` 测试覆盖所有 variant（不仅是新增的）

---

## 2. snowluma-backend-runtime（7 个 hotfix + 1 个脚本灾难）

7 hotfix 链（按时间顺序，全部源自真机调试）：

1. `node.exe missing` → planner 把 NapCat runtime_root 当 SL daemon 安装根。修：planner 加独立 `snowluma_runtime_root` 字段
2. `SNOWLUMA_QQ_EXE env var missing` → launch planner 没把 `qq_install_path` 注入 env。修：写 `SNOWLUMA_QQ_EXE` / `SNOWLUMA_START_MODE` / `SNOWLUMA_ATTACH_PID` 三个 env vars
3. 卡在"启动中"无反馈 → daemon 入口候选列表 + 立即 spawn `watch_exit`（不等 Ready 后）+ stderr `Stdio::piped()` 必须有 reader 否则缓冲区满会卡
4. 错误信息没线索 → daemon 加 `recent_log: VecDeque<String>` ring buffer（容量 50），`rollback_to_stopped` 等 150ms 让 reader flush 把最近 ≤10 行拼到 `last_error`
5. `MODULE_NOT_FOUND` → 入口候选列表 `index.mjs` 放第一位 + 路径 `runtime/SnowLuma`（对齐 Linux 大小写）
6. daemon Ready 但 WebUI 按钮禁用 → BotCard 加显式 `isSnowLuma: boolean` prop（不要用 `daemonState !== null` 反推）；BotListPage 异步通过 `getBotConfig` 拉每 bot `backend_type` 缓存到 `flavorByBot` map
7. WebUI 按钮跳转 → 后端 `open_snowluma_webui` 命令读 `<runtime_root>/SnowLuma/config/runtime.json` 的 `webuiPort` + `<data_root>/snowluma/session.json` 的密码，前端 `navigator.clipboard.writeText(password)` + `openUrl(url)`（SL 表单登录不接受 token query）

### 脚本灾难复盘（SESSION_LOG #4，全员血泪）

起因：用 `scripts/strip_spec_refs.py` 想批量清"规划性注释"。最后一段为了清裸空括号用了 `re.sub(r"\(\s*\)", "", text)`。这一行致命，把所有合法空括号清掉：`.method()` → `.method`、`String::new()` → `String::new`、`Result<()>` → `Result<>`、`tauri::generate_context!()` → `tauri::generate_context!`。`cargo check` 报 119 个错误。

救援链路：试图 `git stash --keep-index` 时脚本已经覆盖了文件，stash 保存的是损坏版本。又写 `fix_corruption.py` 反向加 `()` 但误伤 attribute 字符串、把字段 `bot_id`/`status` 当方法加 `()`。最后放弃脚本，纯手工 + PowerShell `-replace` 按 cargo 错误一项一项过，约 6 轮 cargo check 才修完（119 → 87 → 79 → 73 → 51 → 24 → 10 → 0）。删除 3 个修复脚本。

副作用：所有触及文件的缩进被 PowerShell `-replace` 压平，长行折行消失。功能等价但视觉极乱，需要 `cargo fmt --all` + `prettier --write src-ui` 恢复。

教训（已落 R7 / R8）：
- 永远不要用纯文本正则批量清代码注释。要做就用 AST 工具（tree-sitter / ra_ap_syntax / syn），保证只动 token kind == comment 的范围
- 改大量文件之前先 `git commit`，留 reset 退路
- 修复阶段每改一项跑 cargo check：错误数下降才证明方向对；上升立刻 revert
- PowerShell `-replace` 无 lookahead/lookbehind，批量改动很难证明正确。手动 `str_replace` 更慢但更安全
- 单次写入 ≤ 4096 tokens（R9）已经救过几次 sub-agent 网络中断，手动救援也用了同样策略

---

## 3. 前端推倒重写阶段踩的 8 个坑

- 坑 1：npm 11.15 `arborist` NPE。`--legacy-peer-deps` / `--no-audit` / `--no-fund` 都治不了。修：删 `node_modules` + `package-lock.json` 全清后再 `npm install --legacy-peer-deps`
- 坑 2：`smartRelocate` 把旧 Fluent App.tsx 的 import 也跨蓝绿改了。修：`git restore`。教训：涉及"两套并存"目录时优先用 `fs_write` + 手动改 import，不要用 smartRelocate
- 坑 3：Tauri TitleBar 挂顶层时盖住 mascot。修：布局换成 `flex-col [(Sidebar | (TitleBar | main))]`，Sidebar 真正贴 0,0
- 坑 4：折叠 sidebar 后展开按钮跑左下角反人类。修：折叠态把整个 logo 当展开按钮，hover 时 logo 淡出 / `ChevronsRight` 淡入
- 坑 5：OccupancyChart 改了 4 版才对。前 3 版凭感觉调。第 4 版回头读 legacy `_OccupancyCanvas` 全部 200 行算法 1:1 对应（N+1 点、`x = i * step_x - progress * step_x`、`_build_display_points` 端点插值、cubic bezier 控制点 = 段中点、rAF 1950ms linear、padding `top:8 bottom:8`、纯手写 SVG 脱离 recharts）。教训："改了 N 版还不对"时停下来回头读源算法
- 坑 6：flex 嵌套撑满高度 CPU/RAM 不等高。修：CPU 和 RAM 都加 `min-h-0 flex-1`，父容器每层都加 `min-h-0`
- 坑 7：mascot 衣服染色不能用 filter hue-rotate（会污染脸 / 头发 / 围巾）。修：Vite `?raw` 把 SVG 当字符串引入，运行时 `String.replaceAll('#6a95aa', primary)` 替换 legacy 衣服两色，`dangerouslySetInnerHTML` 渲染。`vite-env.d.ts` 加 `declare module '*.svg?raw'`。SVG 是构建时打包的可信资源，安全
- 坑 8：CDN 中转网关在长耗时图像生成 API 上 502。教训：中转 API 不可靠，设计沟通别强依赖 AI 出图

---

## 4. 跨阶段反复出现的 trap

- trait 用 `#[async_trait]` 宏 → impl 必须也用，不能混用原生 `async fn in trait`（M3.B）
- 照抄 docs.rs 最新示例不靠谱，不同 minor 版本 API 重排很常见。新引入第三方 crate 第一件事去看 cargo registry 实际源码（M3.A russh 0.45）
- SSH 协议下 `ExitStatus` / `Eof` / `Close` 到达顺序不保证，break 条件挑最强的（Close）（M3.D）
- 真机测试前先 `ssh -v <host>` 看本地哪把密钥实际工作（M3.C：服务器只有 RSA 公钥，默认 ed25519 reject）
- stub 也要写满 trait 17 个方法，Rust trait 没"部分实装"概念（M4.B RemoteWindowsHost）
- Debug 派生不走 `#[serde(rename_all)]` 转换，要 snake_case 用 `as_str()` helper（M4.C）
- 跨 crate ts-rs 派生：字段类型来自其他 crate 时要么那个 crate 派生 TS，要么这边 `#[ts(type = "...")]` 显式覆盖（M5.B SchemaVersion）
- Windows MSVC linker + crate 名含 update / setup / install 关键字 = UAC 弹窗（os error 740）。修：`build.rs` 嵌入 manifest 显式声明 `asInvoker`（M5.A ncd-update）
- git mv + 反悔比想象中麻烦。最稳的做法是先完整 commit 一次"目录移动"，再做内部内容修改，反悔时只需 `git revert`（M6.1 napcat 子目录还原）
- fixture 文件不能等到接入时才发现"覆盖不全"。理想做法初版 fixture 就完整覆盖最常见的 schema 场景（M6.5）
