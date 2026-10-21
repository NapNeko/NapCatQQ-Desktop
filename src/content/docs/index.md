---
title: 从这里开始
description: 安装 Desktop、登录第一个 QQ Bot，再把它接到应用端；也可以按问题直接找到对应操作。
section: 开始使用
order: 0
---

第一次接触 QQ Bot，可以先按下面的顺序走一遍：安装 Desktop，准备 QQ 和协议端，创建并登录 Bot，最后连接应用端。先让一条链路跑通，再添加插件、模型和远端主机。

## 先分清这三个部分

| 名称 | 做什么 | 在 Desktop 里从哪里管理 |
| --- | --- | --- |
| QQ 与协议端 | 登录 QQ、收发消息。这里的协议端可以是 NapCat 或 SnowLuma | 「组件」准备程序，「机器人」管理账号与运行状态 |
| 应用端 | 处理命令、插件和 AI 对话，例如 NoneBot2、Karin、AstrBot、Koishi | 「应用端」创建实例、配置和对接 |
| Desktop | 帮你安装、启动和管理前两者，并提供日志、聊天、调试与远端管理 | 你正在使用的 Windows 桌面程序 |

只安装 Desktop，QQ 账号不会自动登录；只登录 Bot，也不会自动获得插件回复。要让它响应某个指令，还需要运行对应的应用端，并建立 OneBot 连接。

## 第一次使用，按这条路线读

1. **[下载安装](/docs/installation/)**：选对 Windows 安装包，确认 WebView2 和运行环境。
2. **[创建第一个 Bot](/docs/first-bot/)**：在本机准备组件，填写 QQ 账号，启动并完成登录。
3. **[连接应用端与 OneBot](/docs/connections/)**：先用 HTTP 查询登录信息，再按需要对接应用框架。
4. **[安装应用端](/docs/apps/)**：创建一个实例，完成 Bot 对接，验证应用端收到消息。
5. **[备份、迁移与恢复](/docs/backup/)**：把刚配置好的内容导出，弄清哪些数据需要另行备份。

入门教程默认使用 **Windows 本机 + NapCat + 直接运行**。需要在服务器上运行时，从 [远端 Linux](/docs/remote-linux/) 或 [远端 Windows](/docs/remote-windows/) 开始；本机不支持 Docker 部署。

## 已经装好，按任务找文档

| 你现在想做什么 | 继续阅读 |
| --- | --- |
| 看运行摘要、待处置与最近通知 | [概览](/docs/overview/) |
| 安装 QQ、NapCat、SnowLuma 或更新组件 | [组件管理](/docs/components/) |
| 修改 Bot、批量启停、设置自动重启 | [机器人管理](/docs/bots/) |
| 选择 HTTP / 正向 WS / 反向 WS，排查连接 | [连接应用端与 OneBot](/docs/connections/) |
| 管理应用端插件、模型与框架配置 | [框架功能](/docs/frameworks/) |
| 私聊、群聊、群文件、聊天截图 | [聊天与截图](/docs/chat/) |
| 调用 OneBot 接口、查看事件和保存请求 | [API 调试台](/docs/debugging/) |
| 使用本机与 SSH 终端 | [终端](/docs/terminal/) |
| 找安装失败原因、查看 Bot 与应用日志 | [任务与日志](/docs/tasks-logs/) |
| 添加 SSH 主机、准备服务器 | [远端 Linux](/docs/remote-linux/) · [远端 Windows](/docs/remote-windows/) |
| 在服务器上使用容器运行 Bot | [Docker](/docs/docker/) |
| 给远端 SnowLuma 扫码登录 | [远端登录](/docs/remote-login/) |
| Desktop 退出后继续监控远端 Bot | [ncd-watch 值守](/docs/watch/) |
| 下载慢、配置代理和 GitHub 线路 | [网络设置](/docs/network/) |
| 调整主题、通知、功能入口与托盘 | [设置](/docs/settings/) |
| 导出配置、换盘、清理下载缓存 | [备份与数据](/docs/backup/) |
| 不知道从哪查起 | [故障排查](/docs/troubleshooting/) |

## 先看演示，再跟着做

组件、Bot、连接、应用端、聊天、调试台、设置和日志教程都可以直接试。点「自动演示」看一遍，随时点「自己操作」接手；「重置」会恢复这篇教程的初始数据。

截图来自实际前端界面，使用项目自带的浏览器演示数据；账号、消息、服务器和状态用于说明操作位置。明色、暗色截图展示同一个步骤。你自己的版本、主题、已安装组件和功能开关可能让页面略有不同。

交互演示也复用实际前端，安装进度、接口返回和消息使用样例数据。它不会安装程序、登录 QQ、发送真实消息或修改电脑设置；完成演示后，再按正文在自己的 Desktop 中操作。

跟着做时，请把示例账号换成你准备登录的 QQ 号，把服务器地址换成自己的主机。端口和路径要与应用端实际配置一致。教程中的占位 Token 不能拿来作为长期密码。

## 遇到问题时留下什么

先记下 **Desktop 版本、协议端及版本、运行主机、直接运行或 Docker、失败前最后一步**，再查看对应的任务和日志。提交 Issue 时说明预期结果与实际结果，并隐藏密码、Token、API Key、SSH 私钥和私人消息。

本文档依据 [main 分支用户指南](https://github.com/NapNeko/NapCatQQ-Desktop/blob/main/docs/user/README.md) 与实际界面整理。各框架自己的插件行为、模型费用和 QQ 账号限制仍以相应项目或服务的说明为准。
