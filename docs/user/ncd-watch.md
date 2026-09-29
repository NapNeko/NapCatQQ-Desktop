# ncd-watch 远端值守

ncd-watch 是装在远端 Linux 主机上的小程序。桌面端关掉、断网或者 SSH 断开以后，由它接着盯这台主机上的 Bot，掉线时照你在桌面端设置里配好的方式发通知：Webhook、邮件，或者用同机另一个 Bot 发 QQ 消息。

- 桌面端在线时，默认只由桌面端发通知，ncd-watch 不会重复发。
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
| `webuiPort` | NapCat WebUI 在这台主机上的端口，ncd-watch 从 `127.0.0.1` 访问它来判断 QQ 账号是否在线 |
| `webuiToken` | NapCat WebUI token |
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

`onebot`：`messengers` 是同机能用来发消息的 Bot（`baseUrl` 是它的 OneBot HTTP 地址，只收录监听在本机的 HTTP 服务器），`targetType` 是 `private` 或 `group`，`targetIds` 是发给谁，`messageTemplate` 是消息模板，占位和 Webhook 一样。跨机的 Bot 不会被写进来。

## 怎么判断掉线

每一轮对每个 Bot 分两层看：

1. 进程在不在。Docker 部署跑 `docker inspect -f {{.State.Running}} <容器名>`，直接运行跑 `pgrep -f -- <processMatch>`。
2. QQ 账号是不是登录着。只对 NapCat 且 `webuiPort`、`webuiToken` 都有的 Bot 做：用 token 登录 `http://127.0.0.1:<webuiPort>` 的 WebUI，查登录状态。

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

3. WebUI 端口对不上。桌面端部署的 NapCat 容器会把 WebUI（容器内 6099）映射到宿主机 `6099 + QQ号 % 500`，例如 QQ 123456789 对应 6388，`notify.json` 里的 `webuiPort` 也按这个规则写。对一下实际映射：

   ```bash
   docker port ncbot-<QQ号> 6099
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:<webuiPort>/
   ```

   `docker port` 给出的端口和 `webuiPort` 不一致，或者 `curl` 连不上，说明容器不是按桌面端的规则映射的，常见于自己用 compose 起、再导入桌面端的容器。`notify.json` 会被桌面端重写，改它不管用。目前的办法是在桌面端重新部署这个 Bot，或者把容器的 6099 映射到上面算出的端口。

4. 防火墙。ncd-watch 访问的是本机回环地址 `127.0.0.1`，ufw、firewalld 默认不拦回环。上一步的 `curl` 能连上，就可以排除防火墙。云服务器的安全组只管外部流量，和这里无关。ncd-watch 需要能访问外网的只有 Webhook 地址和 SMTP 服务器。

5. 桌面端其实在线。桌面端开着、连着这台主机时，ncd-watch 不发通知，由桌面端发。`once` 输出里 `desktop_present_skip=true` 就是这种情况。想两边都发，把 `watch.json` 的 `notifyWhileDesktopPresent` 改成 `true`。

6. ncd-watch 没在跑。`systemctl --user status ncd-watch` 看状态，断开 SSH 以后就停了的话见上文 `enable-linger`。
