# Linux 组件校验矩阵

本文对应组件目录中的 12 个主机组件，以及应用端注册表中的 7 个框架。核对入口为 `crates/ncd-runtime/src/components/graph.rs`、各组件的 `supported_targets` / `detect_outcome` / `verify` 和应用端启动编排。

表中的“支持”表示代码声明和执行路径覆盖该目标，不表示已在所有发行版、架构或真实服务器上完成安装测试。

## 校验入口

- **目标支持**：安装与应用端启动检查 `Component::check_target`。Windows 专属组件在 Linux 上保持不支持。
- **依赖就绪**：组件依赖图通过 `detect_outcome` 和版本约束判断可用性；组件页与 Bot 原生启动预检共用组件执行器。`verify` 是安装后的检查入口，不等同于每次启动都重新安装或运行完整自检。
- **路径与架构**：本机 Linux 使用 POSIX 路径；远端 Linux 的架构在建立连接时由 `uname -m` 确定并保存在 Host 中，未知架构拒绝连接，不按 Desktop 的架构猜包。
- **应用端启动**：检查组件目标支持；Linux 重新检查实例主端口是否占用，再解析 Node / Python 启动命令。启动失败沿原有 `last_error` 返回；独立面板口由相应框架配置与安装挑口逻辑处理。
- **进程与日志**：本机通过 Host 进程句柄与进程树管理，远端通过 PID、实例目录与日志跟随管理；本机日志按目标 OS 渲染，远端 Linux 使用 POSIX 路径。

## 主机组件（12 项）

“本机 / 远端”列均指 Linux 原生宿主机。Docker 列指 Desktop 对协议容器的管理路径，不表示把主机组件安装器运行在容器内。

| 组件 | 本机原生 | 远端原生 | Docker | 已有校验与边界 |
| --- | --- | --- | --- | --- |
| NapCat | 支持 | 支持 | 支持协议镜像 | 检查 `napcat.mjs`、`loadNapCat.js`、QQ `package.json` 的注入入口与可解析版本；依赖 QQ、unzip 及 Linux 系统依赖，架构由 QQ 限定为 x86_64 / aarch64。容器走镜像与配置部署路径。 |
| SnowLuma | 支持 Full / Lite | 支持 Full / Lite | 支持协议镜像 | x86_64 / aarch64；检查 `index.mjs`、`client/`、`native/`；Full 检查自带 Node 能执行且满足版本，Lite 要求外部 Node `^22.13.0` 或 `>=23.4.0`。远端原生还要求 noVNC 图形栈。 |
| Node.js | 支持 | 支持 | 随协议镜像管理 | Linux tar.xz 的 `bin/node`；拒绝已不提供受管包的 x86 目标，执行 `--version` 并检查消费者版本约束；支持托管路径、用户路径与 PATH 候选。主机组件不校验容器内的 Node。 |
| uv | 支持 | 支持 | 不单独安装 | GNU 包覆盖 x86_64 / aarch64 / x86 / armv7，默认版本按架构校验内置 SHA；检查可执行文件、执行版本与所选版本。Python 应用端优先使用托管路径和实例 `.ncd-uv` 标记，再查 PATH。 |
| Git | 支持系统包 | 支持系统包 | 不单独安装 | 执行 Git 版本探测；Linux 使用系统 Git，不安装 Windows MinGit 包，也不卸载用户的系统 Git。 |
| Redis / Valkey | 支持 | 支持 | 不单独安装 | 检查服务端程序能执行并返回版本；Valkey 默认包覆盖 x86_64 / aarch64，要求 glibc ≥2.34 与 `libssl.so.3` 并校验 SHA，不兼容时使用系统 Redis 包。程序可执行不等同于实例配置的 Redis 地址已连通。 |
| QQ | 支持 | 支持 | 随协议镜像管理 | x86_64 / aarch64 的 deb / rpm；检查 QQ 程序、`package.json` 与真实版本，安装依赖走 QQ 系统包和解包器检查。用户受管布局包含 `opt/QQ`，系统目录安装才走包管理器提权；文件校验不证明图形会话、扫码和账号登录已成功。 |
| VC++ Runtime | 不支持 | 不支持 | 不适用 | Windows 专属；Linux 依赖图不应把它作为 NapCat 的前置组件。 |
| noVNC 图形栈 | 支持 | 支持 | 随协议镜像管理 | 检查 Xvfb、x11vnc、websockify、dbus-launch、窗口管理器与 noVNC 页面；这些程序存在不等同于真实浏览器/VNC 登录流程已跑通。 |
| ncd-watch | 不支持 | 支持 | 不作为容器组件 | 远端 x86_64 / aarch64 musl 程序；检查受管程序存在并能返回 `--version`，运行通过 `systemd --user` 或 nohup；服务安装和告警发送需要真实远端环境验证。 |
| Windows Agent | 不支持 | 不支持 | 不适用 | 仅远端 Windows 使用，不进入 Linux 运行依赖。 |
| Desktop 自身 | 支持 | 不支持 | 不适用 | 检查当前产品版本和程序路径；Linux 更新打开 Release，由用户安装 deb 或替换 AppImage，不能使用 MSI 自更新。 |

本次主机组件校验收敛：

- Node、uv、Valkey 在暂存目录执行版本检查后才替换既有安装；复制、权限或程序执行失败不再作为安装成功继续。
- Node / uv 的探测和校验统一接受合格的托管、外部或 PATH 工具，Node 版本必须是可识别 semver，Linux 路径保留大小写。
- NapCat 注入脚本缺失、QQ 二进制缺失、QQ 注入入口未修改都能在探测或校验阶段失败。
- SnowLuma 检查入口、目录和 Full 自带 Node，先校验暂存内容再替换，保留用户配置与数据；提供预期 SHA 时，复用缓存归档也要重新核对摘要。
- NapCat Release 和 ncd-watch ARM 附件传递相应 SHA；ncd-watch 的版本标记不能替代程序真实执行。
- noVNC 补齐窗口管理器和静态页面检查，SSH 失败上抛，安装后重新探测；执行器统一检查目标支持。

## Docker 协议部署

NapCat 与 SnowLuma 支持 Linux 本机 Docker 和 Linux 远端 Docker。Windows 本机 Docker 仍拒绝，应用端框架的 Docker 部署仍未开放。

- 本机项目位于 `data_root/docker/<container>`，WebUI、noVNC、日志和 OneBot 使用本机端口；远端项目位于 `$HOME/.napcat-bots`，需要桌面访问的端口通过 SSH 隧道转发。
- 本机容器使用实际用户 UID/GID，指标挂载复用 `data_root/metrics`；远端保留现有 UID/GID 1000 策略，未据此验证所有远端用户映射。
- 拉取或选择镜像后检查镜像与 Docker daemon 的 OS / 架构；镜像回退的 retag 失败会返回错误。Compose 不隐式重拉镜像，启动后确认容器仍为 running；清理和卸载失败会返回错误。
- 本机原生应用端与本机 Docker Bot 使用宿主机网桥地址上的 TCP 转发完成反向 WS；远端沿用相应 SSH 转发。此能力不会把应用端本身变成容器应用。
- 数据根迁移会复制项目文件，但不会修改已有容器的挂载与 Compose 标签。迁移预览和结果会提示保留旧目录，并在 Docker 页删除旧的已停止容器（保留数据卷）后重新启动 Bot；本机运行态接管也核对所属目录，目录不一致时提供实际路径、期望路径和恢复操作。

这些校验覆盖镜像选择、部署结果和连接规划，不证明容器内 QQ 已成功登录，也不把主机组件 `verify` 当作容器内部自检。

## 应用端框架（7 项）

七个框架均声明 Linux 本机与远端原生部署。当前应用端 Docker placement 均未开放；把协议 Bot 部署到 Docker 与把应用端框架装进 Docker 是两个独立能力。

| 框架 | 本机原生 | 远端原生 | Docker | 工具链、启动与安装后校验 |
| --- | --- | --- | --- | --- |
| Karin | 支持 | 支持 | 未开放 | Node ≥18；实例私有 pnpm，直接启动 Karin mjs。检查 `node-karin/package.json`、`.env`、项目入口和 `start/app.mjs`。安装步骤为依赖构建提供 Node/pnpm PATH。 |
| NoneBot2 | 支持 | 支持 | 未开放 | uv ≥0.4、Python ≥3.10 且 <3.14；使用 `.venv/bin/python -u bot.py`，缺 venv 时可由已解析 uv 启动。检查 `pyproject.toml`、`bot.py` 与实例解释器。 |
| AstrBot | 支持 | 支持 | 未开放 | uv ≥0.4、Python 3.12；使用 `.venv/bin/astrbot run`。检查 `data/cmd_config.json` 与实例命令入口，面板账号和 OneBot 配置按框架逻辑写入。 |
| MaiBot | 支持 | 支持 | 未开放 | uv ≥0.4、Python 3.12；实例解释器启动 `bot.py` Runner。检查源码、解释器及 NapCat 适配器 manifest；Linux 安装预检架构、glibc ≥2.28 和剩余空间，拒绝已识别的 musl / 32 位环境。 |
| Koishi | 支持 | 支持 | 未开放 | Node ≥18；使用整包内 `.yarn/releases/*.cjs start`。按 Host 架构选择整包，检查 `koishi.yml`、Koishi 包、OneBot 适配器与 Yarn 入口。 |
| TRSS-Yunzai | 支持 | 支持 | 未开放 | Node ≥22.12、Git、Redis；使用 `node app.js daemon`。检查项目、依赖目录、`app.js` 与 `server.yaml`；依赖安装与私有 pnpm 安装提供正确 PATH。 |
| NeoBot | 支持 | 支持 | 未开放 | uv ≥0.4、Python 3.13；使用 `.venv/bin/neobot`。检查 `app/data/config.toml` 与实例入口，OneBot 口和独立面板口按配置挑选。远端停止使用进程管理，本机可按上游版本和凭据请求优雅关闭。 |

这些 `verify` 检查的是安装产物和工具链条件，不会替用户登录账号、连接模型服务或逐个加载所有第三方插件。

## 本次收敛的应用端问题

- Linux 发行版的 npm 可以位于 `/usr/share/nodejs/npm`，不能只按官方 Node tarball 的 `lib/node_modules/npm` 布局判断。工具链解析保留官方布局优先，并允许通过目标主机的 npm 入口使用已选 Node 执行。
- Karin 安装依赖、初始化与原生依赖构建，以及云崽安装私有 pnpm，都需要给子进程提供相同的 Node PATH；只给最外层命令一个绝对路径不够。
- Koishi 与 MaiBot 使用 Host 保存的真实架构，移除重复 SSH 探测和静默按默认架构回退。
- Linux 应用端启动重新检查主端口，组件不支持与端口占用均在启动前返回，并保存失败原因。

## 实际验证边界

本次在 Linux 工作树完成源码路径审计及以下项目内检查。表内测试是不同筛选范围，单独重跑的用例不重复累计为新的覆盖。

| 命令 / 检查范围 | 结果 |
| --- | --- |
| `cargo check --workspace --all-targets` | 通过，无 Rust warning |
| `cargo test -p ncd-host --lib` | 最终重跑 141 通过；包括本机进程、PTY、SSH 替身、TCP 转发及 sudo 输入分离回归，覆盖需密码、缓存和 NOPASSWD 情况 |
| `cargo test -p ncd-component --lib linux_detect_tests` | 3 通过 |
| `cargo test -p ncd-component --lib snowluma::tests` | 26 通过 |
| `cargo test -p ncd-component --lib nodejs::tests` | 13 通过，包含坏归档保留旧安装的回归 |
| `cargo test -p ncd-component --lib uv::tests` | 6 通过 |
| `cargo test -p ncd-component --lib ncd_watch::tests` | 17 通过 |
| `cargo test -p ncd-appframework --lib node_tooling` | 4 通过；不是七个框架的联网安装测试 |
| `cargo test -p ncd-appframework --lib ports::tests` | 5 通过，包含真实本机回环/通配监听占用与远端监听表解析 |
| `cargo test -p ncd-runtime --lib app_framework::listen_port::tests` | 6 通过；未单独完成 Linux `start_failed` / `last_error` 端到端测试 |
| `cargo test -p ncd-domain --lib docker` | 47 通过 |
| `cargo test -p ncd-deploy --lib docker` | 最终重跑 76 通过；使用项目内模拟环境 |
| `cargo test -p ncd-runtime --lib docker` | 最终重跑 63 通过，包含 Docker 路径的数据根迁移回归 |
| `cargo test -p ncd-runtime --lib native_deployment_adapter` | 16 通过，与 Docker 筛选范围有重叠 |
| `cargo test -p ncd-runtime --test bot_config_repo` | 12 通过 |
| `cargo test -p ncd-runtime --test bot_manager docker` | 4 通过 |
| 数据根迁移相关回归 | 1 通过，与 Docker 筛选范围有重叠 |
| 本次修改的 111 个 Rust 文件执行 `rustfmt --check` | 通过 |
| CI YAML 解析检查 | 通过；没有执行真实 CI 发布 |
| `tsc -p src-ui/tsconfig.json --noEmit` | 通过 |
| 23 个修改的 UI 源码文件执行 ESLint | `--max-warnings 0` 通过；按项目规则忽略生成文件，没有据此声称未修改的全部 UI 已检查 |
| `vite build --config src-ui/vite.config.ts` | 通过，保留已有的大 chunk 提示 |
| Tauri Linux 调试程序构建 | `build --debug --no-bundle` 通过，无 Rust warning |
| 隔离 Xvfb / DBus 下启动调试程序 | 进程存活 12 秒，1280×800 窗口正确居中，主界面与协议弹窗完成渲染；不是容器或 QQ 登录检查 |
| CI 门禁与缓存脚本既有测试 | 29 通过、1 个显式启用的 Cargo 缓存集成检查未运行；临时 Git 仓库测试在沙箱外重跑通过 |
| Bot 运行组合、创建导航与身份页相关前端测试 | 4 个文件、35 个用例通过 |
| 身份页、Docker 部署区与 sudo 对话框相关前端测试 | 最终重跑 3 个文件、9 个用例通过，与上一组有重叠 |

矩阵中的支持声明与 `verify` 实现不能代替实机部署结果。

尚未据此证明以下场景全部通过：真实 SSH 主机安装 12 个组件、七个应用端完整联网安装与插件加载、QQ 图形会话及扫码登录、NapCat/SnowLuma 容器拉取与登录、不同发行版和 ARM 主机的全流程。

Linux 启动中的 `libayatana-appindicator` 弃用提示来自当前 Tauri 托盘依赖使用的系统库，见 [tray-icon 上游问题 #260](https://github.com/tauri-apps/tray-icon/issues/260)。它与 Rust 编译警告分开处理，保留系统诊断输出；单独一条弃用提示不能作为应用崩溃证据。

隔离图形检查禁用了 DBus 服务自动激活，因此会出现测试环境的 AT-SPI 服务缺失提示；该环境的 GitHub 版本请求也曾失败，保留了相应网络诊断。以上没有计作已解决的产品警告，也没有影响本次窗口渲染与进程存活检查。

## 原生托盘补充验证（2026-10-30）

Linux 使用 GTK / Cairo / Pango 承载共用托盘视图。XEmbed 兼容后端支持右键直出面板；系统菜单回退保留「打开托盘面板」，账号托盘保留「账号面板」和「未读预览」。面板显式拿焦点，通过 Escape / 失焦收起。Tauri 自带的 Linux 托盘不提供鼠标事件，兼容后端单独接收 GTK 信号，见 [Tauri 托盘 API](https://v2.tauri.app/reference/javascript/api/namespacetray/)。X11 面板按事件位置与工作区定位，纯 Wayland 面板由合成器安排位置。

当前 Ubuntu GNOME 的 AppIndicator 扩展直接在 Shell 内切换右键系统菜单，不向应用发送右键请求，因此现有接入无法把右键直接替换为自绘面板，见[扩展实现](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/indicatorStatusIcon.js)。[菜单实现](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/dbusMenu.js)中的 `AboutToShow` 是菜单布局更新通知，也可能在初始绑定时发生，不能用来取消系统菜单或当作右键事件；未加入此类会产生误弹和双菜单的钩子。

- `cargo check -p ncd-tauri --all-targets --locked --offline` 和 Linux debug 构建通过，无 Rust 编译警告；本次涉及的主题、图标路径、头像缓存、聊天托盘和退出请求既有测试共 17 项通过。
- 直接引用当前绘制源码的离屏检查覆盖 100% / 125% / 150%：中文、省略、圆角透明、头像预乘及采样正常，透明字体使用灰度抗锯齿。
- GTK 宿主检查通过 125% / 150% 实际尺寸及鼠标坐标换算、点击/键盘回调重入布局、Escape / 失焦收起、隐藏停止动画、销毁解除持有环及重新创建。
- 实际程序在隔离 X11 / D-Bus / StatusNotifierWatcher 中完成托盘注册、原生菜单读取；释放主界面后 WebKitWebProcess 退出，再从托盘打开自绘面板时未重建主窗口或 WebKitWebProcess。框架的 WebKitNetworkProcess 仍保留，不能宣称退出全部 WebKit 进程。隔离进程均已清理。

完整严格 Clippy 检查仍受项目既有告警阻挡，例如日志输出、截图实现中的 `expect` 和旧类型复杂度；未添加全局 lint 屏蔽。Wayland 桌面上的实际定位/焦点体验及真实 QQ 账号未读流程仍需实机确认。

## Linux 启动窗口尺寸补充（2026-10-30）

`commands/window.rs` 按显示器可用工作区的逻辑尺寸设置主窗，宽 80% / 高 85%，基准 1280×800；每侧至少留出 24 个逻辑像素，小屏同步降低最小尺寸。系统 DPI 只换算一次，保持用户选择的 WebView 界面倍率。启动与轻量模式重建共用该入口，普通恢复已有窗口不改用户调整过的尺寸。几何请求在首次显示前提交，居中使用目标尺寸，避免 GTK 异步配置回读旧尺寸。

- 两项几何回归通过，覆盖高分辨率、系统 200% 缩放、负坐标显示器及小工作区的最小尺寸与定位。
- Linux debug 构建通过，无 Rust 编译警告；本次未改前端渲染代码。
- 隔离 Xvfb / D-Bus 中启动实际新版程序：2560×1600 得到 2048×1360 窗口、1366×768 得到 1280×720 窗口、3840×2160 且系统 200% 缩放时得到 3072×1836 物理窗口（1536×918 逻辑尺寸）。三组均可见、居中且等待配置稳定后尺寸保持一致，测试进程与数据根均独立于用户会话。未据此声称已完成真实 Wayland 合成器的定位验收。
- 大屏场景额外等待 15 秒，确认完整首页与 CPU / RAM 图表正常渲染，先设置几何再显示未阻塞 WebKitGTK 的页面启动。隔离测试进程已清理。

## 托盘右键直出面板的社区调查（2026-10-31）

现有 AppIndicator 接入的限制只适用于该托盘后端。社区项目会按后端能力分流：[Telegram](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/platform/linux/tray_linux.cpp) 同时保留系统菜单和自绘菜单，收到 Context / 旧 X11 右键事件时打开自绘菜单；[Electron](https://www.electronjs.org/docs/latest/api/tray/) 的 Linux 托盘优先使用 SNI，但 `right-click` 事件仍只标注 Windows / macOS 支持，换用 Electron 本身不会消除这一限制。

Ubuntu GNOME 扩展另外提供 [XEmbed 托盘宿主](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/trayIconsManager.js)。本机 GNOME Wayland 会话已启用 legacy tray 且存在 XWayland；临时 GTK 探针确认图标真实嵌入，同进程的默认 display 和后建窗口仍保持 Wayland。做法是在 GTK 主线程打开 X11 display，仅在同步创建图标期间临时切换默认 display，然后立即恢复。依据 [GTK StatusIcon 源码](https://github.com/GNOME/gtk/blob/gtk-3-24/gtk/deprecated/gtkstatusicon.c)，单纯在 Wayland 上创建图标后再 `set_screen` 不能补建底层 X11 图标。

已在 `src-tauri/src/linux_tray.rs` 接入 XEmbed 兼容后端。GTK `embedded` 通知决定可见图标：成功嵌入后隐藏 SNI 图标，未嵌入或宿主消失时恢复 SNI 系统菜单；没有 X11 display 时直接保留原有行为。宿主在运行中出现时同样自动切换，无需轮询。主托盘和账号托盘同步更新图标、tooltip 及未读闪烁帧，账号关闭时移除对应 GTK 图标。各图标使用独立 WM_CLASS，避免 GNOME 将同进程的主图标与账号图标相互替换。

鼠标按下时阻止 GTK 的默认激活，在松开时派发一次点击，使用事件根坐标定位；键盘仍可激活图标和上下文菜单。`native_panel/window_linux.rs` 持久保存托盘 display，覆盖异步业务入口，并让原生面板单独使用 X11 screen、RGBA visual 与工作区；主应用默认 display 不变。Linux 账号菜单增加「未读预览」行，避免隐藏 SNI 后丢失原有入口。

图标创建作用域通过 RAII 临时选择 X11 display 和独立 `program_class`，随后恢复主应用设置。GTK 在构造 GtkWindow 时复制该类名，因此首次映射即使用正确 WM_CLASS；不在已实现的 GtkPlug 上调用 `set_name` 重新创建窗口，避免重复 dock 和 GDK 更新计数断言。GTK StatusIcon 已弃用，作为有条件的兼容路径使用，见 [GTK 文档](https://docs.gtk.org/gtk3/class.StatusIcon.html)。没有强制整个应用使用 X11，没有修改用户桌面设置，没有增加后台轮询或新的依赖。

最终 Linux debug 构建通过，无 Rust 编译警告。隔离 Xvfb / D-Bus 下实际产品在 `G_DEBUG=fatal-criticals` 中通过完整流程：无宿主时系统菜单正常；宿主出现后仅一次 dock，实际 WM_CLASS 第二项为 `main-tray`，SNI 切为 Passive；真实右键直出原生面板，Escape / 失焦收起；轻量模式下不重建主窗口或 WebKitWebProcess；宿主消失后 SNI 恢复 Active，系统菜单可重建主窗。最终日志无 GTK / GDK Critical。

相关既有 Rust 测试 17 项通过，未新增永久测试；Clippy 仍保留项目既有告警，未增加全局屏蔽。真实 GNOME Wayland 的完整用户点击与定位体验及真实 QQ 账号流程仍需实机确认；此前的 Wayland / X11 同进程探针只证明显示器共存。隔离测试进程均已清理。

## Linux Dock 图标与应用身份（2026-10-31）

Linux Dock 通过桌面入口解析应用图标，Wayland 下不能只依赖窗口 PNG。`window_icon.rs::prepare_application_identity` 在 GTK 初始化前将 GLib 程序名设为配置 identifier；正式配置启用 GTK App ID，使 Wayland `app_id` 和 GTK 应用 ID 均与 `com.napcatqq.desktop` 对齐。生成的可见桌面入口使用同值的 `StartupWMClass`；deb / AppImage 另包含隐藏的 `com.napcatqq.desktop.desktop` 兼容入口，图标沿用包内 hicolor 资源，保持一个可见启动项。

`pnpm run tauri:dev` 在用户 XDG 应用目录准备 `com.napcatqq.desktop.Development.desktop`，使用仓库内 PNG 的绝对路径，避免依赖已安装的正式包。开发配置关闭 GTK App ID，通过 `NCD_DEV_DESKTOP_ID` 向早期初始化提供独立开发身份；bundle identifier、单实例身份和数据根保持不变。入口标记 `NoDisplay=true`，不额外占用应用菜单，仍可供 Dock 解析；其 Exec 可重新启动原开发脚本，保留 CLI 参数转发与信号收尾。

- 开发脚本 5 项检查通过，其中 2 项覆盖 Linux 入口隔离、XDG 目录及 Exec 特殊字符；Linux 专用检查在其它系统跳过。私有 GIO 启动检查确认空格、引号、反斜杠、美元符号、百分号及反引号路径按原样解码。
- 开发模式实际 Wayland 协议确认 `app_id=com.napcatqq.desktop.Development`，X11 WM_CLASS 实例同值，窗口 `_NET_WM_ICON` 为 256×256。GIO 可解析开发入口及绝对 PNG 图标。
- 正式模式实际 Wayland 协议确认 `app_id` 与 GTK Application ID 均为 `com.napcatqq.desktop`；X11 WM_CLASS 实例同值，窗口图标为 256×256。检查通过后已清理隔离进程，未以协议检查代替 Dock 视觉截图验收。
- 最终 Linux debug DEB 构建通过，无 Rust 编译警告；包内桌面入口与图标文件经过校验。AppImage 配置复用相同桌面模板与兼容入口；未重跑完整 AppImage 打包，宿主桌面集成仍由安装器或集成器负责。
