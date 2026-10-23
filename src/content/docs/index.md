---
title: 从这里开始
description: 安装 Desktop、登录第一个 QQ Bot，再把它接到应用端；也可以按问题直接找到对应操作。
section: 开始使用
order: 0
---

NapCatQQ Desktop 是一个管 QQ 机器人的 Windows 程序：帮你装 QQ 和协议端、登录账号、接上应用端，平时还能看日志、聊天、调接口。Bot 放在服务器上，也一样能从这里管。

第一次接触 QQ 机器人也不用担心，这套文档就是照着新手的路线写的。我们先在自己电脑上让一个 Bot 跑起来、能回消息，再去折腾插件、模型和服务器。

## 先认识三样东西

| 名称 | 负责什么 | 在 Desktop 里去哪找 |
| --- | --- | --- |
| QQ 与协议端 | 登录 QQ、收发消息，可以理解成 Bot 的嘴和耳朵。协议端是 NapCat 或 SnowLuma | 「组件」装程序，「机器人」管账号和运行状态 |
| 应用端 | 决定收到消息以后做什么：响应指令、跑插件、AI 对话，相当于 Bot 的脑子。比如 NoneBot2、Karin、AstrBot、Koishi | 「应用端」建实例、改配置、做对接 |
| Desktop | 把前两样装好、启动、管起来，另外提供日志、聊天、调试和远端管理 | 就是你现在用的这个 Windows 程序 |

很多人第一次卡住都是在这里：Bot 明明登录了，在群里喊它却没反应。原因是能收发消息不等于会处理消息。要它响应指令，还得有一个应用端在运行，并且和 Bot 之间建立好 OneBot 连接。

## 第一次用，照这个顺序来

1. [下载安装](/docs/installation/)：装好 Desktop 本体。
2. [创建第一个 Bot](/docs/first-bot/)：在本机装组件，填 QQ 号，扫码登录。
3. [连接应用端与 OneBot](/docs/connections/)：先用 HTTP 查一次登录信息，确认能连通。
4. [安装应用端](/docs/apps/)：装一个框架，对接上 Bot，发条消息试试。
5. [备份、迁移与恢复](/docs/backup/)：都弄好以后导一份配置出来，之后折腾坏了还能退回去。

入门部分默认你用的是 Windows 本机 + NapCat + 直接运行，这是最少踩坑的组合。打算让 Bot 在服务器上常驻的，可以从 [远端 Linux](/docs/remote-linux/) 或 [远端 Windows](/docs/remote-windows/) 开始。本机不支持 Docker 部署，Docker 只在远端 Linux 上用得到。

## 已经会用了，按需查

| 想做什么 | 看这篇 |
| --- | --- |
| 看运行摘要、待处置与最近通知 | [概览](/docs/overview/) |
| 安装 QQ、NapCat、SnowLuma 或更新组件 | [组件管理](/docs/components/) |
| 修改 Bot、批量启停、设置自动重启 | [机器人管理](/docs/bots/) |
| 选择 HTTP / 正向 WS / 反向 WS，排查连接 | [连接应用端与 OneBot](/docs/connections/) |
| 管理应用端插件、模型与框架配置 | [框架功能](/docs/frameworks/) |
| 私聊、群聊、群文件、聊天截图 | [聊天与截图](/docs/chat/) |
| 调用 OneBot 接口、查看事件和保存请求 | [API 调试台](/docs/debugging/) |
| 让本机 AI 客户端检查 Bot、通道与调用记录 | [接入 NCD MCP](/docs/mcp/) |
| 编写 MCP 客户端或确认工具参数 | [MCP 工具参考](/docs/mcp-tools/) |
| 限制 AI 可访问的内容和可执行的操作 | [MCP 安全与权限](/docs/mcp-security/) |
| 使用本机与 SSH 终端 | [终端](/docs/terminal/) |
| 找安装失败原因、查看 Bot 与应用日志 | [任务与日志](/docs/tasks-logs/) |
| 添加 SSH 主机、准备服务器 | [远端 Linux](/docs/remote-linux/) · [远端 Windows](/docs/remote-windows/) |
| 在服务器上用容器运行 Bot | [Docker](/docs/docker/) |
| 给远端 SnowLuma 扫码登录 | [远端登录](/docs/remote-login/) |
| Desktop 退出后继续监控远端 Bot | [ncd-watch 值守](/docs/watch/) |
| 下载慢、配置代理和 GitHub 线路 | [网络设置](/docs/network/) |
| 调整主题、通知、功能入口与托盘 | [设置](/docs/settings/) |
| 导出配置、换盘、清理下载缓存 | [备份与数据](/docs/backup/) |
| 不知道从哪查起 | [故障排查](/docs/troubleshooting/) |

## 演示和截图

不少教程页里嵌了一个能操作的演示。点「自动演示」它会自己走一遍；想亲手试，点「自己操作」接过来；点乱了就「重置」。

演示和截图用的都是 Desktop 真实的界面，只是数据是假的。它不会真的装软件、登 QQ，也不会往群里发消息，可以放心点。你那边的版本、主题或功能开关不一样的话，界面会有些出入。

照着做的时候，记得把示例里的 QQ 号、地址、端口和 Token 换成你自己的。示例 Token 写在网页上谁都看得到，别拿去当正式密码用。

## 遇到问题

先别急着卸载重装。记下 Desktop 版本、协议端和它的版本、跑在哪台机器上、直接运行还是 Docker、出错前最后一步做了什么，然后去看对应的任务和日志。很多时候看完日志，问题出在哪就清楚了。

需要提 Issue 的话，写清楚你预期会怎样、实际又是怎样，并把密码、Token、API Key、SSH 私钥和私人消息打上码。
