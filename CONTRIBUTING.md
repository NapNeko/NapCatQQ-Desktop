# 参与 NapCatQQ Desktop 开发

欢迎修复问题、改进体验和补充文档。开始前先读 [代码地图](docs/context/codemap.md)，找到对应功能的入口；较大的行为变更建议先在 [Issue](https://github.com/NapNeko/NapCatQQ-Desktop/issues) 里说明使用场景和预期。

## Windows 开发环境

| 工具                      | 要求                                                         |
| :------------------------ | :----------------------------------------------------------- |
| Node.js                   | 22.13+ 的 22.x，或 24+；需满足当前 ESLint / jsdom 的版本要求 |
| pnpm                      | `9.15.9`，见 `package.json` 的 `packageManager`              |
| Rust                      | stable，MSVC 工具链；workspace 声明最低 `1.85`               |
| Visual Studio Build Tools | C++ 桌面开发工作负载与 Windows SDK                           |
| WebView2                  | Microsoft Edge WebView2 Runtime                              |
| Git                       | 用于源码和应用端依赖管理                                     |

Build Tools、WebView2 和 Rust 的安装步骤见 [Tauri 官方 Windows 环境说明](https://v2.tauri.app/start/prerequisites/#windows)。正式 MSI 构建与 Windows 专属行为请在 Windows 上验证。

```powershell
git clone https://github.com/NapNeko/NapCatQQ-Desktop.git
cd NapCatQQ-Desktop
pnpm.cmd install --frozen-lockfile
pnpm.cmd run tauri:dev
```

Windows 下使用 `pnpm.cmd` 可避免 PowerShell 脚本执行策略影响。启动开发应用前，留意本机已配置的 Bot、自动启动和通知设置。

Windows 的 `tauri:dev` 自动使用 `%LOCALAPPDATA%\NapCatQQ Desktop\cargo-targets\<工作区标识>`，避免可执行产物继承沙盒工作区的低完整性标签。每个工作区独立缓存，已有 `CARGO_TARGET_DIR` 时沿用它；实际路径会显示在启动日志中。首次切换目录需要重建。直接执行 Cargo 命令时可使用日志中的同一产物目录。

## Linux 开发环境

使用与上表相同的 Node.js、pnpm 和 Rust stable，Rust 使用本机 GNU 工具链。Linux 桌面依赖 GTK 3、WebKitGTK 4.1 和 AppIndicator；Debian / Ubuntu 可先安装：

```bash
sudo apt update
sudo apt install build-essential curl wget file pkg-config libwebkit2gtk-4.1-dev libxdo-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev patchelf
pnpm install --frozen-lockfile
pnpm run tauri:dev
```

其他发行版的依赖名称见 [Tauri 官方 Linux 环境说明](https://v2.tauri.app/start/prerequisites/#linux)。开发与运行均需要图形桌面会话。Linux 下使用 `pnpm`，下文检查表中的 `pnpm.cmd` 替换为 `pnpm` 即可；`scripts/build-remote-agent.mjs` 在 Linux 上会跳过 Windows Agent 构建。

```bash
# 在 Linux 上构建 deb 和 AppImage
pnpm run tauri:build:linux
```

产物位于 `target/release/bundle/{deb,appimage}`。Tauri 自动合并 `src-tauri/tauri.linux.conf.json`；Windows 保持使用 `tauri.windows.conf.json`。正式 Linux 安装包在 Ubuntu 22.04 x64 上构建，避免依赖更新系统的 glibc。CI 对 Linux 运行 workspace 编译检查，平台专属行为仍需对应桌面环境验收。

## 浏览器预览

```powershell
pnpm.cmd run dev
```

打开 `http://localhost:1420`。非 Tauri 环境会使用 `src-ui/core/ipc/mock/` 的演示数据，适合检查页面、交互和文案。它不能验证真实协议端、SSH、窗口生命周期、原生截图或 PTY 终端。

## 代码结构

| 路径                                               | 职责                                     |
| :------------------------------------------------- | :--------------------------------------- |
| `src-ui/`                                          | React 界面、hooks、服务与生成的 IPC 类型 |
| `src-tauri/`                                       | Tauri 命令薄壳、窗口、托盘与安装包         |
| `crates/ncd-domain`                                | 配置、模型、事件与错误类型               |
| `crates/ncd-traits`                                | 接口与 Mock 边界                         |
| `crates/ncd-runtime`                               | Bot、应用端、调试台与任务编排            |
| `crates/ncd-backend-*`                             | NapCat 与 SnowLuma 实现                  |
| `crates/ncd-host` / `ncd-component` / `ncd-deploy` | 主机、组件与部署                         |
| `crates/ncd-appframework`                          | 应用端框架适配                           |
| `crates/ncd-onebot` / `ncd-mcp`                    | OneBot 协议能力与 MCP 接入               |
| `crates/ncd-watch`                                 | 独立发布的远端监控程序                   |
| `docs/`                                            | 用户文档、代码地图和发布说明             |

Tauri command 只转换参数和错误，业务放在对应 crate。跨 IPC 的类型由 Rust `ts-rs` 导出，前端使用生成类型；两个协议端 backend 互不依赖。完整架构与功能落点见 [代码地图](docs/context/codemap.md)。

## 检查与构建

实现完成后按改动范围统一验证。本地默认最多 2 个 Vitest worker、2 个 Cargo 编译任务、2 个 Rust 测试线程，避免同时启动多套全量检查。

| 改动 / 目标         | 命令                                           |
| :------------------ | :--------------------------------------------- |
| 前端类型            | `pnpm.cmd run typecheck`                       |
| 相关前端测试        | `pnpm.cmd run test:related <源文件路径>`       |
| 全部前端测试        | `pnpm.cmd run test:unit`                       |
| Rust 编译检查       | `pnpm.cmd run rust:check`                      |
| 指定 Rust 测试      | `cargo test -p <crate> <过滤词>`               |
| 全部 Rust 测试      | `pnpm.cmd run rust:test`                       |
| 修改 IPC 类型后导出 | `pnpm.cmd run ts-bindings`                     |
| lint 与格式检查     | `pnpm.cmd run lint` / `pnpm.cmd run fmt:check` |
| 综合检查            | `pnpm.cmd run verify`                          |
| 版本元数据一致性    | `pnpm.cmd run check:versions`                  |
| 构建 MSI（Windows） | `pnpm.cmd run tauri:build:msi`                 |
| 构建 deb / AppImage（Linux） | `pnpm run tauri:build:linux`           |

`verify` 包含类型导出、TypeScript、lint、Rust 和格式检查，不包含完整测试；导出的 `src-ui/core/ipc/generated/` 有变化时一起提交。CI 入口见 [ci.yml](.github/workflows/ci.yml)，真实环境冒烟测试按相应测试的说明单独执行。

## 提交 PR

1. 从 `main` 创建分支，一次聚焦一个问题或功能，保留工作区里无关的改动。
2. 补充必要的验证和用户文档；UI 文案保持简洁，注释说明原因或边界。
3. 按具体路径暂存和提交，使用中文 subject / body 与英文类型前缀，例如 `fix(chat): 修复切换会话后草稿丢失`。
4. 在 PR 中说明触发场景、改动后的行为、已执行的验证和未覆盖的环境。附图时隐藏真实消息、凭据和私人路径。

不要提交 `.env`、密钥、token、本地计划或参考仓库副本。旧 Python 实现仅供行为对照，不恢复到生产路径。

## 发布说明

用户可见的变化写入 `docs/releases/vX.Y.Z.md`，远端监控使用 `watch-vX.Y.Z.md`。维护和预览流程见 [发布说明指南](docs/releases/README.md)。Desktop 与 ncd-watch 独立发布；Windows 自更新读取 GitHub Release 中的 MSI，Linux 使用发布页的 deb / AppImage 手动升级。
