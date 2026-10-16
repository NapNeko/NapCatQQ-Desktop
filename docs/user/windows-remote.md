# 远端 Windows 管理

Desktop 通过 Windows OpenSSH 连接目标机；安装 Windows 远程代理后，可管理目标机上的原生 Bot、应用和后台值守。QQ 与扫码还需要目标用户登录 Windows 桌面。

当前已完成代码检查、本机隔离验证和 Windows Server 2022 上的 SSH、文件传输、命令、隧道及代理探针实测。服务部署与升级、用户会话恢复、QQ 扫码登录仍待真实远端验收。

## 连接前准备

- 目标机使用 Windows 10 1809+、Windows 11 或 Windows Server 2019+，有 PowerShell 5.1 和 OpenSSH Server。QQ 运行需要桌面环境。
- 首次安装代理需要目标机上的管理员 SSH 账户。以后连接仍应使用安装时绑定的同一个 Windows 用户；用户名相同但 SID 不同也不属于同一用户。
- 控制端能访问目标机的 SSH 端口。可以使用局域网、已有 VPN 或受控的公网入口；代理不额外开放管理 TCP 端口。

Windows 家庭版也可使用 OpenSSH，远程管理不依赖 RDP。企业策略禁止安装可选功能、执行脚本或启动服务时，需要目标机管理员处理相应策略。

### 目标机尚未启用 SSH

在目标 Windows 上打开**管理员 PowerShell**，安装并启动系统 OpenSSH Server：

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Start-Service sshd
Set-Service -Name sshd -StartupType Automatic
Get-NetFirewallRule -Name OpenSSH-Server-In-TCP
```

最后一条用于检查安装程序创建的防火墙规则。连接范围和端口应由目标机管理员按网络环境配置；NAT 后面的主机还需要可达的网络路径。详见 Microsoft 的 [Windows OpenSSH 安装说明](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)。

### 添加并测试连接

1. 打开「远端」，添加主机地址、端口和 Windows 登录用户名。目标系统可选「自动识别」或「Windows」。
2. 使用密码或私钥认证。Windows Hello PIN 不是 SSH 密码；Microsoft Entra 账户不在 Windows OpenSSH 的支持认证范围内，使用可用于 SSH 的本地或域账户。
3. 测试连接。首次出现主机指纹时，在目标机核对后再信任。远端卡片里的 Windows 管理入口会显示目标身份、目录和托管状态。

可在目标机 PowerShell 核对默认 Ed25519 主机密钥指纹：

```powershell
ssh-keygen.exe -lf (Join-Path $env:ProgramData 'ssh\ssh_host_ed25519_key.pub')
```

核对的算法必须与 Desktop 弹窗显示的一致。管理员账户的公钥通常保存在目标机 `%ProgramData%\ssh\administrators_authorized_keys`，普通用户使用个人目录下的 `.ssh\authorized_keys`；Desktop 配置免密时会处理对应路径和权限。系统规则见 [Windows OpenSSH 密钥管理](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement)。

## 启用托管与桌面会话

1. 连接成功后，打开卡片上的「Windows · 管理」，点击「启用托管」。也可以到「组件」选择这台主机，安装「Windows 远程代理」。进度与失败原因在任务队列中显示。
2. 安装会注册该用户的登录启动任务。使用管理页显示的 **SSH 账户**登录目标机桌面，使用户工作进程启动。SSH 登录本身不能代替桌面登录。
3. 回到管理页刷新。托管服务应显示「运行中」，QQ 会话应显示「工作进程已就绪」。如果仍在等待，管理页提供目标机上的用户会话绑定命令；按页面提示修复后再刷新。

机器代理文件位于目标机的 `%ProgramData%\NapCatQQ Desktop\remote`。组件、应用和用户数据位于它下方按用户 SID 隔离的 `users` 目录；管理页显示实际目标路径。控制端自己的数据根不决定远端路径。

| 状态 | 可进行的操作 | 下一步 |
| :--- | :--- | :--- |
| 基础连接 | 检测库存、传输文件、打开 PowerShell 终端 | 安装代理以运行托管任务 |
| 托管服务运行，等待用户桌面会话 | 查看主机与代理状态 | 用绑定用户登录桌面并刷新 |
| 工作进程已就绪 | 启停原生 Bot、应用与读取日志 | 在 Bot 卡片完成 QQ 登录 |
| 桌面已锁定 | 后台进程可能继续运行，扫码不可用 | 解锁目标用户桌面后扫码 |
| 存在多个用户会话 | 不能自动确定 QQ 使用哪个桌面 | 保留一个目标用户会话再刷新 |
| SSH 断开 | 已托管任务由目标机继续管理，控制端状态待刷新 | 恢复网络并重新连接 |

退出 Desktop 不等于停止目标机上的托管进程。注销目标用户会结束其桌面会话；QQ 与该会话中的工作负载不能继续依靠机器服务代替运行。服务重启和机器重启后的恢复还取决于用户重新登录及对应任务的启动配置。

升级 Desktop 后，管理员可在管理页点击「重新部署托管」，将随附的代理更新到目标机。已有用户工作进程继续使用其原版本，新的会话工作进程在下次登录时启用；部署失败会恢复原服务版本。

## 准备组件并创建 Bot

在「组件」选择这台 Windows 主机，安装 QQ、NapCat 或 SnowLuma 以及所需运行时。已有安装可从「远端 → 库存」刷新探测结果，核对或填写目标机上的 Windows 路径，例如 `D:\NapCat`；使用带空格或中文的路径时直接填写完整路径。

在「机器人」新建 Bot，运行位置选择该远端主机，部署方式选择原生。启动后使用 Bot 卡片提供的 WebUI、扫码、日志与调试入口。Windows 原生管理不需要 Linux 的 Xvfb、VNC 或 systemd。

QQ 需要绑定用户的桌面工作进程。没有桌面会话时应先按管理页完成登录；锁屏时先解锁，随后刷新二维码。实际扫描、账号登录和协议端就绪是独立步骤，以 Bot 的状态和日志为准。

原生 Bot 的定时重启由目标机用户工作进程执行，Cron 使用目标 Windows 的本地时区。Desktop 退出后计划继续执行；用户注销期间等待下次登录。

## 应用、终端与值守

- **应用**：在「应用端」选择这台主机，安装或导入实例，再从实例卡片启停与查看日志。Windows 实例由远程代理监督；运行时依赖仍在对应主机的组件页准备。
- **终端**：从主机或 Bot 的终端入口打开目标机 PowerShell。命令、路径与环境变量属于目标 Windows；终端关闭不会变成后台托管任务。
- **值守**：在这台主机的组件页启用 ncd-watch，再从 Bot 的值守与通知入口配置。Windows 值守由远程代理承载；涉及 QQ 登录恢复或二维码时仍需要用户桌面会话。通用通知配置见 [ncd-watch](ncd-watch.md)。

导入的应用如果已由其他终端或启动器运行，先在原入口停止，再由 Desktop 启动以建立托管归属。仅凭发现的 PID 不会接管或终止该进程。

## 常见问题

**能连 SSH，却无法安装代理**：检查管理页的 SSH 账户是否为管理员，查看任务队列的具体错误。远端操作不会弹出可由控制端确认的目标机 UAC 窗口；请使用具备安装权限的目标 SSH 会话。

**服务运行，但 QQ 一直等待会话**：核对登录桌面的 Windows 用户是否与 SSH 用户 SID 相同，确认没有注销或残留多个会话，再刷新管理页。不能通过另一个管理员的桌面代替绑定用户。

**安装了 QQ 或框架仍未识别**：刷新库存，核对目录是否属于目标机以及是否包含正确的程序文件。在组件页查看检测结果，不要填写控制端本机的安装路径。

**连接恢复后状态不一致**：先刷新主机与 Bot 状态，再决定启停。状态不明时不要在目标机手动再开一份同一 Bot 或应用；查看日志确认原进程归属后处理。
