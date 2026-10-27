---
title: 下载与安装
description: 选对安装包，完成 Windows 安装，认识第一次启动后的页面与数据位置。
section: 开始使用
order: 10
---

先安装 Desktop。QQ、NapCat / SnowLuma 和应用框架可以稍后在「组件」页按需安装。

## 1. 看看系统支不支持

| 项目 | 要求 |
| --- | --- |
| 桌面客户端 | Windows 10 / 11 x64，或 Windows Server 2016+ |
| WebView2 | 没有的话安装包会自动装上 |
| 内置本机终端 | Windows 10 1809 / Windows Server 2019+（需要 ConPTY） |
| 远端运行 | 通过 SSH 管理 Linux 或 Windows 主机 |
| 磁盘 | 需要为 QQ、协议端、应用端和数据预留空间；实例越多，占用越大 |

Desktop 目前只有 Windows 版。要在 Linux 服务器上运行 Bot，仍需将 Desktop 安装在 Windows 电脑上，通过 SSH 管理服务器。本机只能直接运行，Docker 部署仅支持远端 Linux。

## 2. 下载安装包

1. 打开 [GitHub 最新版本发布页](https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest)。
2. 在 Assets 里找到 `NapCatQQ-Desktop-<版本>-x64.msi` 下载。旁边的 `NapCatQQ-Desktop-x64.msi` 是同一个文件换了个名字，下哪个都一样。

发布页上带 `watch-v` 的版本是远端值守程序 ncd-watch，`Source code` 是源码压缩包；Desktop 安装包请选 MSI。

GitHub 打不开或者下得很慢，就先想办法解决网络问题。装好以后，组件的下载线路可以在「设置 → 网络」里换，见 [网络设置](/docs/network/)。

## 3. 安装并启动

1. 双击 MSI，按向导一路往下走。电脑上没有 WebView2 的话，安装程序会联网装上，等它装完就好。
2. 装完以后，从开始菜单或桌面快捷方式打开 NapCatQQ Desktop。

打开主窗口后，确认侧栏可以切换到「机器人」「组件」等页面。新安装时机器人列表为空，这是正常的。

如果窗口空白或启动报错，先记录错误信息，检查 WebView2 是否安装成功。不要急于删除数据目录，以免丢失数据。

## 4. 认识几个常用入口

| 入口 | 用来做什么 |
| --- | --- |
| 组件 | 选「本机」，装 QQ 和 NapCat / SnowLuma |
| 机器人 | 新建账号配置，启动、登录、看日志 |
| 应用端 | 安装插件框架、对接 Bot |
| 远端 | 要在别的机器上运行时才用得到 |
| 任务 | 下载、安装、更新和部署进度 |
| 设置 | 主题、网络、通知、功能入口和数据 |

有些入口可以在「设置 → 功能」里关掉，所以你的侧栏要是和截图对不上，先去那里看看。想换明暗主题，在「设置 → 外观」，见 [设置](/docs/settings/)。

## 5. 数据放在哪

默认数据根目录是：

```text
%ProgramData%\NapCatQQ Desktop
```

实际路径可以在「设置 → 数据」里看到。安装目录和数据目录是分开放的，所以升级 Desktop 不需要动数据。系统盘不够用的话，点「数据根目录」旁边的「迁移」换到别的盘，见 [备份与数据](/docs/backup/)。Bot 或应用端还在运行的时候，别手动去剪切这个目录。

## 升级

可以安装新版 MSI 覆盖旧版，也可以在应用内检查更新。升级前建议停止正在运行的实例、导出配置，并查看发布说明中的版本要求。

Desktop、NapCat、SnowLuma 和应用框架分别更新。升级 Desktop 不会更新协议端或应用实例；可在「组件」或实例详情中查看它们的版本。

## 下一步

装好了就去 [创建第一个 Bot](/docs/first-bot/)。打算直接上服务器的，看 [远端 Linux](/docs/remote-linux/) 或 [远端 Windows](/docs/remote-windows/)。
