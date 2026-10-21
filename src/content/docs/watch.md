---
title: 掉线通知与远端值守
description: 配置 Webhook、邮件和同机 Bot 通知，安装 ncd-watch，并核对 Desktop 退出后的接管情况。
section: 远端与部署
order: 140
---

如果 Bot 跑在远端服务器上，合上电脑后仍想接到掉线消息，需要让服务器自己值守。ncd-watch 负责这件事：它安装在远端，不依赖你的聊天窗口或调试台常驻。

普通通知在 Desktop 在线时由 Desktop 负责，退出后由 watch 接管；启用重登二维码的账号始终由 watch 值守。ncd-watch 只向外发请求，不监听管理端口，不需要为它新增防火墙入站端口。

## 1. 先选一种能独立收到的通知

打开「设置 → 通知」，选择需要的通道。只想先跑通，可以从自己已有的 Webhook 或 SMTP 邮箱开始，不必一次打开所有方式。

![通知页配置投递通道与远端值守](/docs/screenshots/settings-notifications-light.png)

| 方式 | 要准备什么 | 适合什么时候用 |
| --- | --- | --- |
| Webhook | 接收地址、对方要求的请求方法和消息模板 | 已有推送服务或自己的接收接口 |
| 邮件 | SMTP 服务器、认证信息和收件人 | 不希望依赖另一个 QQ Bot |
| OneBot QQ 消息 | 同一主机上的另一个在线 Bot、目标 QQ / 群 | 已有多个 Bot，想在 QQ 接收消息 |

### 配置 Webhook

1. 在通知页的 Webhook 通道中添加通道，给它取一个能区分用途的名称。
2. 填写接收「地址」，选择对方要求的「请求方法」，按服务预设或消息模板填写请求内容。
3. 对照接收服务的文档确认字段。需要 Bearer 鉴权时，在对应密钥字段填写，不要把密钥放进公开截图。
4. 使用编辑器的测试入口，确认接收端确实收到，再保存通道和设置。

模板支持 `{nickname}` / `{bot_name}`、`{uin}` / `{bot_qq_id}`、`{event}`、`{time}` / `{disconnect_time}`。watch 发出的通知还支持 `{source}`，值为 `watch`，方便区分来源。

### 配置邮件

打开邮件配置，填写邮箱服务商提供的 SMTP 地址、端口、认证信息和收件人。需要授权码的服务商应填写授权码，不能直接假设网页登录密码可用。测试通过后启用邮件并保存设置。

### 用另一个 Bot 发 QQ 通知

在 OneBot 配置中选发送方，再填写私聊 QQ 号或群号、消息模板。发送方必须和被监控 Bot 在**同一台远端主机**，跨主机 Bot 不会被同步为 watch 的发送方。

普通 QQ 通知需要可用的 HTTP 服务器连接：原生部署可监听本机或 `0.0.0.0`；Docker 内需要监听 `0.0.0.0`，并把端口发布到宿主机。watch 会尝试读取实际映射。

掉线的 Bot 不能替自己发送通知。只有一个 Bot 时，使用 Webhook 或邮件。配置多个发送方后，watch 会在完全发送失败时尝试下一位，已经送达的目标不会换 Bot 重发。

## 2. 给需要监控的 Bot 打开通知

1. 打开 Bot「配置 → 高级」。
2. 开启「掉线时发送通知」，保存。
3. 回「设置 → 通知」，按需要设置「上线恢复通知」和「掉线防抖（秒）」。默认不发送恢复通知，防抖 `0` 表示不限制。
4. 点击「保存设置」。未保存的草稿不会被同步到远端。

SnowLuma 的 Bot 通知开关即时生效，NapCat 的开关按界面提示在重启 Bot 后生效。先在合适的时间完成这一步，再验证通知。

## 3. 安装并同步 ncd-watch

1. 打开「组件」，选择 Bot 所在的远端主机，安装 ncd-watch。
2. 回「设置 → 通知」，找到「远端脱管监控（NCD Watch）」。
3. 刷新这台主机的状态，确认已安装。发现「可更新」时，先更新远端组件。
4. 保存所有通知设置后，点击对应主机的同步按钮，或对多台主机使用「全部同步」。
5. 核对同步结果。多台服务器需要分别安装、同步，不能只在其中一台安装。

Desktop 在线并连接这台主机时，会约每 45 秒更新配置与心跳；保存设置也会触发同步。普通模式下，watch 等待心跳超时后接管，默认超时 90 秒，因此退出 Desktop 的瞬间没有接管并不一定是故障。

Windows 远端的 watch 由 Windows 远程代理承载，先完成 [Windows 托管](/docs/remote-windows/)。下面的路径和 systemd 命令仅适用于 Linux。

## 4. 在 Linux 确认值守进程存在

默认路径在 SSH 用户家目录：

| 路径 | 用途 |
| --- | --- |
| `~/ncd-watch/bin/ncd-watch` | 程序 |
| `~/ncd-watch/config/watch.json` | 探测间隔等参数，可手动修改 |
| `~/ncd-watch/config/notify.json` | Desktop 生成的 Bot 与通知配置 |
| `~/ncd-watch/config/metrics.json` | Desktop 退出后的 CPU / 内存采样配置 |
| `~/ncd-watch/state/` | 心跳、上次状态和重登投递账本 |

安装优先创建用户级 systemd 服务，可运行：

```bash
systemctl --user status ncd-watch
journalctl --user -u ncd-watch -f
```

没有 systemd 的主机退回 `nohup` 后台运行，这种方式没有日志文件。用户级 systemd 服务可能在该用户所有登录会话退出后停止；如果断开 SSH 后服务就消失，由管理员确认后执行：

```bash
sudo loginctl enable-linger "$USER"
```

需要重启服务时：

```bash
systemctl --user restart ncd-watch
```

## 核对普通通知是否接管

用下面的命令只探测一轮：

```bash
~/ncd-watch/bin/ncd-watch once
```

`probed` 表示探测了几个 Bot，`fired` 表示这轮发送了几次通知。Desktop 正在线时，`desktop_present_skip=true` 表示普通通知交给 Desktop，这不是故障。

普通通知判断的是状态变化，不会每 15 秒重复告诉你同一 Bot 已离线。进程在线且能查询账号时，以 QQ 登录状态为准；进程或容器运行不代表账号在线。无法确认归属或 WebUI 鉴权失败会保留为未知，不能据此推断应该扫码。

做验证时使用自己的测试账号，先确认曾在线，再在可接受的时间测试真实掉线场景。手动停止和首次等待登录不等同于需要自动重登的故障。

## 启用同机互助重登二维码

这一模式让同机的另一个在线 Bot 私发通知与二维码，适合你暂时不在电脑旁时处理登录。它还需要这些条件：

1. 远端 ncd-watch 已更新，并支持当前能力。旧二进制会明确拒绝启用。
2. 互助 Bot 配置**通用正向 WebSocket**，原生部署监听本机地址，Docker 内监听 `0.0.0.0` 并发布端口。只配 HTTP 或仅事件 / API 的 WebSocket 不足以完成指令收发。
3. 在「设置 → 通知 → OneBot」选择同机互助 Bot，打开「远端掉线后发送重登二维码」。
4. 单独填写「管理员 QQ 号」，并确认管理员能正常私聊互助 Bot。普通私聊接收目标不会自动成为二维码管理员。
5. 保存设置，同步到对应主机，等 watch 确认加载。监控范围沿用各 Bot 的「掉线时发送通知」。

账号曾在线并持续掉线后，默认 60 秒由互助方发送一次通知。二维码只私发管理员；群目标只收到文字。无人可用时尝试已配置的邮件和 Webhook，已恢复的旧事件不会补发。

| 管理员发送的内容 | 用途 |
| --- | --- |
| 回复重登通知「刷新二维码」 | 获取或刷新本次事件的二维码 |
| 私聊「刷新二维码」 | 当前会话仅有一个待处理账号时使用 |
| `/ncd 刷新 QQ号或唯一名称` | 指定账号，默认刷新冷却 30 秒 |
| `/ncd 状态` | 查看未结束的掉线事件 |
| `/ncd 忽略 QQ号或唯一名称` | 停止本次事件的主动通知 |

群指令要求管理员在已配置群内 @ 当前互助 Bot，二维码回复仍走私聊，不会因为私聊失败就发到群里。二维码过期不会自动追加消息，按上表请求刷新。主动刷新过的管理员会在连续在线 2 分钟后收到一次恢复确认。

SnowLuma 仍依赖可用 QQ 图形会话与扫码辅助；NapCat 通过 WebUI 取码。QQ / VNC 需要人工处理时，刷新指令不会让 watch 擅自重启进程。本机自动取码和跨服务器代发尚未接入。

## 远端没发通知时，先查这几处

1. 「设置 → 通知」是否已保存，主机是否同步成功，Bot 的通知开关是否开启。
2. watch 是否仍运行，断开 SSH 后用户服务是否被停止。
3. 普通模式是否仍检测到 Desktop 心跳；重登模式则检查 `relogin-status`。
4. Docker 用户能否 `docker ps`，容器名和实际 WebUI 端口是否正确。Desktop 连接一次 Bot 后会读取映射并更新同步配置。
5. Webhook / SMTP 是否能从**服务器**访问，互助 Bot 是否在线、连接方向和 token 是否正确。

进一步查看：

```bash
RUST_LOG=debug ~/ncd-watch/bin/ncd-watch once
~/ncd-watch/bin/ncd-watch capabilities
~/ncd-watch/bin/ncd-watch relogin-status
```

`once` 只执行普通边沿探测，不执行二维码或管理员指令服务。不要为排错删除重登账本，清空投递记录可能导致重复通知。

## 手动配置哪些文件会保留

`watch.json` 安装后可手动修改，每轮重新读取，无需重启。常见字段为 `probeIntervalSecs`（默认 15）、`desktopPresentTtlSecs`（默认 90，最小 15）、`debounceSecs`（默认 0）和 `notifyWhileDesktopPresent`（默认 `false`）。把最后一项改为 `true` 会让 Desktop 和 watch 可能各发一次普通通知。

`notify.json` 和 `metrics.json` 由 Desktop 维护，手动修改会被后续同步覆盖。`notify.json` 权限为 `600`，包含 WebUI token 和通知密钥，不能整份贴到公开问题里。使用自定义安装根时，命令加 `--root <目录>` 或设置 `NCD_WATCH_ROOT`，不要误查默认目录。

本页依据：[main 的 ncd-watch 指南](https://github.com/NapNeko/NapCatQQ-Desktop/blob/main/docs/user/ncd-watch.md) 及[通知设置界面](https://github.com/NapNeko/NapCatQQ-Desktop/tree/main/src-ui/modules/settings/tabs/notifications)。
