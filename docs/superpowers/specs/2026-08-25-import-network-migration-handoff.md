# 导入网络配置迁移 —— 调研交接（补全）

日期：2026-08-25
状态：调研完成，待实现（本文接续被截断的同名文档）
范围：导入已有远端 Bot 时，把框架侧网络配置反向填进桌面 `ConnectConfig`（及同文件内的少量附属字段）

## 0. 产品位置

已落地的发现链：

本机 SSH config 导入主机 → 库存探测组件 → 从组件/容器指纹发现 Bot → 导入并 reconcile 接管状态机。

缺的是 **Bot 身份之外的运行配置**。当前 `toBotConfig()` 只用 `createDefaultBotConfig()` 填空壳 `connect`。用户看到的「连接」页是空的，启动后桌面会按空配置 **覆盖** 远端已有 OneBot 通道（Native 启动必走 renderer）。这比「看不见」更严重。

本文只补 **网络配置（及同文件附属字段）**。不迁 WebUI 密码、VNC、compose `.env`、端口映射。

---

## 1. 已完成：SnowLuma 远端 Native

| 环节 | 落点 |
| --- | --- |
| 反向解析 | `crates/ncd-deploy/src/backend_config_renderer.rs` → `parse_snowluma_onebot_connect` |
| 远端读取 | `crates/ncd-backend-snowluma/src/remote_snowluma/config.rs` → `read_remote_onebot_connect(host, snowluma_dir, qq)` |
| 门面 | `crates/ncd-runtime/src/lib.rs` `pub use …read_remote_onebot_connect` |
| Tauri | `src-tauri/src/commands/servers.rs` → `fetch_remote_onebot_connect(server_id, qq_id)`（**写死 SL Native**：从 `inventory.selected.snowluma_dir` 拼路径） |
| 前端服务 | `botService.fetchRemoteOnebotConnect` |
| 对话框 | `ImportRemoteBotsDialog.runImport`：仅 `backend===snowluma && deployment===native` 时拉取，只赋 `cfg.connect` |

键名对齐（已做）：`accessToken→token`、`messageFormat→messagePostFormat`、`wsServers/wsClients→websocketServers/websocketClients`、`enableWebSocket→enableWebsocket`、`role` 小写→PascalCase。`statusCommand.enabled/swallow/cooldownSeconds` 已解析，**但读取器丢弃**（`.map(|(connect, _)| …)`），前端也没赋 `cfg.statusCommand`。

单测：`onebot_import_tests` 2 例。

### 1.1 SL 解析器还缺的键（随本任务修）

正向渲染（`SnowLumaConfigRenderer`）还有两处，反向没对齐：

| 远端键 | 桌面键 | 现状 |
| --- | --- | --- |
| `enabled` | `enable`（`NetworkBaseFields`，默认 true） | 未改名。条目仍能反序列化，开关语义靠默认值，不是空列表原因 |
| `reconnectIntervalMs` | `reconnectInterval` | 未改名。单位都是 ms（桌面默认 30000），丢了会回落到默认 |

`musicSignUrl` 在 onebot 顶层，解析器完全没取。

---

## 2. Bug#0：迁移成功但「连接」页是空的

### 2.1 现象（原窗口实证）

kunming `2703401480` 勾选迁移（当时文案；**现 UI 没有独立勾选，Native SL 总会尝试**），InfoBar 成功，`bot.json` 里 `httpServers`/`websocketServers` 完整，端口与远端 `onebot_*.json` 一致。存储与 serde round-trip 正常。

### 2.2 根因（已用代码核对，不再是猜测）

**H1 成立，query key 与原文档不一致。**

| 项 | 真实值 |
| --- | --- |
| 配置查询 key | `['botConfig', botId]`（`useBotConfig.ts` / `useBotConfigsMap.ts`） |
| 导入成功后 invalidate | **只** `botSnapshotsKey`，**没有** `['botConfig']` |
| 列表页缓存 | `useBotConfigsMap`：`staleTime: 60_000` |
| 配置页水合 | `BotConfigPage.next.tsx`：`formHydratedForBotRef` **每个 botId 只灌一次** |
| `normalizeLoadedConfig` | 只补 `statusCommand` 默认和 `runtime_target` 规范化，**不滤连接条目**（H2 排除） |
| `ConnectionsTab` | `totalCount` 对五类数组求和，无 backend 过滤已有条目（H3 排除） |

典型复现：同 QQ 先空壳导入 → 列表把空 `connect` 缓存 60s → 删除后再导入（删除若未 invalidate `botConfig`，key 仍是空数据）→ 配置页用陈旧缓存水合一次 → 磁盘已是新数据，UI 仍空。

首次导入、从未缓存过该 botId 时，按理应能看见。仍建议无条件修，避免列表 60s stale + 水合守卫叠加。

### 2.3 修复（小，和 NC/Docker 同一 PR）

1. `ImportRemoteBotsDialog` 成功分支追加：
   `queryClient.invalidateQueries({ queryKey: ['botConfig'] })`
2. 配置页：已水合且 **未 dirty** 时，允许 `loadedConfig` 更新后再灌一次（或 `dataUpdatedAt` 变化则重灌）。dirty 仍交给现有离开拦截。
3. 不要用 `['bot-config']`（原文档写错）。

---

## 3. 四格矩阵（要补齐的）

库存只发现 `ncbot-<qq>` / `slbot-<qq>` 与 Native 配置文件名。导入行带 `backend`、`deployment`、`dockerName?`、`serverId`。

| | Native | Docker |
| --- | --- | --- |
| SnowLuma | **已读文件**；解析缺键 + 丢 `statusCommand`/`musicSignUrl` | **未做**。配置在 **named volume**，不是 host 目录 |
| NapCat | **未做**。文件形状与桌面 `ConnectConfig` 同名 | **未做**。host bind 目录可读 |

失败策略与现 SL 一致：**不阻断导入**，InfoBar 累计成功数 / 失败原因。

不迁：`napcat_<qq>.json`（fileLog / packetBackend / bypass / o3HookMode）、`webui.json`、compose 端口。这些不是「连接」页，另开任务。

同文件内要迁（推荐，一次 SSH 读完）：

- 所有：`connect`
- SL：`statusCommand`、`musicSignUrl`
- NC：`musicSignUrl`、`enableLocalFile2Url`、`parseMultMsg`（进 `bot` / `advanced`）

---

## 4. NC Native

### 4.1 路径（不要用 install_base 硬拼）

库存 `emit_nc_bots` 扫的是 **`{napcat_root}/config/onebot11_*.json` 与 `napcat_*.json`**。`napcat_root` 可以是 QQ 树内目录，也可以是进程环境 `NAPCAT_WORKDIR`（自定义工作目录）。

权威路径：

```
{inventory.selected.napcat_root}/config/onebot11_{qq}.json
```

`napcat_root` 缺失时才回退：

```
{qq_install_base}/opt/QQ/resources/app/app_launcher/napcat/config/onebot11_{qq}.json
```

后者等于现有私有函数 `napcat_config_dir(install_base)`（`launch.rs:100`）。**优先 `selected.napcat_root`。**

已有内核：`read_existing_napcat_config`（`launch.rs:141`，PathNotFound 跳过）。抽出/并列一个公开读取器即可，不要复制一份 SSH 读文件。

### 4.2 文件形状（`NapCatConfigRenderer::build_onebot_payload` 的逆）

```json
{
  "network": {
    "httpServers": [],
    "httpSseServers": [],
    "httpClients": [],
    "websocketServers": [],
    "websocketClients": [],
    "plugins": []
  },
  "musicSignUrl": "...",
  "enableLocalFile2Url": false,
  "parseMultMsg": false
}
```

`napcat_normalize_connect` 写出的数组键、item 键与桌面 `ConnectConfig` **同名**（`token` / `messagePostFormat` / `enableCors` / `enableWebsocket` / `heartInterval` / `role`…）。官方多出来的键 serde 忽略；老文件可以没有 `httpSseServers`（default 空）。

```
parse_napcat_onebot_connect(raw) =
  ConnectConfig::from(raw["network"])  // serde_json::from_value
  + musicSignUrl / enableLocalFile2Url / parseMultMsg
```

**没有 SL 那套键名重命名。**

`napcat_{qq}.json` 本轮不读。

---

## 5. Docker

### 5.1 发现范围

库存只认容器名 `ncbot-*` / `slbot-*`（`docker ps -a`）。第三方随便起的容器不在导入列表里，本任务不扩扫描。

`ImportableRemoteBot.dockerName` 已有，读文件时用它，不要只靠 qq 再拼一次（虽然当前命名就是 `ncbot-{qq}` / `slbot-{qq}`）。

compose 项目目录（桌面自己装的）：

```
{inventory.selected.home}/.napcat-bots/{docker_name}/
```

HOME 用库存 `selected.home`，不要再 `echo $HOME`（和现 `docker_project_dir` 重复探测）。

### 5.2 NapCat Docker —— host bind，先读盘

compose（`napcat.yml.j2`）：

```
./napcat/config:/app/napcat/config
```

宿主机文件：

```
{home}/.napcat-bots/{docker_name}/napcat/config/onebot11_{qq}.json
```

与 `render_docker_config_on_host` 写入路径一致。`host.read_file` 即可，解析走 `parse_napcat_onebot_connect`。

文件不存在（不是桌面 compose、或尚未写出）→ 回退 `docker cp`：

```
{docker_name}:/app/napcat/config/onebot11_{qq}.json
```

`DockerCli` 今天 **没有** `exec` / `cp`。要加一个按 arg 传参的 `copy_from_container(container, src, dest)`（内部 `docker_cmd().arg("cp")…`，走现有 elevated 决策）。禁止把容器名拼进 `sh -c` 字符串。

`docker cp` 对 **已停止** 容器可用，适合 `docker ps -a` 扫到的导入对象。

### 5.3 SnowLuma Docker —— named volume，不要信 host 目录

compose（`snowluma.yml.j2`）：

```
volumes:
  - {{ name }}-data:/app/snowluma-data
```

真实数据在 Docker named volume，容器内路径：

```
/app/snowluma-data/config/onebot_{qq}.json
```

**陷阱：** `render_docker_config_on_host` 把 SL 配置写到

```
{project_dir}/snowluma-data/config/onebot_{qq}.json
```

该目录 **没有** bind 进容器。这是既有写入路径问题，本任务 **读** 不能以它为权威。最多作最后兜底（桌面曾经写下、容器却没用）。

卷的 **daemon 名** 还带 compose 项目前缀（目录名 `slbot-123` → 常见 `slbot-123_slbot-123-data`），不要猜。权威读法：

```
docker cp {docker_name}:/app/snowluma-data/config/onebot_{qq}.json {tmp}
```

再 `host.read_file(tmp)`，然后删临时文件。解析走现有 `parse_snowluma_onebot_connect`。

容器从未创建过 volume、或文件不存在 → `Ok(None)`（与 SL Native 缺文件相同）。

`DockerCli::copy_from_container` 前先 `ensure_daemon_ready()`（导入不需要 compose 插件）。

---

## 6. 推荐实现（一条命令，四格分发）

不要再为 NC / Docker 各加一条 Tauri 命令。现有 `fetch_remote_onebot_connect` 写死 SL Native，扩展参数会破坏「命令只做参数转换」的清晰度。换成统一入口。

### 6.1 Domain（ts-rs）

新建（建议 `ncd-domain`，挨着 `ConnectConfig`）：

```rust
pub struct ImportedNetworkConfig {
    pub connect: ConnectConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_sign_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_command: Option<StatusCommandConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_local_file_to_url: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse_mult_msg: Option<bool>,
}
```

空字符串 `musicSignUrl` 当 `None`，避免把桌面默认空串当成「迁到了」。

### 6.2 解析（`ncd-deploy` renderer，纯函数，好单测）

- 修 `parse_snowluma_onebot_connect`：补 `enabled→enable`、`reconnectIntervalMs→reconnectInterval`；返回值改为能带出 `musicSignUrl`（或并行 `parse_snowluma_onebot_file → ImportedNetworkConfig`）。
- 新增 `parse_napcat_onebot_connect(&Value) -> Option<ImportedNetworkConfig>`。

### 6.3 读取（各 crate 守自己的路径）

| 格 | 函数 | crate |
| --- | --- | --- |
| SL Native | 现有 `read_remote_onebot_connect`，改为返回 `ImportedNetworkConfig` | snowluma backend |
| NC Native | `read_remote_napcat_connect(host, napcat_root, qq)` | napcat backend（`launch.rs` 旁） |
| NC/SL Docker | `read_docker_imported_network(host, home, docker_name, backend, qq)` | `ncd-runtime` `native_deployment_adapter/docker_helpers.rs`（已有 project_dir / 读 existing） |

Docker 辅助依赖 `ncd-deploy::DockerCli`，放 runtime 比放 backend 合适（Docker 会话本就在 runtime）。

### 6.4 分发（runtime 门面，tauri 只调这里）

`crates/ncd-runtime/src/remote/import_network.rs`（新文件，`remote/mod.rs` 再 export）：

```text
fetch_imported_network(host, selected, backend, deployment, qq, docker_name)
  SL+native  → snowluma_dir 必有，否则 Err「未发现 SnowLuma」
  NC+native  → napcat_root（或 install_base 回退）
  *+docker   → docker_name 必有；home 用 selected.home
```

缺库存路径 = `Err`（提示先「重新发现」）。文件不存在 = `Ok(None)`。JSON 坏 = `Err`（InfoBar 记失败，导入继续）。

删掉 `lib.rs` 上那条仅 SL 的 `pub use read_remote_onebot_connect`，改为 export 分发函数。

### 6.5 Tauri

替换 `fetch_remote_onebot_connect`：

```text
fetch_imported_network(server_id, qq_id, backend, deployment, docker_name: Option<String>)
  → Option<ImportedNetworkConfig>
```

薄壳：找 profile → `get_live_host` → 调 runtime。`generate_handler` 同步改名。

### 6.6 前端

- `bot.service.ts`：新 invoke；mock 返回 `null`。
- `ImportRemoteBotsDialog.runImport`：**对每一行**（不再判断仅 SL native）调用；成功则：
  - `cfg.connect = imported.connect`
  - 有 `musicSignUrl` → `cfg.bot.musicSignUrl`
  - 有 `statusCommand` → `cfg.statusCommand`
  - 有 NC 两字段 → `cfg.advanced.*`
- 成功后 invalidate `botSnapshotsKey` **和** `['botConfig']`。
- 不新增「迁移网络」勾选（失败不阻断）。现有「接管 WebUI 密码」保持独立。

`pnpm run ts-bindings` 必跑。

---

## 7. 测试

| 测什么 | 放哪 |
| --- | --- |
| NC `network` 子对象 → ConnectConfig + 三个顶层字段 | `backend_config_renderer.rs` `onebot_import_tests` |
| NC 缺 `httpSseServers`、多未知键仍成功 | 同上 |
| SL `enabled` / `reconnectIntervalMs` / `musicSignUrl` | 扩现有 kunming 用例 |
| 分发：四格路径选择（mock Host 记录读了哪） | `ncd-runtime` 单测或 backend 测 |
| Docker NC 先 host 路径、没有再 cp | docker_helpers 测（mock Host 命令） |
| 导入对话框：NC/Docker 行也会调 fetch（若有前端单测） | 现有 vitest 风格；没有则手工点验 |

不在本任务打真实 SSH / 真 Docker。

---

## 8. 实现闭包

```text
域: 导入已有 Bot 的网络配置（NC Native + Docker NC/SL + Bug#0 + SL 解析补键）
Rust:
  ncd-domain（ImportedNetworkConfig）
  ncd-deploy/backend_config_renderer.rs（parse_*）
  ncd-deploy/docker/cli.rs（copy_from_container）
  ncd-backend-napcat/remote_native_napcat_session/launch.rs（读 NC Native）
  ncd-backend-snowluma/remote_snowluma/config.rs（返回类型加宽）
  ncd-runtime/remote/import_network.rs（分发）
  ncd-runtime/native_deployment_adapter/docker_helpers.rs（Docker 读）
  ncd-runtime/src/lib.rs（门面）
Tauri: commands/servers.rs + lib.rs generate_handler
UI: bot.service.ts, ImportRemoteBotsDialog.tsx, BotConfigPage.next.tsx（水合）
生成类型: 是（ImportedNetworkConfig）
KB: 不改启停语义；读文件只在导入
不碰: napcat_<qq>.json 高级项、WebUI 凭据、库存探测脚本、启动 renderer
```

建议提交切分：

1. `fix(ui):` Bug#0 invalidate + 水合
2. `feat(deploy):` 解析器（NC + SL 补键）+ 单测
3. `feat(runtime):` 读取 + 分发 + Docker cp
4. `feat(ui):` 对话框四格接线 + ts-bindings

---

## 9. 明确不做

- 已导入 Bot 的「再同步远端网络」（`already_imported` 仍不可选）。要覆盖请先删再导，或以后做单独「从远端刷新连接」。
- 修 SL Docker **写入** 走 named volume（启动覆盖远端配置的正确姿势）。本任务只保证 **导入读到容器内真实文件**。
- 非 `ncbot-`/`slbot-` 容器、本机 Windows Docker。
- 把 `serde_json::Value` 当业务配置透传。

---

## 10. 验收

1. Native SL：导入后连接页能看到条目；`statusCommand` / `musicSignUrl` 若远端有则进高级/身份。重启应用仍在。
2. Native NC：同上，端口与远端 `onebot11_{qq}.json` 一致。
3. Docker NC：host bind 文件能迁。
4. Docker SL：容器内 `/app/snowluma-data/config/onebot_{qq}.json` 能迁；即使 `{project_dir}/snowluma-data/` 是空的。
5. 远端无文件：导入成功，InfoBar 不报迁移成功，连接页为空（空壳，与现在无文件行为一致）。
6. 解析失败：导入成功，InfoBar 带该 QQ 的失败原因。
7. Bug#0：同 QQ 删了再导，连接页是新数据，不必重启应用。
