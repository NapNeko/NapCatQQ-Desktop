---
title: 下载与安装
description: 选对安装包，完成 Windows 安装，认识第一次启动后的页面与数据位置。
section: 开始使用
order: 10
---

本页先把 Desktop 装好。QQ、NapCat / SnowLuma 和应用框架会在之后的「组件」页按需安装，不需要提前把所有东西都下载一遍。

## 1. 确认电脑是否支持

| 项目 | 要求 |
| --- | --- |
| 桌面客户端 | Windows 10 / 11 x64，或 Windows Server 2016+ |
| 页面运行环境 | Microsoft Edge WebView2；安装包会在缺少时处理安装 |
| 内置本机终端 | Windows 10 1809 / Windows Server 2019+，需要 ConPTY |
| 远端运行 | 从 Windows Desktop 经 SSH 管理 Linux 或 Windows 主机 |
| 磁盘与内存 | 除 Desktop 外，还要为 QQ、协议端、应用端和数据预留空间；实例越多，占用越多 |

目前没有 Linux / macOS 桌面客户端。本机运行方式为「直接运行」；Docker 用于远端 Linux。不要把 Windows 的桌面安装包上传到 Linux 当成服务端程序运行。

## 2. 下载 Desktop 安装包

1. 打开 [GitHub 最新版本发布页](https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest)。
2. 在 Assets 中找 `NapCatQQ-Desktop-<版本>-x64.msi`。
3. 下载到本地，等待浏览器完成下载，再双击 MSI。

`NapCatQQ-Desktop-x64.msi` 是同内容的别名，也可以使用。带 `watch-v` 的发布属于远端值守组件 **ncd-watch**，不是 Desktop 安装包。`Source code` 是源码压缩包，也不是安装程序。

如果无法访问或下载很慢，可以先检查系统网络。成功进入 Desktop 后，组件下载线路可在「设置 → 网络」调整，见 [网络设置](/docs/network/)。不要把网页保存为 `.msi`，也不要反复打开尚未下载完的文件。

## 3. 完成安装并启动

1. 双击 MSI，按安装向导继续。
2. 如果安装向导需要准备 WebView2，等待安装结束；这一步需要可用的下载网络。
3. 安装完成后，从开始菜单或桌面快捷方式打开 NapCatQQ Desktop。
4. 首次打开时先查看界面，不必立即填写服务器或 Token。

**完成标志：** Desktop 主窗口能正常显示，侧栏可以切换到「机器人」「组件」等页面。此时列表为空是正常的，你还没有创建 Bot。

安装过程中如果提示权限不足，先看向导具体要求。遇到空白窗口或启动报错，记录错误内容并检查 WebView2 是否安装完成；不要靠删除数据目录来试错。

## 4. 认识几个常用入口

| 入口 | 第一次使用时会做什么 |
| --- | --- |
| 组件 | 选择「本机」，准备 QQ 和 NapCat / SnowLuma |
| 机器人 | 新增账号配置，启动、登录、看日志 |
| 应用端 | 后续安装插件框架、连接 Bot |
| 远端 | 只有准备在另一台机器上运行时才需要 |
| 任务 | 看下载、安装、更新和部署进度 |
| 设置 | 修改主题、网络、通知、功能入口和数据设置 |

部分入口可以在「设置 → 功能」关闭。如果你和截图看到的侧栏不同，先检查功能开关。想看明色或暗色界面，可以在「设置 → 外观」调整，具体见 [设置](/docs/settings/)。

## 5. 找到数据位置

默认数据根目录为：

```text
%ProgramData%\NapCatQQ Desktop
```

在「设置 → 数据」可以查看当前路径。安装目录与数据目录是两回事；升级 Desktop 不需要先删除数据。如果系统盘空间不足，用「数据根目录」旁的「迁移」完成换盘，步骤见 [备份与数据](/docs/backup/)。

不要在 Bot 或应用端运行时直接剪切整个数据目录。先完成首次配置，再导出一份配置 ZIP，后面排错或换机时会省很多事。

## 之后如何升级

可以安装新版 MSI 覆盖升级，也可以从应用内检查更新。升级前先停止正在处理重要任务的实例，导出配置，并查看发布说明中是否有组件或框架版本要求。

Desktop、NapCat、SnowLuma 和应用框架有各自的版本。更新 Desktop 不代表所有协议端与应用实例都已更新；它们的当前版本要到「组件」或实例详情里确认。

## 下一步

继续 [创建第一个 Bot](/docs/first-bot/)。如果你准备把 Bot 放到服务器上，改读 [远端 Linux](/docs/remote-linux/) 或 [远端 Windows](/docs/remote-windows/)。

来源：[main 用户指南：安装与数据](https://github.com/NapNeko/NapCatQQ-Desktop/blob/main/docs/user/README.md)。
