---
title: 下载与安装
description: 选对安装包，完成 Windows 安装，认识第一次启动后的页面与数据位置。
section: 开始使用
order: 10
---

这一篇只装 Desktop 本身。QQ、NapCat / SnowLuma 和各种应用框架，后面在「组件」页里用到哪个装哪个，不用现在一口气全下下来。

## 1. 看看系统支不支持

| 项目 | 要求 |
| --- | --- |
| 桌面客户端 | Windows 10 / 11 x64，或 Windows Server 2016+ |
| WebView2 | 没有的话安装包会自动装上 |
| 内置本机终端 | Windows 10 1809 / Windows Server 2019+（需要 ConPTY） |
| 远端运行 | 通过 SSH 管理 Linux 或 Windows 主机 |
| 磁盘 | QQ、协议端、应用端和数据都要占地方，实例越多占得越多 |

目前只有 Windows 版，没有 Linux / macOS 客户端。想把 Bot 放到 Linux 服务器上的话，Desktop 还是装在你的 Windows 电脑上，通过 SSH 去管服务器，后面有专门的章节讲。本机只能直接运行，Docker 部署只在远端 Linux 上用得到。

## 2. 下载安装包

1. 打开 [GitHub 最新版本发布页](https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest)。
2. 在 Assets 里找到 `NapCatQQ-Desktop-<版本>-x64.msi` 下载。旁边的 `NapCatQQ-Desktop-x64.msi` 是同一个文件换了个名字，下哪个都一样。

同一个页面上还能看到带 `watch-v` 的发布和 `Source code` 压缩包。前者是远端值守用的 ncd-watch，后者是源码，都不是你要找的安装包。

GitHub 打不开或者下得很慢，就先想办法解决网络问题。装好以后，组件的下载线路可以在「设置 → 网络」里换，见 [网络设置](/docs/network/)。

## 3. 安装并启动

1. 双击 MSI，按向导一路往下走。电脑上没有 WebView2 的话，安装程序会联网装上，等它装完就好。
2. 装完以后，从开始菜单或桌面快捷方式打开 NapCatQQ Desktop。

能看到主窗口，侧栏能切到「机器人」「组件」这些页面，就算装好了。列表里空空的不用紧张，你还没建 Bot 呢。

要是窗口一片空白，或者一启动就报错，先把错误信息记下来，再看看 WebView2 有没有装成功。别急着去删数据目录，那样多半解决不了问题，还可能把别的东西弄丢。

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

有两种办法：直接装新版 MSI 覆盖旧版，或者在应用里检查更新。升级前最好先停掉正在干活的实例，导出一份配置，再扫一眼发布说明里有没有特别的版本要求。

还有一点容易误会：Desktop、NapCat、SnowLuma 和各个应用框架的版本是各管各的。Desktop 升级了，协议端和应用实例不会跟着升，它们的版本要到「组件」或实例详情里单独看。

## 下一步

装好了就去 [创建第一个 Bot](/docs/first-bot/)。打算直接上服务器的，看 [远端 Linux](/docs/remote-linux/) 或 [远端 Windows](/docs/remote-windows/)。
