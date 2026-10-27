---
title: 掉线通知与远端值守
description: 配置 Webhook、邮件和同机 Bot 通知，安装 ncd-watch，并核对 Desktop 退出后的接管情况。
section: 远端与部署
order: 140
---

ncd-watch 安装在远端主机上，可以在 Desktop 退出后监控 Bot 并发送掉线通知。

Desktop 在线时负责普通通知，退出后由 watch 接手；开启重登二维码的账号始终由 watch 负责。ncd-watch 只主动向外发请求，不监听端口，无需为它开放防火墙入站规则。

## 1. 选一种通知方式

打开「设置 → 通知」，选择可用的通知方式，例如已有的 Webhook 或 SMTP 邮箱。

![通知页配置投递通道与远端值守](/docs/screenshots/settings-notifications-light.png)

| 方式 | 要准备 | 适合 |
| --- | --- | --- |
| Webhook | 接收地址、请求方法和消息模板 | 已有推送服务或自己的接口 |
| 邮件 | SMTP 服务器、认证信息和收件人 | 不想依赖另一个 QQ Bot |
| OneBot QQ 消息 | 同一主机上另一个在线的 Bot，接收的 QQ / 群 | 有多个 Bot，想在 QQ 上收通知 |

### Webhook

1. 在 Webhook 通道里添加一个，起个能看出用途的名字。
2. 填接收「地址」，选对方要求的「请求方法」，再按服务预设或者模板填好请求内容。
3. 对照接收服务的文档核对一下字段。需要 Bearer 鉴权的，把密钥填在对应的字段里，截图时记得遮住。
4. 用编辑器里的测试按钮发一条，确认对方真的收到了，再保存通道和设置。

模板里可以用这些变量：`{nickname}` / `{bot_name}`、`{uin}` / `{bot_qq_id}`、`{event}`、`{time}` / `{disconnect_time}`。由 watch 发出的通知还多一个 `{source}`，值是 `watch`，可以用来区分通知是谁发的。

### 邮件

打开邮件配置，填上邮箱服务商提供的 SMTP 地址、端口、认证信息和收件人。很多服务商要求用授权码而不是登录密码，这种情况就填授权码。测试通过后，启用邮件并保存设置。

### 用另一个 Bot 发 QQ 通知

在 OneBot 配置里选一个发送方，填上接收通知的 QQ 号或群号，以及消息模板。发送方必须和被监控的 Bot 在同一台远端主机上，跨主机的 Bot 不会被同步成 watch 的发送方。

发普通 QQ 通知，需要发送方有一条能用的 HTTP 服务器连接。原生部署可以监听本机或者 `0.0.0.0`；Docker 里则要监听 `0.0.0.0`，并把端口发布到宿主机，watch 会尝试读取实际的端口映射。

掉线的 Bot 无法给自己发通知，只有一个 Bot 时可使用 Webhook 或邮件。配置多个发送方后，如果一个发送方完全无法投递，会自动换下一个；已送达的目标不会重复发送。

## 2. 给要监控的 Bot 打开通知

1. 打开 Bot 的「配置 → 高级」，开启「掉线时发送通知」，保存。
2. 回到「设置 → 通知」，按需设置「上线恢复通知」和「掉线防抖（秒）」。恢复通知默认是不发的；防抖填 `0` 表示不限制。
3. 点「保存设置」。还没保存的草稿是不会同步到远端的。

SnowLuma 的通知开关改了马上生效，NapCat 要按提示重启 Bot 后才生效。

## 3. 安装并同步 ncd-watch

1. 打开「组件」，选中 Bot 所在的那台远端主机，安装 ncd-watch。
2. 回到「设置 → 通知」，找到「远端脱管监控（NCD Watch）」。
3. 刷新这台主机的状态，确认已经安装好了；如果显示「可更新」，先把它更新一下。
4. 通知设置都保存好以后，点这台主机的同步按钮。有好几台主机的话，可以直接点「全部同步」。
5. 看一下同步结果。每台服务器都需要分别安装、分别同步。

Desktop 在线并连接主机时，约每 45 秒同步一次配置和心跳，保存设置也会触发同步。普通模式下，watch 在心跳超时后接手，默认超时时间为 90 秒；刚关闭 Desktop 时尚未接管属于正常情况。

Windows 远端上的 watch 是由 Windows 远程代理来承载的，需要先完成 [Windows 托管](/docs/remote-windows/)。下面讲的路径和 systemd 命令都只适用于 Linux。

## 4. 在 Linux 上确认 watch 在运行

默认装在 SSH 用户的家目录下：

| 路径 | 用途 |
| --- | --- |
| `~/ncd-watch/bin/ncd-watch` | 程序 |
| `~/ncd-watch/config/watch.json` | 探测间隔等参数，可以手改 |
| `~/ncd-watch/config/notify.json` | Desktop 生成的 Bot 与通知配置 |
| `~/ncd-watch/config/metrics.json` | Desktop 退出后的 CPU / 内存采样配置 |
| `~/ncd-watch/state/` | 心跳、上次状态和重登投递账本 |

安装时会优先创建一个用户级的 systemd 服务，可以这样查看：

```bash
systemctl --user status ncd-watch
journalctl --user -u ncd-watch -f
```

没有 systemd 的主机，会退而用 `nohup` 在后台运行，这种方式没有日志文件。另外，用户级的 systemd 服务有可能在这个用户所有的登录会话都退出以后被停掉。如果你发现一断开 SSH 服务就没了，请管理员执行：

```bash
sudo loginctl enable-linger "$USER"
```

需要重启服务时：

```bash
systemctl --user restart ncd-watch
```

## 确认普通通知能接管

可以让它只跑一轮探测看看：

```bash
~/ncd-watch/bin/ncd-watch once
```

输出里的 `probed` 是这一轮探测了几个 Bot，`fired` 是发出了几条通知。如果 Desktop 正在线，你会看到 `desktop_present_skip=true`，意思是普通通知交给 Desktop 去发了，这不是故障。

普通通知只在状态变化时发送。进程在线且能查到账号时，以 QQ 登录状态为准；进程或容器运行中不代表账号在线。如果无法确认进程归属或 WebUI 鉴权失败，状态会记为未知，不会因此触发扫码。

验证的时候，用你自己的测试账号：先确认它确实在线过，再找个不影响使用的时间，真的让它掉一次线。手动停止 Bot，或者第一次启动还在等登录，这些都不算需要重登的故障。

## 启用同机互助重登二维码

同机互助由同一台主机上在线的 Bot 私聊发送掉线通知和重登二维码，方便在手机上重新登录。使用前需要满足以下条件：

1. 远端的 ncd-watch 是支持这个功能的新版本。旧版本会直接拒绝启用。
2. 互助 Bot 配了一条通用正向 WebSocket：原生部署监听本机地址，Docker 里监听 `0.0.0.0` 并发布端口。只有 HTTP，或者只管事件、只管 API 的 WebSocket，都不够用。
3. 在「设置 → 通知 → OneBot」里选择同机的互助 Bot，打开「远端掉线后发送重登二维码」。
4. 单独填一个「管理员 QQ 号」，并确认这个管理员能正常私聊互助 Bot。普通的私聊接收目标，不会自动变成二维码管理员。
5. 保存设置，同步到对应的主机，等 watch 确认已经加载。要监控哪些 Bot，仍然由各个 Bot 自己的「掉线时发送通知」决定。

一个账号在线过、然后持续掉线后，默认 60 秒会由互助 Bot 发一次通知。二维码只会私聊发给管理员，群里只会收到文字通知。如果没有可用的互助 Bot，会改用你配好的邮件和 Webhook；已经恢复的旧事件不会补发。

| 管理员发送 | 作用 |
| --- | --- |
| 回复重登通知「刷新二维码」 | 获取或刷新这次事件的二维码 |
| 私聊「刷新二维码」 | 当前只有一个待处理账号时用 |
| `/ncd 刷新 QQ号或唯一名称` | 指定账号刷新，默认冷却 30 秒 |
| `/ncd 状态` | 查看还没结束的掉线事件 |
| `/ncd 忽略 QQ号或唯一名称` | 这次事件不再主动通知 |

在已配置的群里发指令时，需要 @ 互助 Bot。二维码始终通过私聊发送，私聊失败也不会改发到群里。二维码过期后不会自动补发，请按上表手动刷新。主动刷新过二维码的管理员会在账号连续在线 2 分钟后收到恢复确认。

SnowLuma 依赖可用的 QQ 图形会话和扫码辅助，NapCat 通过 WebUI 取码。如果需要在 QQ / VNC 中手动处理，刷新指令不会重启进程。目前不支持本机自动取码和跨服务器代发。

## 远端没发通知

1. 「设置 → 通知」保存了吗？主机同步成功了吗？Bot 的通知开关打开了吗？
2. watch 还在运行吗？断开 SSH 以后，用户服务是不是被停掉了？
3. 普通模式下，watch 是不是还能检测到 Desktop 的心跳；重登模式的话，查 `relogin-status`。
4. Docker：这个用户能不能执行 `docker ps`，容器名和实际的 WebUI 端口对不对。Desktop 连上一次 Bot 以后，会读取端口映射并更新同步的配置。
5. Webhook / SMTP 从服务器上能不能访问到，互助 Bot 是不是在线，连接方向和 token 对不对。

还想再深入查的话：

```bash
RUST_LOG=debug ~/ncd-watch/bin/ncd-watch once
~/ncd-watch/bin/ncd-watch capabilities
~/ncd-watch/bin/ncd-watch relogin-status
```

`once` 只运行普通状态探测，不运行二维码和管理员指令服务。排错时不要删除重登账本；清空投递记录可能导致通知重复发送。

## 手动配置

`watch.json` 可以手动改，每一轮都会重新读取，改了不用重启。常用的字段有：`probeIntervalSecs`（默认 15）、`desktopPresentTtlSecs`（默认 90，最小 15）、`debounceSecs`（默认 0）、`notifyWhileDesktopPresent`（默认 `false`）。把最后一项改成 `true` 的话，Desktop 和 watch 可能会各发一次同样的普通通知。

`notify.json` 和 `metrics.json` 是由 Desktop 维护的，手动改了也会在下次同步时被覆盖。`notify.json` 的权限是 `600`，里面有 WebUI token 和通知用的密钥，不要整份贴到公开的地方。如果你用了自定义的安装目录，执行命令时加上 `--root <目录>`，或者设置环境变量 `NCD_WATCH_ROOT`。
