# ncd-watch 远端值守

ncd-watch 是装在远端 Linux 主机上的小程序。桌面端关掉、断网或者 SSH 断开以后，由它接着盯这台主机上的 Bot，掉线时照你在桌面端设置里配好的方式发通知：Webhook、邮件，或者用同机另一个 Bot 发 QQ 消息。

- 普通通知默认在桌面端退出后由 watch 接管；启用重登二维码的账号始终由 watch 值守。
- ncd-watch 不监听任何端口，只往外发请求，不需要在防火墙上开入站端口。
- 在桌面端「组件」页选中远端主机，就能安装、更新、卸载 ncd-watch。

## 安装位置

默认装在 SSH 用户的家目录下：

| 路径 | 内容 |
| :--- | :--- |
| `~/ncd-watch/bin/ncd-watch` | 程序本体 |
| `~/ncd-watch/config/watch.json` | 运行参数，可以手动改 |
| `~/ncd-watch/config/notify.json` | 要盯哪些 Bot、通知发到哪，由桌面端写 |
| `~/ncd-watch/config/metrics.json` | 桌面端退出后接着采实例 CPU / 内存，由桌面端写 |
| `~/ncd-watch/state/desktop_present` | 桌面端在线心跳 |
| `~/ncd-watch/state/edge_state.json` | 每个 Bot 上一次的状态，用来判断“刚掉线” |
| `~/ncd-watch/state/relogin.json` | 重登事件、指令去重和投递记录，不含二维码 |
| `~/ncd-watch/state/relogin-status.json` | 最近加载配置与已连接的互助 Bot |
| `~/ncd-watch/state/intents/` | Desktop 同步的人工启停意图 |

换目录可以给程序传 `--root <目录>`，或者设环境变量 `NCD_WATCH_ROOT`。

## 进程怎么跑

桌面端安装时会写一个用户级 systemd 服务 `~/.config/systemd/user/ncd-watch.service` 并启动它。主机没有 systemd 时退回 `nohup` 后台运行，这种情况下没有日志文件。

```bash
systemctl --user status ncd-watch       # 看是否在跑
systemctl --user restart ncd-watch      # 重启
journalctl --user -u ncd-watch -f       # 看日志
```

用户级服务在该用户所有登录会话都退出后会被 systemd 停掉。如果你断开 SSH 以后 ncd-watch 就没了，执行一次：

```bash
sudo loginctl enable-linger "$USER"
```

程序自带几个子命令，排查时有用：

```bash
~/ncd-watch/bin/ncd-watch once             # 只探一轮，打印结果
~/ncd-watch/bin/ncd-watch print-defaults   # 打印路径和默认配置
~/ncd-watch/bin/ncd-watch init             # 补齐目录和默认配置，已有文件不覆盖（加 --force 才覆盖）
```

`once` 输出里 `probed` 是探了几个 Bot，`fired` 是这一轮发了几次通知，`desktop_present_skip=true` 表示桌面端在线、这一轮不发。想看每个 Bot 的探测细节，加上 `RUST_LOG=debug`。

## watch.json

安装时写一份默认值，之后桌面端不再改它，可以放心手动编辑。每一轮探测都会重新读，改完不用重启。

| 字段 | 默认 | 说明 |
| :--- | :--- | :--- |
| `protocol` | `1` | 配置格式版本，不要改 |
| `probeIntervalSecs` | `15` | 多少秒探一轮 |
| `desktopPresentTtlSecs` | `90` | 桌面端心跳超过多少秒没更新就当它离线，最小 15。桌面端每 45 秒写一次心跳 |
| `debounceSecs` | `0` | 同一个 Bot 两次掉线通知之间至少隔多少秒，`0` 不限制 |
| `notifyWhileDesktopPresent` | `false` | 桌面端在线时 ncd-watch 也发通知。打开后两边会各发一次 |
| `features` | — | 目前只做记录，不影响行为 |

示例：

```json
{
  "protocol": 1,
  "features": ["process_watch", "docker_watch", "webhook"],
  "probeIntervalSecs": 15,
  "desktopPresentTtlSecs": 90,
  "debounceSecs": 0,
  "notifyWhileDesktopPresent": false
}
```

## notify.json

由桌面端根据 Bot 列表和「设置 → 通知」生成。桌面端开着并连着这台主机时，大约每 45 秒重写一次，保存设置后也会马上重写，所以手动改的内容会被覆盖。要改通知方式请在桌面端设置里改。文件权限是 `600`，里面有 WebUI token 和通知密钥，不要外传。

下面的字段说明用来看懂它、排查问题。

顶层：

| 字段 | 说明 |
| :--- | :--- |
| `serverId` | 这台主机在桌面端里的档案 id，只用于诊断 |
| `bots` | 要盯的 Bot，见下表 |
| `webhookEnabled` / `webhooks` | 掉线 Webhook 开关和通道列表 |
| `emailEnabled` / `email` | 掉线邮件开关和 SMTP 设置 |
| `onebot` | 用同机另一个 Bot 发 QQ 消息 |
| `notifyOnRecovered` | 恢复在线时也发一次通知 |

`bots` 里每一项：

| 字段 | 说明 |
| :--- | :--- |
| `botId` / `qqId` / `botName` | Bot 的 QQ 号和名字，`botName` 会出现在通知里 |
| `backend` | `napcat` 或 `snowluma` |
| `deployment` | `native`（直接运行）或 `docker` |
| `containerName` | Docker 容器名。NapCat 是 `ncbot-<QQ号>`，SnowLuma 是 `slbot-<QQ号>`，缺省时也按这个规则推 |
| `processMatch` | 直接运行时用 `pgrep -f` 找 QQ 进程的匹配串，形如 `no-sandbox -q <QQ号>$` |
| `pidFile` | 可选，按 pid 文件判断进程在不在 |
| `webuiPort` | WebUI 在这台主机上的端口，ncd-watch 从 `127.0.0.1` 访问它来判断 QQ 账号是否在线。Docker 部署取容器实际的端口映射 |
| `webuiToken` | NapCat WebUI token 或 SnowLuma WebUI 密码，仅保存在权限为 `600` 的配置文件中 |
| `enabled` | 是否盯这个 Bot |

`webhooks` 里每一项（注意这里的字段是下划线写法）：

| 字段 | 说明 |
| :--- | :--- |
| `id` / `name` | 通道标识 |
| `enabled` | 是否启用，`url` 为空的通道会被跳过 |
| `url` | 通知地址 |
| `method` | `POST` 或 `GET`。`GET` 会把请求体里的 JSON 字段拆成查询参数 |
| `secret` | 填了就带上 `Authorization: Bearer <secret>` |
| `body_template` | 请求体模板 |

请求体模板里可以用这些占位：`{nickname}`（等同 `{bot_name}`，没填名字时是 QQ 号）、`{uin}`（等同 `{bot_qq_id}`）、`{event}`、`{time}`（等同 `{disconnect_time}`）。ncd-watch 发出的通知还多一个 `{source}`，值是 `watch`，可以用来区分是桌面端还是 ncd-watch 发的。

`onebot`：`messengers` 是同机能用来发消息的 Bot，`targetType` 是 `private` 或 `group`，`targetIds` 是发给谁，`messageTemplate` 是消息模板，占位和 Webhook 一样。跨机的 Bot 不会被写进来。

- 直接运行的 Bot：`baseUrl` 取它第一个监听在本机或 `0.0.0.0` 上的 HTTP 服务器。
- Docker 部署的 Bot：实际服务需要监听容器内 `0.0.0.0`，并发布 TCP 端口。Desktop 保存或启动托管容器时会自动补齐映射，将连接中的回环监听转换为 `0.0.0.0`；在「设置 → 通知」一键补齐 HTTP 时也会按此配置。每一项多带 `containerName` 和 `containerPort`（容器内端口），ncd-watch 发送前用 `docker inspect` 查实际的宿主机端口；查不到才用 `baseUrl`。可在「Bot 配置 → 容器」调整宿主机端口。
- 掉线的 Bot 不会替自己发。只有一个 Bot 时 QQ 消息通知用不了，请配合 Webhook 或邮件。
- 按顺序试，一个发送方完全发不出去就换下一个；已经有人收到的不会换发送方重发。

## 怎么判断掉线

### 重登二维码（同机互助）

在「设置 → 通知 → OneBot」选择同一远端主机上的互助 Bot，打开「远端掉线后发送重登二维码」，填写管理员 QQ 号。各 Bot 的掉线通知开关决定监控范围。互助 Bot 必须有通用正向 WebSocket：原生部署监听本机地址，Docker 在容器内监听 `0.0.0.0` 并发布该端口。只配 HTTP 或仅事件/API 的 WebSocket 不足以完成收发闭环。

升级远端 ncd-watch 后同步通知配置。旧二进制会明确拒绝启用此能力。配置刚写入但 watch 尚未确认加载时，稍后重新同步；CLI 可查看当前状态：

```bash
~/ncd-watch/bin/ncd-watch capabilities
~/ncd-watch/bin/ncd-watch relogin-status
```

此模式不依赖 Desktop、聊天窗口或调试台常驻，也不受 `notifyWhileDesktopPresent` 控制。目前覆盖远端同机互助；本机 Desktop 自动取码和跨服务器代发尚未接入。

- 账号曾在线且持续掉线默认 60 秒后，由另一个在线 Bot 私发一次通知与可用的二维码。群目标只收到文字提示，二维码只发给管理员。
- 二维码过期不追加通知。取码失败时先发一次文字，等待管理员请求。无人参与的恢复默认不提示；可沿用恢复通知开关。主动刷新过的管理员会收到一次恢复确认，需连续在线 2 分钟。
- 普通私聊接收目标不会自动获得二维码权限；管理员必须单独配置。没有在线互助方时尝试已配置的邮件和 Webhook，已恢复的旧事件不补发。
- 手动停止、首次等待登录以及无法确认账号状态都不自动发送重登码。超时投递按结果未知保存，避免立即换 Bot 重复通知。
- 进程或容器确认停止时仍发送文字告警；WebUI 鉴权失败、探测权限不足保留为未知，不据此判断需要扫码。

| 管理员消息 | 行为 |
| :--- | :--- |
| 回复重登通知「刷新二维码」 | 读取或刷新该事件的二维码 |
| 私聊「刷新二维码」 | 当前会话只有一个待处理账号时可用 |
| `/ncd 刷新 QQ号或唯一名称` | 获取指定账号的当前二维码，默认冷却 30 秒 |
| `/ncd 状态` | 查看未结束的掉线事件 |
| `/ncd 忽略 QQ号或唯一名称` | 停止本次事件的主动通知 |

群指令必须来自管理员、位于已配置群内并 @ 当前互助 Bot；回复仍走私聊，不将二维码回退到群里。管理员应能正常私聊该 Bot。

SL 取码依赖 QQ 的图形会话和扫码辅助；原生 Linux / Docker 共用进程锁与刷新冷却，已扫码时不会刷新。NC 通过 WebUI 读取二维码；有明确扫码阶段的版本才主动调用刷新接口，老版本保守地返回当前码。QQ 页面、辅助或会话不可用时需要打开 QQ / VNC 处理，刷新指令不会由 watch 主动重启进程。

账本损坏或写入失败会暂停重登投递；修复文件权限/磁盘问题后重启 watch。不要为排障直接删除账本，清空投递记录可能导致重复通知。`once` 只运行普通边沿探测，不执行二维码或管理员指令服务。

### 普通通知

每一轮对每个 Bot 分两层看：

1. 进程在不在。Docker 部署跑 `docker inspect -f {{.State.Running}} <容器名>`，直接运行跑 `pgrep -f -- <processMatch>`。
2. QQ 账号是不是登录着。有 `webuiPort`、`webuiToken` 的 Bot 会登录本机 WebUI 查询账号状态。NapCat 查询登录接口；SnowLuma 查询进程列表，并通过账号号、PID 和进程启动代次确认归属。SnowLuma 未登录时账号号为 `0`，只有确认属于目标 Bot 的进程才会判为掉线。

能查账号的 Bot 以账号为准：从“已登录”变成“未登录”才算掉线，进程在不代表账号在线。查不了账号的 Bot 才退回按进程判断。

## Docker 部署掉线了却没通知

先跑一轮看结果：

```bash
RUST_LOG=debug ~/ncd-watch/bin/ncd-watch once
```

然后按下面几条逐个对。

1. 容器名对不上。ncd-watch 按 `ncbot-<QQ号>`（NapCat）或 `slbot-<QQ号>`（SnowLuma）找容器，用 `docker ps -a --format '{{.Names}}'` 看一下。自己建的、名字不一样的容器查不到，会一直显示离线或未知。

2. SSH 用户跑不了 docker。ncd-watch 以 SSH 用户身份执行 `docker inspect`。直接执行 `docker ps`，报 `permission denied` 就把用户加进 docker 组，重新登录后生效：

   ```bash
   sudo usermod -aG docker "$USER"
   ```

3. WebUI 端口对不上。桌面端连上 Docker Bot 时，会用 `docker inspect` 读 WebUI（容器内 6099）实际映射到宿主机的哪个端口，再写进 `notify.json` 的 `webuiPort`，自己用 compose 起、再导入的容器也一样。读不到时按桌面端部署的规则推算，也就是 `6099 + QQ号 % 500`，例如 QQ 123456789 对应 6388。推算出来的端口不会覆盖 `notify.json` 里已有的端口。对一下实际映射：

   ```bash
   docker port ncbot-<QQ号> 6099
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:<webuiPort>/
   ```

   两边不一致时，在桌面端把这个 Bot 连上一次（启动它，或者桌面端启动时它已经在跑），等一两分钟 `notify.json` 就会更新。如果 WebUI 只绑在某个网卡 IP 上（例如 `192.168.1.5:6099`），从 `127.0.0.1` 连不上，要改成绑 `0.0.0.0` 或 `127.0.0.1`。

4. 防火墙。ncd-watch 访问的是本机回环地址 `127.0.0.1`，ufw、firewalld 默认不拦回环。上一步的 `curl` 能连上，就可以排除防火墙。云服务器的安全组只管外部流量，和这里无关。ncd-watch 需要能访问外网的只有 Webhook 地址和 SMTP 服务器。

5. 桌面端其实在线。普通模式下，Desktop 在线时由桌面端发通知，`once` 会输出 `desktop_present_skip=true`。重登二维码模式始终由 watch 值守，应查看 `relogin-status` 和服务日志。

6. ncd-watch 没在跑。`systemctl --user status ncd-watch` 看状态，断开 SSH 以后就停了的话见上文 `enable-linger`。
