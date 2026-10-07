# Chat 双窗内存优化结果

2026-10-07，基于 `26f43651`，本地分支 `codex/chat-memory-optimize`。场景是主窗口与 Chat 弹窗同时使用。

**本轮已完成入口隔离、绘制配置和窗口生命周期优化。受控的两份 Chat 页面总私有内存从 359.5 降至 308.6 MiB，减少 14.2%；第二份页面的增量减少 29.4%。真实主窗 + Chat 已通过原生生命周期验收，但没有同配置的优化前真实双窗基线，不能把这两个百分比当作真实账号双窗的完整收益。**

## 实现

| 改动 | 作用与边界 |
| --- | --- |
| `chat.html` / `chat-main.ts` 独立入口；主窗启动门按需加载 | Chat 不再通过主应用入口加载控制台模块；兼容旧 Chat 路由 |
| `TitleBarChrome` 与主窗标题栏拆开 | Chat / 调试弹窗复用窗口按钮，避免标题栏带入终端状态；工具窗口关闭自身 |
| 移除 Chat 依赖链的 UI barrel 导入；CodeMirror 单独分包 | 解决消息分段组件间接加载编辑器的问题；最终 Chat 不加载 CodeMirror、xterm、终端或主窗启动模块 |
| 浏览器预览 mock 按需加载 | 原生 Chat 不加载仅供浏览器预览的样例数据，服务的异步契约不变 |
| Chat 使用不透明原生窗口，取消继承 Mica | 减少绘制资源；主窗口配置不变，Chat 保留阴影 |
| 已有 Chat 窗口也处理 `release_main` | 复用 Chat 时，显式回收主窗的请求不再被忽略；保留组件任务门控，回收失败恢复主窗可见 |
| 显隐与原生 Focus / Resize 协调 | 最小化进入隐藏/Low，恢复重新可见；不可见窗口的迟到焦点事件不再唤醒 controller，初始隐藏启动不提前休眠 |
| 后台 Chat / 主窗更早进入 Low | Chat 15 秒、主窗 20 秒；可见窗口仍保留，调试弹窗阈值不变 |
| 异步 resize 订阅清理 | 组件卸载后才完成的监听注册立即取消，避免遗留监听 |

没有加入生产 Suspend 或关闭正在使用的主窗口。Low 保持页面脚本运行。

## 受控 release 对照

环境与优化前调查一致：本机 Windows / WebView2、DPR 1.5；一份测试账号与一条文本消息，不安装生产 AppState，不加载真实配置。每个阶段取三个相邻样本的中位数；这是短时受控实验，不是三次独立冷启动的统计验收。

原生探针窗口保持隐藏，Normal 阶段 controller 可见；因此下表测页面与引擎驻留成本，不包含两个原生窗口同时显示时的完整桌面输出成本。最终运行显式指定新的独立 WebView profile，关闭远程调试，并核对 renderer 数量。

| 阶段 | 优化前总私有内存 MiB | 优化后总私有内存 MiB | 优化后 renderer 数 |
| --- | ---: | ---: | ---: |
| 没有窗口 | 3.9 | 3.9 | 0 |
| 一份 Chat，Normal | 242.3 | 225.8 | 1 |
| 两份 Chat，Normal | 359.5 | 308.6 | 2 |
| 两份均隐藏并设 Low | 272.6 | 254.2 | 2 |
| 两份进一步 Suspend（仅探针） | 266.3 | 248.8 | 2 |
| 销毁一份，保留已挂起的一份 | 218.7 | 209.3 | 1 |
| 全部销毁 | 6.8 | 6.8 | 0 |

第二份页面增量由 **117.2 降至 82.8 MiB**。最终双页的 renderer 合计 94.2 MiB、GPU 130.4 MiB；优化前分别为 101.4 / 162.6 MiB。主要收益包含绘制成本下降，不能把它全部归因于 JS 包缩小。

同一轮中间版本前端的绘制配置 A/B：继承透明/Mica 时双页 359.0 MiB，不透明时 317.9 MiB。两次都是短时单轮实验，提示绘制配置贡献明显；后续依赖隔离完成后的最终值是 308.6 MiB。

最终探针返回 `MULTIWINDOW_RESULT Ok(())`，全部销毁后 renderer / GPU / browser 消失。另一次没有显式指定新 profile 的运行出现额外、未关联的 renderer，不满足预定进程数，已排除出对照；其原因尚未定位，没有把该次异常解释为产品泄漏。

## 真实产品原生验收

本次运行使用临时 `product_dual_window_probe` 入口，在构建时指定独立 bundle identifier，并在启动时指定临时数据根与 WebView profile，避免单实例插件把操作转发到另一个工作区。验收程序使用产品 AppState 与窗口命令，带 CDP 仅用于驱动和检查，未调用聊天发送命令。该入口依赖外部参数才能隔离，已在 PR 审查后移除，避免默认运行时误触生产实例。

**空临时数据根触发了产品的旧配置迁移，因此该次运行不是空账号测试。** 只用它验收真实窗口和模块加载行为，没有把它作为干净的前后内存对照。生产注册表 InstallDir / DataRoot 指针经检查未改变；测试复制的账号配置、档案、日志与截图已单独清除。

| 检查 | 结果 |
| --- | --- |
| 主窗与 Chat 同时可见 | 两个 renderer 均存在 |
| Chat 加载资源 | 未加载 AppBootGate、CodeMirror、xterm、TerminalDock、预览 mock；已加载 JS 解码大小合计 978,009 字节，约 0.93 MiB |
| 后台主窗 | 仍可见，20 秒后 controller 实际进入 Low |
| 最小化 / 恢复 Chat | controller 实际隐藏 / Low，并能恢复可见 |
| 已有 Chat 再请求回收主窗 | 主窗 renderer 消失，Chat 保留 |
| 恢复控制台 | 创建新的主窗页面并完成挂载 |
| 关闭 Chat | Chat renderer 消失，主窗保留 |

原生驱动返回 `PRODUCT_DUAL_WINDOW_RESULT Ok`。本次主窗 + Chat 初期总私有内存约 459.7 MiB，后台主窗 Low 后约 424.9 MiB；显式回收主窗后约 336.2 MiB。这些数字含迁移配置和远程调试开销，只记录该次运行，不能与上表相减算收益。

主窗重建和随后关闭 Chat 时出现过较大的 GPU 临时分配峰值；后续主窗空闲 Low 的采样回落到总量约 363 MiB、GPU 约 173 MiB。关闭 Chat 的 renderer 清理通过，但本轮没有解决所有 GPU 瞬时峰值，也没有完成图多群聊、长时间收消息的增长验收。

## 验证与产物

- `pnpm.cmd run build`：TypeScript 与 Vite 生产构建通过。
- 前端全量单测：195 个文件、1767 个测试通过；包含窗口关闭归属和迟到监听清理回归测试。
- `cargo check --workspace --all-targets` 通过；调度器相关 Rust 测试 19/19 通过。
- 修改的 UI 文件 Prettier 检查、修改及新增 Rust 文件 rustfmt 检查通过；无 IPC 类型变更。
- 原始验收已生成默认 identifier 的正式 release 可执行文件，未构建 MSI；代码随后提交到 PR #152 审查。

PR #152 审查后的修复：移除依赖外部隔离配置的产品入口；显示窗口时等待 WebView 可见性读回成功，并用窗口/状态代次拒绝失效请求。调度器回归测试 20/20 通过，覆盖未完成、失败和迟到的可见性确认。

原始受控日志保留在本机 `tmp/perf/final-chat-repeat.log`，优化前数据见[调查报告](2026-10-07_chat-dual-window-memory-report.md)。保留的隔离探针见 `src-tauri/examples/chat_multiwindow_probe.rs`，不调用生产 runner。测试 profile 的批量删除被执行策略拦截（只返回 `blocked by policy`，没有具体理由）；账号配置等明确文件通过编辑器删除，剩余临时缓存目录是 `tmp/perf/optimized-product-profile/` 和 `tmp/perf/optimized-product-profile-final/`。

以下哈希对应原始验收构建，不含 PR 审查后的窗口恢复时序修复。

| 原始验收产物 | SHA-256 |
| --- | --- |
| `target/release/NapCatQQ-Desktop.exe` | `C6D36F8FD904A0EFB0BB34D2B9ABF06532B4B2F1358848DBD5230AFE392A0F6E` |
| `dist/chat.html` | `932A31A981036276FB7699C56246593E9E2DE67EC0DAF8F0B6CCE1FE48E8BA64` |

下一轮应针对真实主窗与 Chat 的同配置前后曲线、主窗重建 GPU 峰值和图片密集会话做测量。本轮结果证明固定增量可以降低，不证明长时增长已经消除。
