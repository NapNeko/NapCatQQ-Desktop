# -*- coding: utf-8 -*-
"""SnowLuma 远端 launcher 命令构造器 (W5).

由 :class:`SnowLumaDeployment` 部署的两份脚本提供能力:

- ``snowluma_daemon_launcher.sh`` (子命令: start / stop / status / restart / wait-ready)
- ``snowluma_bot_launcher.sh`` (子命令: start <qq_id> [uin] / stop <qq_id> / status <qq_id>)

本模块**只**构造 Desktop 端要执行的 shell 命令字符串, **不**直接调 SSH; 调用者
(W9 ``SnowLumaRemoteDaemon`` / ``SnowLumaRemoteDriver``) 通过 :class:`ExecutionBackend`
执行命令并解析输出.

设计取舍 (与本地 :class:`src.core.runtime.snowluma_daemon.SnowLumaDaemon` 对照):

- 本地 daemon: 用 :class:`QProcess` 直接 spawn ``node.exe``, Desktop 进程内拥有 daemon
- 远端 daemon: Desktop **不**拥有 daemon, 通过 launcher 命令间接控制远端 daemon 进程组
- 因此本模块的 API 极简: 一组无副作用的字符串构造函数 + 严格 qq_id/uin 校验

注入安全: ``qq_id`` / ``uin`` 经过 :func:`isdigit` 严格校验, 与 launcher 脚本内部
``case '' | '*[!0-9]*'`` 校验同源; 拒绝命令注入 + 路径污染.
"""

from __future__ import annotations

from dataclasses import dataclass

from .paths import SnowLumaRemotePaths


@dataclass(slots=True, frozen=True)
class SnowLumaLauncherCommands:
    """生成远端 launcher 脚本调用命令.

    所有命令默认通过 ``bash`` 显式调起 (脚本在 W4 部署期已 ``chmod +x``,
    用 ``bash <script>`` 仍然有效; 对 ``noexec`` 挂载的 ``/tmp`` 场景也安全).

    Args:
        paths: SL 远端路径; ``daemon_launcher_script`` 与 ``bot_launcher_script``
            字段被本模块读取.

    Examples:
        >>> cmds = SnowLumaLauncherCommands(paths)
        >>> backend.run(cmds.daemon_start_cmd())
        >>> bot_status = backend.run(cmds.bot_status_cmd("114514"))
    """

    paths: SnowLumaRemotePaths

    # ==================== daemon ====================
    def daemon_start_cmd(self) -> str:
        """启动 daemon (Xvfb + fluxbox + x11vnc + websockify + node)."""
        return f'bash "{self.paths.daemon_launcher_script}" start'

    def daemon_stop_cmd(self) -> str:
        """逆序停 daemon 进程组; 删 pid_* 文件."""
        return f'bash "{self.paths.daemon_launcher_script}" stop'

    def daemon_restart_cmd(self) -> str:
        """``stop && sleep 1 && start``; daemon 内部会再次跑 wait-ready."""
        return f'bash "{self.paths.daemon_launcher_script}" restart'

    def daemon_status_cmd(self) -> str:
        """打印 ``status_daemon.json`` 内容到 stdout.

        与 :class:`SnowLumaRemoteRuntimeService.get_daemon_status` 内部 ``cat`` 不同,
        本命令通过 launcher 脚本统一入口, 失败时输出 minimal stopped json 模板.
        """
        return f'bash "{self.paths.daemon_launcher_script}" status'

    def daemon_wait_ready_cmd(self, timeout: int = 60) -> str:
        """阻塞轮询 daemon 进入 ready 状态.

        Args:
            timeout: 最长等待秒数; 默认 60s (与本地 daemon spawn 超时同量级).
                超时返非 0 退出码, 调用方可视为 STARTING 失败.

        Raises:
            ValueError: ``timeout <= 0``.
        """
        if not isinstance(timeout, int) or timeout <= 0:
            raise ValueError(f"timeout 必须是正整数: {timeout!r}")
        return f'bash "{self.paths.daemon_launcher_script}" wait-ready {timeout}'

    # ==================== bot ====================
    def bot_start_cmd(self, qq_id: str, uin: str | None = None) -> str:
        """启动单 Bot (qq.exe spawn).

        Args:
            qq_id: SnowLuma 会话 id (纯数字, 与本地 BotConfig.QQID 同语义)
            uin: 可选; 用户 QQ 号 (纯数字), 写入 status_bot json 供 BotCard 显示

        Raises:
            ValueError: ``qq_id`` 不是非空数字字符串, 或 ``uin`` 非空且非数字
        """
        _validate_qq_id(qq_id)
        if uin is not None:
            _validate_uin(uin)
            return f'bash "{self.paths.bot_launcher_script}" start {qq_id} {uin}'
        return f'bash "{self.paths.bot_launcher_script}" start {qq_id}'

    def bot_stop_cmd(self, qq_id: str) -> str:
        """优雅停止单 Bot (SIGTERM 后 10s 超时升级 SIGKILL)."""
        _validate_qq_id(qq_id)
        return f'bash "{self.paths.bot_launcher_script}" stop {qq_id}'

    def bot_status_cmd(self, qq_id: str) -> str:
        """打印 ``status_bot_<qq_id>.json`` 内容."""
        _validate_qq_id(qq_id)
        return f'bash "{self.paths.bot_launcher_script}" status {qq_id}'


# ==================== 校验 helper ====================
def _validate_qq_id(qq_id: str) -> None:
    """与 launcher 脚本里 ``case '' | '*[!0-9]*'`` 同源的数字校验.

    Raises:
        ValueError: 空串 / 含非数字字符
    """
    if not isinstance(qq_id, str) or not qq_id or not qq_id.isdigit():
        raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")


def _validate_uin(uin: str) -> None:
    if not isinstance(uin, str) or not uin or not uin.isdigit():
        raise ValueError(f"uin 必须是非空数字字符串: {uin!r}")


__all__ = [
    "SnowLumaLauncherCommands",
]
