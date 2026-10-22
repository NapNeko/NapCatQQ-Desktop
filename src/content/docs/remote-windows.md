---
title: 管理远端 Windows
description: 配置 OpenSSH、安装 Windows 远程代理，并把 QQ 运行在正确的用户桌面会话里。
section: 远端与部署
order: 110
---

除了 Linux，Desktop 也能管另一台 Windows 机器上的 Bot，方式是「SSH + Windows 远程代理」。和 Linux 最大的不同在于，Windows 上的 QQ 必须跑在一个已经登录的用户桌面里。只连上 SSH、只开着后台服务都不够，得真的有个用户登录进了桌面。

这部分功能还比较新，建议先用一个 Bot 跑通，确认没问题了，再把其他实例迁过来。

## 1. 准备目标 Windows

目标机需要是 Windows 10 1809 及以上、Windows 11，或者 Windows Server 2019 及以上，并且有 PowerShell 5.1、OpenSSH Server，以及能运行 QQ 的桌面环境。

第一次安装远程代理时，要用目标机上的管理员 SSH 账户。之后就一直用安装时绑定的那个 Windows 用户。这里认的是用户的 SID，所以就算用户名一样，SID 不同也会被当成另一个用户。

如果目标机还没开 SSH，在那台机器上用管理员身份打开 PowerShell，运行：

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Start-Service sshd
Set-Service -Name sshd -StartupType Automatic
Get-NetFirewallRule -Name OpenSSH-Server-In-TCP
```

最后一条命令用来查看安装时自动创建的防火墙规则，按你的网络情况调整允许的范围和端口。中间隔着 NAT 的话，还要保证路由能通。Windows 家庭版也能装 OpenSSH，用 SSH 管理也不需要 RDP。

如果公司或学校的策略不允许安装可选功能、执行脚本或者启动服务，这一步就得找目标机的管理员帮忙了，可以参考 [Microsoft OpenSSH 安装说明](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)。

## 2. 在 Desktop 添加主机

1. 打开「远端」，添加一个服务器档案。
2. 填主机地址、SSH 端口和 Windows 用户名，「目标系统」选「Windows」或者「自动识别」。
3. 选密码认证或者私钥认证。注意 Windows Hello 的 PIN 码不是 SSH 密码；要用能通过 OpenSSH 认证的本地账户或域账户，Microsoft Entra 账户是不支持的。
4. 保存后测试连接，第一次连接时核对主机指纹。

主机指纹可以在目标机的 PowerShell 里查，下面这条命令显示的是默认的 Ed25519 主机密钥指纹：

```powershell
ssh-keygen.exe -lf (Join-Path $env:ProgramData 'ssh\ssh_host_ed25519_key.pub')
```

核对的时候，选和 Desktop 弹窗里同一种算法的指纹来比。管理员账户的公钥一般放在 `%ProgramData%\ssh\administrators_authorized_keys`，普通用户放在个人目录下的 `.ssh\authorized_keys`。让 Desktop 来配免密登录的话，这些路径和权限它会自己处理；想手动配置，可以看 [Windows OpenSSH 密钥管理](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement)。

用密码连通以后，可以点远端卡片上的钥匙图标「配置免密登录」，输入「当前 SSH 密码」确认。Desktop 会写入公钥，用新私钥实际登录验证一次，成功后自动把档案切到密钥认证。新建档案时勾上「自动配置免密登录」也能一步到位，完整流程见 [远端主机的免密设置](/docs/remote-linux/)。私钥不在配置 ZIP 里，换电脑时要另外搬过去，或者重新配一次。

## 3. 启用托管

1. 连接成功后，在远端卡片上打开「Windows · 管理」。
2. 点「启用托管」。也可以去「组件」切到这台主机，安装「Windows 远程代理」。进度在任务队列里看。
3. 记下管理页上显示的 SSH 账户，然后用这个账户登录目标机的 Windows 桌面。安装过程会给这个用户注册一个登录启动任务，所以 SSH 会话代替不了桌面登录，必须真的登进桌面。
4. 回到「Windows · 管理」刷新。托管服务应该显示「运行中」，QQ 会话应该显示「工作进程已就绪」。
5. 如果还一直在等待，用管理页给出的用户会话绑定命令修复一下，再刷新。

机器代理的文件放在目标机的 `%ProgramData%\NapCatQQ Desktop\remote` 下，组件、应用和用户数据放在它下面按用户 SID 分开的 `users` 目录里。具体路径以管理页显示的为准，和你这台控制端电脑的数据根目录没有关系。

| 当前状态 | 意思 | 怎么办 |
| --- | --- | --- |
| 只有基础 SSH 连接 | 能查库存、传文件、开 PowerShell | 继续装远程代理 |
| 服务运行，等待桌面会话 | 机器代理在，用户工作进程还没起来 | 用绑定的用户登录桌面 |
| 工作进程已就绪 | 可以启停原生 Bot 和应用、读日志 | 创建 Bot 并登录 |
| 桌面已锁定 | 后台进程可能还在，但扫不了码 | 解锁桌面后刷新二维码 |
| 有多个用户会话 | 不知道 QQ 该用哪个桌面 | 只保留目标用户的会话，再刷新 |

## 4. 安装组件并启动 Bot

1. 到「组件」切到这台 Windows 主机。
2. 装 QQ、NapCat / SnowLuma 和运行时。已经装过的，从「远端 → 库存」刷新探测就行。
3. 核对一下目标机上的安装路径，比如 `D:\NapCat`。路径里有空格或中文也没关系，直接填完整路径。
4. 新建 Bot，选这台远端主机，运行方式选原生。
5. 启动以后，从卡片上打开 WebUI、扫码或者日志，完成 QQ 登录。

Windows 原生部署用不到 Linux 那套 Xvfb、VNC 或 systemd。扫码之前，先确认目标用户已经登录了桌面，而且没有锁屏，再去刷新二维码。QQ 登录成功和协议端就绪是两个步骤，以卡片上的状态和日志为准。

## 5. 应用、终端与定时任务

在这台 Windows 上装应用实例：到「组件」选中这台主机，在框架卡片上点「新建实例」。已经有的项目，从「应用端 → 导入已有项目」进去，在向导里选这台主机。应用需要的运行时，同样要装在这台 Windows 上。

如果要导入的应用已经被别的终端或启动器跑起来了，先在原来的地方把它停掉，再从 Desktop 启动，这样 Desktop 才能真正托管它。Desktop 不会因为看到了一个进程 PID，就擅自去接管或者结束它。

从主机或 Bot 的终端入口打开的，是目标机上的 PowerShell，命令和环境变量都是那台机器的。关掉终端窗口，里面跑着的前台命令不会自动变成托管任务。

原生 Bot 的定时重启由目标用户的工作进程负责执行，Cron 按目标 Windows 的本地时区来算。Desktop 退出了，计划照样会执行；但如果目标用户注销了，就要等他重新登录后才会继续。

需要后台通知的话，在组件页给这台主机启用 ncd-watch，再去配 [远端值守](/docs/watch/)。Windows 上的值守由远程代理来承载，但 QQ 登录和二维码这些，依然离不开桌面会话。

## 退出、重启与升级

- 退出你这边的 Desktop，不会停掉目标机上托管的进程。SSH 断开了也一样，恢复连接后刷新一下状态，再决定要不要启动。
- 注销目标用户会结束他的桌面会话。后台服务没法顶替这个用户会话去运行 QQ。
- 目标机重启以后能不能自动恢复，取决于用户会不会重新登录，以及启动任务是怎么配置的。
- 升级 Desktop 以后，管理员可以在管理页点「重新部署托管」来更新代理。已经在运行的用户工作进程还是旧版本，要等用户下次登录才会换成新的；部署失败的话，会自动恢复到原来的服务版本。

## 常见卡点

**SSH 能连，代理安装失败：** 核对一下 SSH 账户有没有管理员权限，再看任务队列里的具体错误。远端操作不会在你的电脑上弹出目标机的 UAC 确认窗口，所以权限必须提前就有。

**服务在运行，QQ 一直等会话：** 确认登录桌面的用户和 SSH 用户是同一个 SID，没有注销，也没有残留多个会话。用另一个管理员账户登录桌面是不行的，必须是绑定的那个用户。

**重连后状态看起来不对：** 先刷新主机和 Bot 的状态，看日志确认原来的进程还归谁管。状态没搞清楚之前，别手动再启动一份同账号的 Bot 或者同一个应用。
