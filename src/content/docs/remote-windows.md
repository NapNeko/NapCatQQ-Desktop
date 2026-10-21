---
title: 管理远端 Windows
description: 配置 OpenSSH、安装 Windows 远程代理，并把 QQ 运行在正确的用户桌面会话里。
section: 远端与部署
order: 110
---

远端 Windows 使用「SSH 连接 + Windows 远程代理」管理原生 Bot 和应用。与 Linux 不同，QQ 还需要目标用户登录 Windows 桌面，只有 SSH 连接或机器服务运行都不够。

当前 `main` 已完成 Windows Server 2022 的 SSH、文件传输、命令、隧道和代理探针实测；服务部署升级、用户会话恢复及真实 QQ 扫码仍待完整远端验收。首次部署建议先验证一个 Bot，再迁移其他实例。

## 1. 准备目标 Windows

目标机需要 Windows 10 1809+、Windows 11 或 Windows Server 2019+，具备 PowerShell 5.1、OpenSSH Server 和 QQ 所需桌面环境。

首次安装远程代理使用目标机上的管理员 SSH 账户。以后继续使用安装时绑定的同一个 Windows 用户；相同用户名、不同 SID 也会被视为不同用户。

尚未开启 SSH 时，在目标机的**管理员 PowerShell**运行：

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Start-Service sshd
Set-Service -Name sshd -StartupType Automatic
Get-NetFirewallRule -Name OpenSSH-Server-In-TCP
```

最后一条用于检查安装程序创建的防火墙规则。把连接范围和端口配置成你的实际网络要求；经过 NAT 时还需要可达的网络路径。Windows 家庭版也可以使用 OpenSSH，SSH 管理本身不依赖 RDP。

如果组织策略禁止安装可选功能、执行脚本或启动服务，需要由目标机管理员处理。可对照 [Microsoft OpenSSH 安装说明](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)。

## 2. 在 Desktop 添加主机

1. 打开「远端」，添加服务器档案。
2. 填写主机地址、SSH 端口和 Windows 登录用户名。「目标系统」选择「Windows」或「自动识别」。
3. 选择密码或私钥认证。Windows Hello PIN 不是 SSH 密码；使用能通过 OpenSSH 认证的本地或域账户，Microsoft Entra 账户不在其支持认证范围内。
4. 保存后测试连接，首次连接时核对主机指纹。

在目标机 PowerShell 可查看默认 Ed25519 主机密钥指纹：

```powershell
ssh-keygen.exe -lf (Join-Path $env:ProgramData 'ssh\ssh_host_ed25519_key.pub')
```

核对时要选与 Desktop 弹窗相同的算法。默认管理员账户公钥通常使用 `%ProgramData%\ssh\administrators_authorized_keys`，普通用户使用个人目录的 `.ssh\authorized_keys`。Desktop 配置免密会处理对应路径与权限，手工配置时可查 [Windows OpenSSH 密钥管理](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement)。

密码连接验证成功后，可以点远端卡片的钥匙图标「配置免密登录」，输入「当前 SSH 密码」并确认。Desktop 会核对主机、写入公钥，并验证生成的私钥确实能登录，成功后自动将档案切到密钥认证。新建档案时也可勾选「自动配置免密登录」一起完成；详细流程见 [远端主机的免密设置](/docs/remote-linux/)。私钥不包含在配置 ZIP 中，换机需另行迁移或重新配置。

## 3. 启用托管，让桌面工作进程就绪

1. 连接成功后，在远端卡片打开「Windows · 管理」。
2. 点击「启用托管」；也可以到「组件」切换到这台主机，安装「Windows 远程代理」。在任务队列查看进度。
3. 看清管理页展示的 **SSH 账户**，用这个用户登录目标 Windows 的桌面。安装会注册这个用户的登录启动任务，单独开一个 SSH 会话不能代替桌面登录。
4. 回到「Windows · 管理」刷新。托管服务应为「运行中」，QQ 会话应为「工作进程已就绪」。
5. 如果仍在等待，按管理页提供的用户会话绑定命令修复，再刷新确认。

机器代理文件位于目标机 `%ProgramData%\NapCatQQ Desktop\remote`。组件、应用和用户数据位于下面按用户 SID 隔离的 `users` 目录。以管理页显示的实际目标路径为准，控制端的数据根不会决定远端路径。

| 当前状态 | 意味着什么 | 处理方式 |
| --- | --- | --- |
| 只有基础 SSH 连接 | 能检测库存、传输文件、打开 PowerShell | 继续安装远程代理 |
| 服务运行，等待桌面会话 | 机器代理在，用户工作进程未就绪 | 用绑定用户登录桌面 |
| 工作进程已就绪 | 可启停原生 Bot 和应用、读取日志 | 创建 Bot 并完成登录 |
| 桌面已锁定 | 后台进程可能仍运行，扫码不可用 | 解锁目标桌面后刷新二维码 |
| 同时存在多个用户会话 | 无法确定 QQ 应使用哪个桌面 | 保留一个目标用户会话后刷新 |

## 4. 安装组件并启动 Bot

1. 到「组件」切换到这台 Windows 主机。
2. 安装 QQ、选用的 NapCat / SnowLuma 和运行时。已有安装可以从「远端 → 库存」刷新探测。
3. 核对目标 Windows 上的安装路径，例如 `D:\NapCat`。带空格或中文时直接填写完整路径，不要填控制端的同名目录。
4. 新建 Bot，选择目标远端主机和原生运行方式。
5. 启动后，从 Bot 卡片打开 WebUI、扫码或日志入口，完成 QQ 登录。

Windows 原生部署不需要 Linux 的 Xvfb、VNC 或 systemd。扫码时，先确认目标用户桌面已登录且已解锁，再刷新二维码。QQ 登录成功和协议端就绪是两步，以卡片状态和日志为准。

## 5. 管理应用、终端与定时任务

安装应用实例时，到「组件」选择这台 Windows 主机，再从框架卡片点击「新建实例」。已有项目从「应用端 → 导入已有项目」进入，在向导里选择这台主机；完成后从实例卡片启停、读取日志。运行时仍需装在这台目标 Windows 上。如果导入的应用已从其他终端或启动器运行，先在原入口停止，再从 Desktop 启动以建立托管归属；Desktop 不会只凭发现一个 PID 就接管或终止它。

从主机或 Bot 的终端入口打开的是目标 Windows 的 PowerShell，命令与环境变量都属于目标机。关闭终端不会把其中的普通前台命令变成托管任务。

原生 Bot 的定时重启由目标用户工作进程执行，Cron 使用**目标 Windows 的本地时区**。Desktop 退出后计划仍可执行；目标用户注销期间，要等待用户重新登录。

需要后台通知时，在组件页给这台主机启用 ncd-watch，再配置 [远端值守](/docs/watch/)。Windows 的值守由远程代理承载；涉及 QQ 登录和二维码仍依赖桌面会话，不使用本页以外的 Linux systemd 命令。

## 退出、重启与升级前要知道什么

- 退出控制端 Desktop 不等于停止目标机托管的进程。SSH 断开后，恢复连接并刷新状态再决定是否需要启动。
- 注销目标用户会结束其桌面会话。机器服务不能替代这个用户会话继续运行 QQ。
- 机器重启后的恢复取决于用户重新登录以及对应任务的启动配置。
- 升级 Desktop 后，管理员可在管理页使用「重新部署托管」更新随附代理。现有用户工作进程仍使用原版本，新会话工作进程在下次登录启用；部署失败会恢复原服务版本。

## 常见卡点

**SSH 能连，代理安装失败：**核对 SSH 账户的管理员权限，再看任务队列里的具体错误。远端操作不会弹出一个可在控制端点击的目标机 UAC 窗口。

**服务在运行，QQ 一直等会话：**确认桌面用户与 SSH 用户 SID 相同，未注销且没有残留多个会话。另一个管理员的桌面不能代替绑定用户。

**重连后状态看起来不对：**先刷新主机和 Bot 状态，查看日志确认原进程归属。不要在状态未知时手工再启动一份相同账号或应用。

本页依据：[main 的远端 Windows 指南](https://github.com/NapNeko/NapCatQQ-Desktop/blob/main/docs/user/windows-remote.md)。
