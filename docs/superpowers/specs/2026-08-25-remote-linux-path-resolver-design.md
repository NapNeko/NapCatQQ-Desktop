# 远端 Linux Native 路径解析器

日期：2026-08-25  
状态：已批准，按此实现  
范围：远端 Linux Native 的路径**消费**（组件安装、NC/SL 启停、前端门禁）  
前置：[远端 Linux 安装库存](./2026-08-25-remote-linux-inventory-design.md)（发现与选中，已落地）

## 问题

库存已经把 `RemoteSelectedPaths` 定为路径真相，但消费侧没有强制走它。现在至少四套模型各自拼路径：

| 层 | 现状 |
| --- | --- |
| 库存 | `selected` 是对的（含 `qqInstallBase` / `qqBin`） |
| 组件工厂 | 有 `selected` 字段就用，缺了立刻回落 `{home}/Napcat`、`{home}/snowluma-remote/workspace` |
| NapCat 启动 | 自己的 `RemoteNapcatLayout::{System,Rootless}`；有 selected 时能用，无 selected 仍按 home 猜 |
| SnowLuma 启动 | `layout.qq_bin` 已来自库存，冷启动补 `package.json` 和 spawn 仍写死 `{home}/Napcat` |
| 前端门禁 | 再实现一遍完整包 vs lite，和 Rust `infer_snowluma_linux_package` 平行 |

真机表现（kunming-4-8，SSH 用户 root，系统 QQ 在 `/opt/QQ`）：

> SnowLuma 冷启动前确保纯净 QQ 入口失败：未找到 `/root/Napcat/opt/QQ/resources/app/package.json`

库存选中的是 `/opt/QQ`，补丁打到了一棵不存在的默认树上。前端可以显示「能启动」，后端去补另一条路径。

根因不是「没发现」，是 **发现结果没有成为启动/安装的唯一输入**。

## 产品

远端 Linux Native 上，启动、detect、安装、门禁看到的路径必须是同一份 `selected` 的派生，不允许再按 `$HOME/Napcat` 各自猜测。

- 已发现 QQ/SL：启动必须打选中的那棵树。
- 空机（对应字段 `None`）：不挡加主机；组件**安装**落到桌面默认路径。
- 启动缺必需路径：报错，带上 `home` 和实际 `selected` 字段，**禁止**静默回落默认树。

NC 与 SL 仍共用一棵 QQ 树；`package.json` `main` 切换仍走现有 `remote_qq_entry`，只改传入的 `install_base`。

## 原则

- 只做远端 Linux Native。本机 Windows、Docker 卷/compose 路径不改。
- **路径模板只存在一处**（domain 纯函数）。Backend / 工厂 / 入口补丁禁止再写 `{home}/Napcat`、`opt/QQ/resources/app/package.json` 这类字面量。
- `RemoteLayout::{System,Rootless}` 和 `RemoteNapcatLayout` 只是 `needs_sudo` 的视图，不再当路径来源。
- 覆盖 > 档案 `selected`（仍能校验）> 新探测选中。本规格不改选中规则，只改消费。
- 前端不拼接远端绝对路径。门禁只读库存字段。
- 发现阶段仍不改 `package.json` main。

## 分层

| 层 | 落点 | 职责 |
| --- | --- | --- |
| domain | `crates/ncd-domain/src/remote_paths.rs`（新） | 路径模板、join/反推、默认安装根、从 `selected` 派生具体文件、推断 SL 包形态 |
| domain | `RemoteInventory` 增字段 | 探测时写入 `snowlumaLinuxPackage`，前端不再平行推断 |
| runtime 库存 | `ncd-runtime/src/remote/inventory.rs` | 继续探测/选中；join QQ bin 改调 domain；纯函数迁走后这里 re-export 或删除重复 |
| 组件工厂 | `build_component_for_host` | 有 selected 字段用派生路径；缺字段才用 `desktop_default_install_paths(home)` |
| NC backend | `remote_native_napcat_session` | 启动只吃 `selected` / 派生路径；删 `{home}/Napcat` 回落 |
| SL backend | `remote_snowluma/{layout,backend,orchestrator}` | 布局带 `qq_install_base`；补丁和 spawn 用它 |
| 入口补丁 | `ncd-component/src/remote_qq_entry.rs` | 仍收 `install_base`；内部 join 改调 domain |
| 前端 | 门禁 / `remote-direct-run-deps.ts` | 读 `inventory.selected` + `inventory.snowlumaLinuxPackage` |

`ncd-backend-*` 不能依赖 `ncd-runtime`，所以解析器必须在 **domain**（零 I/O）。

## 数据模型

`servers.json` **不升 schema 版本**。新字段 `#[serde(default, skip_serializing_if = "Option::is_none")]`。

### 仍只持久化根路径

`RemoteSelectedPaths` 字段不变：`home`、`qqInstallBase`、`qqBin`、`napcatRoot`、`snowlumaDir`、`snowlumaWorkspace`、`nodeBin`、`ncdWatchRoot`、`needsSudo`。

不把 `package.json` 等具体文件写进档案。

### `RemoteInventory` 增加派生字段

```text
snowlumaLinuxPackage: Option<SnowLumaLinuxPackage>
```

探测结束时用 domain 的 `infer_snowluma_linux_package(&selected)` 写入。旧档案缺字段视为 `None`；前端把 `None` 与「尚未安装」一样按完整包处理（与现 Rust 默认 `Full` 一致）。

### 运行时派生（不落盘）

```text
RemoteLinuxDerivedPaths
  home
  qq_install_base, qq_bin, qq_app_dir, qq_package_json
  load_napcat_js, napcat_mjs, napcat_root, napcat_config_dir
  snowluma_dir, snowluma_workspace, node_bin
  ncd_watch_root
  needs_sudo
  snowluma_linux_package
```

全部 `Option<String>`（除 `home`、`needs_sudo`、`snowluma_linux_package`）。对应 `selected` 根为 `None` 则派生也为 `None`。此结构不必 ts-rs，除非实现时发现前端真的要展示具体文件；默认前端只读 `selected` + `snowlumaLinuxPackage`。

## Domain API

模块：`crates/ncd-domain/src/remote_paths.rs`。POSIX，`/` 归一，禁止反斜杠残留。

### 拼接（`base == "/"` 时不得出现 `//opt/...`）

现网已踩过：系统包 `root=/` 会拼出 `//opt/QQ/qq`，`/proc/pid/exe` 比对永远失配。

```text
join_under(base, rel) -> String
  base="/" + rel="opt/QQ/qq"     => "/opt/QQ/qq"
  base="/home/u/Napcat" + 同上   => "/home/u/Napcat/opt/QQ/qq"
```

固定相对段（只在本文件出现字面量）：

| 函数 | 相对 `qq_install_base` |
| --- | --- |
| `qq_bin` | `opt/QQ/qq` |
| `qq_app_dir` | `opt/QQ/resources/app` |
| `qq_package_json` | `opt/QQ/resources/app/package.json` |
| `load_napcat_js` | `opt/QQ/resources/app/loadNapCat.js` |
| `napcat_mjs` | `opt/QQ/resources/app/app_launcher/napcat/napcat.mjs` |

`napcat_config_dir(napcat_root)` = `{napcat_root}/config`（`napcat_root` 已是含 `napcat.mjs` 的目录）。

从 runtime 迁入（行为保持）：

- `qq_install_base_from_qq_bin`：`…/opt/QQ/qq` → 上两级的父；`/opt/QQ/qq` → `/`；`~/.config/QQ` 用户数据 → `None`
- `snowluma_workspace_from_dir`：以 `/snowluma` 结尾则父目录，否则 dir 自身
- `needs_sudo_for_qq`：`qq_install_base == "/"` 或 `qq_bin == "/opt/QQ/qq"`
- `is_bundled_snowluma_node` / `infer_snowluma_linux_package`：从 `action_policy` 迁入

### 默认安装根（仅安装，不用于启动回落）

`desktop_default_install_paths(home) -> RemoteSelectedPaths`

| 字段 | 值 |
| --- | --- |
| `home` | 入参（去尾 `/`，根仍为 `/`） |
| `qq_install_base` | `{home}/Napcat` |
| `qq_bin` | join_under(base, `opt/QQ/qq`) |
| `napcat_root` | join_under(base, `opt/QQ/resources/app/app_launcher/napcat`) |
| `snowluma_workspace` | `{home}/snowluma-remote/workspace` |
| `snowluma_dir` | `{workspace}/snowluma` |
| `node_bin` | `{snowluma_dir}/node`（按完整包默认） |
| `ncd_watch_root` | `{home}/ncd-watch` |
| `needs_sudo` | `false` |

`home` 为空则函数返回 `Err`，调用方不得用 `"/Napcat"`。

### 从 selected 派生

`derive_remote_linux_paths(selected) -> RemoteLinuxDerivedPaths`

1. 规范化 `home`。
2. `qq_install_base`：selected 有则用；否则若有 `qq_bin` 则 `qq_install_base_from_qq_bin`。
3. 有 `qq_install_base` 则填 `qq_bin`（selected 优先）及 app / package.json / loadNapCat.js / napcat.mjs。
4. `napcat_root`：selected 有则用；否则若有 QQ 树则用派生的 napcat 目录。
5. `napcat_config_dir`：有 `napcat_root` 才填。
6. SL / node / ncd-watch：原样取 selected；`snowluma_workspace` 缺则从 `snowluma_dir` 反推。
7. `needs_sudo` / `snowluma_linux_package`：用上面的纯函数。

### 启动用：缺了就错

先 `derive_remote_linux_paths(selected)`，再取派生字段。禁止在 `require_*` 里再拼 `{home}/Napcat`。

```text
require_qq_install_base(selected) -> Result<String, String>
require_snowluma_dir(selected) -> Result<String, String>
require_napcat_root(selected) -> Result<String, String>   // 仅 NC Native；无 napcatRoot 时可用 QQ 树派生目录，文件是否存在仍由 ensure_for_napcat 校验
```

错误文案必须包含 `home` 和已有的 selected 字段，例如：

```text
远端未发现 QQ 安装树（home=/root, qqInstallBase=None, qqBin=None）。
请在组件页安装或填写覆盖路径后重新发现。
```

禁止出现「请先在组件页安装到 $HOME/Napcat」这种写死默认树的句子。

## 启动 vs 安装

| 场景 | 输入 | 缺 QQ / SL / NC 根时 |
| --- | --- | --- |
| Native **启动** | 当前库存 `selected`（过期则先按库存规格再探一次） | `require_*` 失败，**不**调用 `desktop_default_install_paths` |
| 组件 **安装 / detect** | `selected` 对应字段，否则默认安装根 | 用默认根实例化 Component，以便空机第一次安装 |
| 前端远程直接运行门禁 | `inventory.selected` 字段是否 `Some` | 提示未发现，引导组件页或重新发现；不本地拼路径 |

启动前校验（对齐库存规格）：`selected` 路径指纹失败则再探一次；仍失败则启动错误。本规格不改探测脚本。

## 消费者改造

### SnowLuma

`RemoteSnowLumaLayout` 增加 `qq_install_base: String`（启动布局里这是必填；构造失败则不要产出 layout）。

- `layout_from_selected`：`require_qq_install_base` + `require_snowluma_dir`；`qq_bin` 来自 selected 或 join。
- `layout_from_selected_or_probe`：只允许补 **SL 安装目录 / node** 的候选探测（进程停了库存会丢 SL 路径）。QQ 若 selected 没有，可对 `{default qq_bin, /opt/QQ/qq}` 做 `test -x`；一旦命中必须写入 `qq_install_base`（反推），**禁止**命中 `/opt/QQ/qq` 后仍把 install_base 设成 `{home}/Napcat`。
- `backend.rs` 冷启动 `ensure_for_native`：传入 `layout.qq_install_base`，删除 `format!("{}/Napcat", layout.home)`。
- `orchestrator.rs` spawn QQ：`QQComponent` 的 install_base 同样用 `layout.qq_install_base`。

### NapCat

- `napcat_paths_from_selected` 保持：无 `qq_install_base` 则错。
- `RemoteNativeLaunchTranslator::layout`：有 `selected` 只用它；无 `selected` 时不得再 `probe_remote_napcat_layout` + `{home}/Napcat`。改为「调用方必须注入 selected」或启动错误。
- `session.rs` / `native_deployment_adapter/remote.rs` 里按 `RemoteNapcatLayout::Rootless => {home}/Napcat` 的回落删除，改为 `require_qq_install_base` 或已缓存的 install_base。

`RemoteNapcatLayout` 可留作 `needs_sudo` 视图，不再用于拼路径。

### 组件工厂

`resolve_napcat_base` / SL workspace / Node 安装目录：

1. `ctx.selected` 对应字段非空 → 用它（QQ 无 base 有 bin 时先反推）。
2. 否则 `desktop_default_install_paths(remote_home)` 的对应字段。
3. 禁止在 factory 内 `format!("{}/Napcat", home)`。

`snowluma_linux_extra_detect_dirs` 的候选列表与库存白名单重复。detect 额外目录允许保留为「已知候选」，但必须调用 domain 的默认/候选函数，不在 factory 再写一份字面量。候选函数可以是：

```text
snowluma_install_candidates(home) -> Vec<String>
qq_bin_candidates(home) -> Vec<String>   // default qq_bin, "/opt/QQ/qq"
```

与库存探测白名单同一实现（库存脚本仍是 shell；Rust 侧候选列表与脚本字符串同源或测过对齐）。

### `remote_qq_entry`

`set_remote_qq_package_main(host, install_base, main)` 签名不变。内部 `qq_package_json` 改为 domain `qq_package_json(install_base.as_posix())` 再转 `HostPath`。调用方必须传入解析后的 base。

### 前端

- `inferSnowLumaLinuxPackageFromInventory`：改为读 `inventory.snowlumaLinuxPackage`；`None` → `'full'`。删除与 Rust 平行的 `isBundledSnowlumaNode` 路径拼接（测试改为断言字段透传）。
- 远程直接运行门禁：
  - NapCat：`selected.qqInstallBase` 与 `selected.napcatRoot` 都有才算路径就绪。
  - SnowLuma：`selected.qqInstallBase` 与 `selected.snowlumaDir` 都有。
  - 完整包 vs lite：只看 `inventory.snowlumaLinuxPackage`；lite 才把 Node.js 列入依赖链。
  - **noVNC 不在 selected 里**，继续走组件 detect，本规格不把 VNC 路径纳入解析器。
- UI 文案可以出现「默认装到 $HOME/Napcat」；运行时路径字符串只来自 IPC。
- 改 IPC 类型后跑 `pnpm run ts-bindings`。

## 错误

- 启动缺路径：`require_*` 的字符串，含 home 与 selected 摘要。
- `package.json` 不存在：现有 `remote_qq_entry` 文案，路径必须是**选中树**上的文件（例如 `/opt/QQ/resources/app/package.json`），不得再是 `/root/Napcat/...`。
- 覆盖无效：沿用库存规格（启动/安装报覆盖路径无效）。
- 探测失败：沿用库存规格（保留旧库存）。

## 测试

纯函数（`ncd-domain`，无 SSH）：

- `join_under("/", "opt/QQ/qq") == "/opt/QQ/qq"`（回归 kunming `//opt/QQ/qq`）
- `qq_install_base_from_qq_bin("/opt/QQ/qq") == Some("/")`
- `qq_install_base_from_qq_bin("/data/qq/opt/QQ/qq") == Some("/data/qq")`
- `qq_install_base_from_qq_bin` 对 `~/.config/QQ` 为 `None`
- `derive`：只有 `qq_bin=/opt/QQ/qq` 时补上 `qq_install_base=/` 和 `qq_package_json=/opt/QQ/resources/app/package.json`
- `derive`：`qq_install_base=/home/u/Napcat` 时 `qq_bin` 为 `{base}/opt/QQ/qq`
- `desktop_default_install_paths("/root")` 的 QQ 根是 `/root/Napcat`，且 **derive 结果不得被启动测试当成回落**
- `infer_snowluma_linux_package`：自带 `{dir}/node` → Full；有 dir 无 bundled node → Lite；无 dir → Full
- 旧 `RemoteInventory` JSON 无 `snowlumaLinuxPackage` 仍能反序列化

Backend / 工厂（现有 mock host 风格）：

- SL `layout_from_selected`：`qq_bin=/opt/QQ/qq` → `qq_install_base=/`；`ensure_for_native` / spawn 使用该 base（断言不再出现 `{home}/Napcat`）
- SL 冷启动在 selected 指向 `/data/qq` 时，补丁路径为 `/data/qq/opt/QQ/resources/app/package.json`
- NC `napcat_paths_from_selected` 无 base 返回含 home 的错误，不返回 `{home}/Napcat`
- factory：`selected=None` 时 QQ 装到默认 `{home}/Napcat`；`selected.qq_install_base=/` 时用 `/`，不得改回 `{home}/Napcat`

前端：

- 门禁：库存有 `/opt/QQ` 选中则不因「默认路径未装」拦截
- `snowlumaLinuxPackage: 'lite'` 才要求 nodejs；`'full'` / `null` 不要求

## 实现顺序

1. domain：`remote_paths.rs` + 单测 + `RemoteInventory.snowlumaLinuxPackage`；从 runtime/action_policy 迁纯函数，旧入口改调 domain。
2. `ts-bindings`。
3. SL layout / backend / orchestrator 改用 `qq_install_base`。
4. NC layout 回落删除。
5. 工厂改用 domain 默认根。
6. `remote_qq_entry` join 改调 domain。
7. 前端门禁改读库存字段。
8. 全仓 Grep：`ncd-backend-*`、`ncd-runtime` 启动路径、`src-ui/core` 中不应再有为了**运行**而写的 `{home}/Napcat` / `snowluma-remote/workspace` 拼接（测试、默认函数、UI 说明文案除外）。

## 不做

- 本机 Windows 路径统一
- Docker 容器 / named volume / compose 路径
- 把具体文件路径写入 `BotConfig` 或 `servers.json`
- 发现阶段改 QQ `package.json` main
- `find` / `locate` / 扫盘
- 新建 `ncd-paths` crate（解析器放 domain 即可）
- 把 noVNC 纳入 `RemoteSelectedPaths`
- 升 `servers.json` 兼容版本号
