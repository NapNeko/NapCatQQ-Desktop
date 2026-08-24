# 远端 Linux 安装库存（智能发现）

日期：2026-08-25  
状态：已批准，按此实现  
范围：远端 Linux Native；Docker 只入库不接管启停

## 问题

远端 NC / SL / QQ / Node / ncd-watch 的探测和启动都按死路径拼：

- NapCat：`/opt/QQ/.../napcat.mjs` 在则 System，否则一律 `$HOME/Napcat`（目录可以不存在）
- SnowLuma：永远 `$HOME/snowluma-remote/workspace`
- QQ：永远 `$HOME/Napcat/opt/QQ/qq`
- Node：永远 `…/snowluma-remote/workspace/node`
- ncd-watch：永远 `$HOME/ncd-watch`

官方 NapCat-Installer、系统包 `linuxqq`、自定义前缀、旧桌面路径 `$HOME/Napcat/snowluma-workspace` 都接不上。禁止用 `find /` / `locate` / 遍历他人家目录来补。

## 产品

强兼容：桌面自己装的、官方安装器、系统包、自定义目录（进程能反推时）都认。结果写入该主机的 `ServerProfile`。用户可覆盖；覆盖校验失败则报错，不静默改回默认。

NC 与 SL **共用一棵 QQ 树**（与现网一致）。什么都没发现时不挡添加主机；组件安装仍落到桌面默认路径。

## 原则

- 只做远端 Linux。本机 Windows 不改。
- 禁止扫盘：`find`、`locate`、递归 `$HOME`、读其他用户 `/home/*` 都不做。
- 一次 SSH 跑完探测，超时 8s；解析与选中在本机 Rust。
- **路径是真相**。现有 `RemoteLayout::{System,Rootless}` 只从选中的 QQ 安装根派生 sudo。
- 覆盖 > 档案里上次 `selected`（仍能校验）> 新探测选中。
- 探测脚本只回收路径，不回收 token / 密码类环境变量。

## 分层

| 层 | 落点 | 职责 |
| --- | --- | --- |
| domain | `crates/ncd-domain/src/remote_inventory.rs` | 强类型 + ts-rs |
| runtime | `crates/ncd-runtime/src/remote/inventory.rs` | 探测脚本、解析、选中、Host I/O |
| server | `ServerProfile` 增字段；`update_server` 原样落盘 | 持久化 |
| component 工厂 | `build_component_for_host` | 用 `selected` 路径实例化，不再只靠 enum 拼 |
| NC/SL backend | `probe_remote_napcat_layout` / `probe_remote_snowluma_layout` | 改为读库存；不再各自猜路径 |
| tauri | 刷新命令 + 进程内缓存 | 薄壳 |
| UI | 远端主机 / 组件页 | 展示、覆盖、重新发现 |

`ncd-component` 仍只在**已知路径**上 detect/install。发现不是 Component Action。

## 数据模型

`servers.json` **不升 schema 版本**。新字段 `#[serde(default, skip_serializing_if = "Option::is_none")]`，旧档案当 `None`。

`ServerProfile` 增加：

- `path_overrides: Option<RemotePathOverrides>`
- `inventory: Option<RemoteInventory>`

### RemotePathOverrides

全部为可选绝对 POSIX 路径：

- `qqInstallBase`：含 `opt/QQ` 的根（rootless 为 `$HOME/Napcat`，系统为 `/`）
- `napcatRoot`：含 `napcat.mjs` 的目录
- `snowlumaDir`：含 `index.mjs` 的 framework 根
- `nodeBin`：`node` 可执行文件
- `ncdWatchRoot`：ncd-watch 安装根

### RemoteInventory

- `v: u32`，本规格为 `1`
- `probedAt`：ISO-8601
- `home`
- `items: Vec<RemoteInventoryItem>`
- `selected: RemoteSelectedPaths`

### RemoteInventoryItem

- `kind`：`qq` / `napcat` / `snowluma` / `nodejs` / `ncd_watch` / `docker_container`（含连续大写的 variant 显式 `serde(rename)`，对齐 `ComponentId`）
- `root`
- `source`：`userOverride` / `desktopOwned` / `officialInstaller` / `systemPackage` / `pathLookup` / `process`
- `verified`：指纹通过
- 可选细节：`qqBin`、`napcatMjs`、`loadNapcatJs`、`indexMjs`、`runtimeJson`、`nodeBin`、`dockerName`

### RemoteSelectedPaths

启动与组件工厂只读这里：

- `home`
- `qqInstallBase`、`qqBin`、`napcatRoot`、`snowlumaDir`、`snowlumaWorkspace`、`nodeBin`、`ncdWatchRoot`：均为 `Option`；未发现则为 `None`
- `needsSudo`：`qqInstallBase` 为 `Some("/")` 或 `qqBin` 为 `Some("/opt/QQ/qq")`；QQ 未发现时为 `false`

SnowLuma workspace：若 `snowlumaDir` 以 `/snowluma` 结尾，workspace 为其父目录；否则 workspace = `snowlumaDir`（扁平布局）。便携 Node 仍先看 `{workspace}/node/bin/node`。

## 指纹（校验用，不是搜索键）

| 组件 | 通过条件（AND） | 不够的信号 |
| --- | --- | --- |
| QQ | `{base}/opt/QQ/qq` 可执行 **且** `{base}/opt/QQ/resources/app/package.json` 存在 | 仅有 `~/.config/QQ`（那是用户数据） |
| NapCat | `{qqApp}/loadNapCat.js` 存在 **且** `{qqApp}/app_launcher/napcat/napcat.mjs` 存在 | 单独的 `napcat.mjs` 在别的目录 |
| SnowLuma | `{dir}/index.mjs` **且**（`{dir}/config/runtime.json` 含 `webuiPort` 或 `{dir}/config/webui.json` 存在） | 单独的 `index.mjs` |
| Node | 路径可执行，`node --version` 能跑（探测脚本里只 `test -x`，版本留给 component detect） | PATH 里同名无关二进制：`command -v` 命中后仍要 `test -x` |
| ncd-watch | `{root}/ncd-watch` 可执行 | 仅有目录名 |
| Docker | `docker ps -a --filter name=ncbot-` 的容器名 | 不作为 Native 启动根 |

`{qqApp}` = `{base}/opt/QQ/resources/app`。

探测脚本**不得**读取或回传：`SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD`、`NAPCAT_WEBUI_*`、`WEBUI_TOKEN`、`VNC_PASSWD`。进程阶段最多读 `NAPCAT_WORKDIR`（只要路径）。

## 探测管道

一次 `sh -c`，stdout 行协议，本机解析。失败/超时时保留旧 `inventory`，UI 标可能过期。

顺序（后者只补充前者没有的 **kind+root**）：

1. **覆盖**：对每个非空 override 做指纹 `test`。
2. **白名单候选**（每条 O(1) `test -e` / `test -x`）：
   - QQ/NC：`$HOME/Napcat/opt/QQ`、`/opt/QQ`
   - SL：`$HOME/snowluma-remote/workspace/snowluma`、`$HOME/Napcat/snowluma-workspace/snowluma`
   - Node：`$HOME/snowluma-remote/workspace/node/bin/node`、`$HOME/Napcat/usr/node/bin/node`
   - ncd-watch：`$HOME/ncd-watch/ncd-watch`
3. **包管理器**：`dpkg -L linuxqq` 或 `rpm -ql linuxqq` 中匹配以 `/qq` 结尾且 `test -x` 的路径，反推 `installBase`（`…/opt/QQ/qq` → 上两级的父，系统包则为 `/`）。
4. **`command -v`**：`node`、`docker`、`qq`（`qq` 命中后再用指纹确认是否 LinuxQQ 树）。
5. **活进程**（`ps -u "$USER" -o pid=,args=` 或只读 `/proc/[0-9]*`，不是扫盘）：
   - args 含 `napcat.mjs` 或 `loadNapCat.js` → 从 `/proc/PID/exe` 或 cwd 反推 QQ 树
   - args 含 `index.mjs` 且 cwd 能通过 SnowLuma 指纹
   - `/proc/PID/environ` 中的 `NAPCAT_WORKDIR=` 路径
6. **Docker**：若 `command -v docker` 成功，列出 `ncbot-*` 容器名入库。Native 启动忽略这些项。

### stdout 协议

无 `jq` / Python 依赖。UTF-8 文本：

```
NCD_INV_V=1
HOME=/home/u
HIT kind=qq source=desktopOwned verified=1 root=/home/u/Napcat qqBin=/home/u/Napcat/opt/QQ/qq
HIT kind=napcat source=officialInstaller verified=1 root=/home/u/Napcat/opt/QQ/resources/app/app_launcher/napcat napcatMjs=.../napcat.mjs loadNapcatJs=.../loadNapCat.js
```

值禁止含空格（路径若含空格：探测脚本用 `printf %q` 不采用；**约定远端路径不含空白**，含空白的覆盖由本机直接 `test` 单条命令，不走 HIT 行）。解析器忽略未知键、忽略空行、忽略非 `HIT`/`HOME`/`NCD_INV_V` 行。

## 选中规则

对每个 kind 选一条 `verified=true` 的 item：

1. `source=userOverride`
2. 档案中已有 `selected` 且此次仍 verified（同 root）
3. `desktopOwned`
4. `officialInstaller`（`$HOME/Napcat`）
5. `systemPackage`（`/opt/QQ`）
6. `pathLookup` / `process`

系统和 rootless 同时有 NapCat 时：默认 rootless（当前 SSH 用户可写）；另一棵留在 `items` 里供 UI 改选（改选即写入 `path_overrides.qqInstallBase`）。

QQ 只有一棵进入 `selected`。启动 NC 或 SL 都用它。`package.json` 的 `main` 切换仍走现有 `remote_qq_entry`，发现阶段不改入口。

未发现某 kind：`selected` 对应字段为 `None`。组件安装工厂此时回落到桌面默认路径（与现在 factory 拼法相同），以便「空机第一次安装」。

## 何时探测

| 时机 | 行为 |
| --- | --- |
| SSH 连接测试成功 | 探一次并写入档案 |
| 远端组件页打开 | 无库存或 `probedAt` 超过 6 小时才探 |
| 「重新发现」 | 强制探 |
| Native 启动前 | `selected` 路径校验失败则再探一次；仍失败则启动错误 |
| Bot 状态轮询 | **不探** |

进程内 `host_probe_cache` 改为缓存 `RemoteInventory`（可与档案并存）。`RemoteHostProbe` 保留为派生视图：`home` + `layout`（`needsSudo` 则为 System，否则 Rootless），避免一次改光所有调用方；新代码走 `selected`。

## 消费者改造

- `probe_remote_napcat_layout`：用 `selected.qqInstallBase` + `napcatRoot`；无 NapCat 时再回落「按 selected QQ 树推导」，不再「没有 /opt 就当 rootless」。
- `probe_remote_snowluma_layout`：用 `selected.snowlumaDir` / `nodeBin` / `qqBin`。
- `build_component_for_host`：`BuildComponentCtx` 增加 `selected: Option<&RemoteSelectedPaths>`；有则按其路径建 Component。
- 启动失败文案带上实际探测到的路径和 `source`，不再写死「请先在组件页安装到 $HOME/Napcat」。

## IPC / UI

- 新命令 `refresh_remote_inventory(serverId) -> RemoteInventory`（SSH + 写档案）。
- 覆盖走现有 `update_server`（profile 上改 `pathOverrides`）。保存覆盖后应再 `refresh` 一次以重算 `selected`。
- 改 IPC 类型后跑 `pnpm run ts-bindings`。

UI（远端主机卡片或组件页，不新开顶层路由）：

- 列出 `items`：路径 + 来源 + 是否选中
- 高级：手填 overrides
- 「重新发现」
- 空库存：说明将按桌面默认路径安装，不挡操作
- 覆盖校验失败：红色错误，保留用户输入

## 错误

- 探测超时 / SSH 失败：保留旧库存；刷新命令返回错误字符串
- 覆盖路径指纹失败：该 HIT `verified=0`；若用户仍保存，启动/安装报「覆盖路径无效，请改正或重新发现」
- 部分 kind 缺失：成功返回，缺失项在 UI 显示未发现

## 测试

纯函数（无 SSH）：

- 解析 HIT 行：正常、缺字段、未知键、空 HOME
- 选中表：覆盖优先、rootless vs system、旧 selected 仍有效、进程补自定义前缀
- 指纹：SL 无 `runtime.json`/`webui.json` 不得过；仅 `~/.config/QQ` 不得当安装根
- `needsSudo` 派生
- servers.json 旧档案缺字段仍能反序列化

Mock Host：白名单命中、dpkg 输出、进程 cmdline 各一条。

## 不做

- 本机 Windows 发现
- `find` / `locate` / 扫整盘 / 扫其他用户
- 把已有 Docker 容器收编进 Native 启停
- 自动搬迁安装目录
- 发现阶段改 QQ `package.json` main
- 收集密钥类环境变量
- 升 `servers.json` 兼容版本号
