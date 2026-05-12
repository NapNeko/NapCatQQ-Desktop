# -*- coding: utf-8 -*-
"""SnowLuma 远端目录布局.

与 :class:`src.core.remote.models.LinuxCorePaths` (NapCat 远端路径) 平行而独立,
不复用其字段, 因为 SnowLuma 的拓扑与 NapCat 完全不同:

- NapCat: ``$HOME/Napcat/{opt/QQ, run, log, packages}`` — QQ 安装与 NapCat 注入物
  共住一个 prefix
- SnowLuma: ``$HOME/snowluma-remote/workspace/{snowluma, runtime, log}`` — daemon
  (Xvfb + fluxbox + x11vnc + websockify + node) 与 per-Bot QQ.exe 共享一个 workspace,
  daemon 的 5 进程辅助 pid 各自独立写文件 + 总 pid 汇成 ``pid_daemon``

字段语义详见 ``docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md`` §W1.

安全: 所有路径字段经过与 NC 相同的 :data:`_LINUX_PATH_PATTERN` 校验, 拒绝 shell 元字符
注入 (复用 :func:`src.core.remote.models._validate_linux_path`).
"""

from __future__ import annotations

from dataclasses import dataclass, field

from src.core.remote.models import _validate_linux_path


_DEFAULT_BASE_DIR: str = "$HOME/snowluma-remote"


def _derive_field(base: str, *parts: str) -> str:
    """拼接 POSIX 风格路径; 不引入 :class:`pathlib.PurePosixPath` 是为了让 ``$HOME``
    前缀字面量保留 (``PurePosixPath`` 会把 ``$HOME`` 识别成普通段, 失去 shell 展开).
    """
    return base if not parts else base.rstrip("/") + "/" + "/".join(p.strip("/") for p in parts)


@dataclass(slots=True)
class SnowLumaRemotePaths:
    """SnowLuma 远端目录布局.

    所有字段在构造时校验合法性 (拒绝 shell 元字符), 校验失败抛 :class:`ValueError`.

    构造方式有两种:

    1. 显式传所有字段 (UI 高级模式 / 测试覆盖):

       .. code-block:: python

          paths = SnowLumaRemotePaths(
              base_dir="$HOME/snowluma-remote",
              workspace_dir="$HOME/snowluma-remote/workspace",
              ...
          )

    2. 推荐: 使用 :meth:`from_base` 一次性派生所有路径 (UI 简单模式 + ServerProfile
       默认值都走这条路径):

       .. code-block:: python

          paths = SnowLumaRemotePaths.from_base("$HOME/snowluma-remote")

    Attributes:
        base_dir: SnowLuma 远端根目录, 例如 ``$HOME/snowluma-remote``
        workspace_dir: 工作目录 (装的 SnowLuma + runtime + log 全部在此); 默认
            ``{base_dir}/workspace``
        snowluma_framework_dir: lite tarball 解压目标; 默认 ``{workspace_dir}/snowluma``
        config_dir: SnowLuma 三件套配置 (runtime/webui/onebot_<uin>) 落点; 默认
            ``{snowluma_framework_dir}/config``
        runtime_dir: pid 文件 + status_*.json + dbus.env 等运行时落点; 默认
            ``{workspace_dir}/runtime``
        log_dir: daemon 与各 Bot 日志根目录; 默认 ``{workspace_dir}/log``
        vnc_secret: VNC 密码文件; 默认 ``{workspace_dir}/vnc.secret`` (mode 600)
        webui_secret: SnowLuma WebUI 密码文件; 默认 ``{workspace_dir}/webui.secret``
        daemon_launcher_script: daemon launcher 脚本远端落点; 默认
            ``{workspace_dir}/snowluma_daemon_launcher.sh``
        bot_launcher_script: bot launcher 脚本远端落点; 默认
            ``{workspace_dir}/snowluma_bot_launcher.sh``
    """

    base_dir: str = _DEFAULT_BASE_DIR
    workspace_dir: str = field(default="")
    snowluma_framework_dir: str = field(default="")
    config_dir: str = field(default="")
    runtime_dir: str = field(default="")
    log_dir: str = field(default="")
    vnc_secret: str = field(default="")
    webui_secret: str = field(default="")
    daemon_launcher_script: str = field(default="")
    bot_launcher_script: str = field(default="")

    def __post_init__(self) -> None:
        # 允许构造时省略派生字段, 自动从 base_dir 推断.
        if not self.workspace_dir:
            self.workspace_dir = _derive_field(self.base_dir, "workspace")
        if not self.snowluma_framework_dir:
            self.snowluma_framework_dir = _derive_field(self.workspace_dir, "snowluma")
        if not self.config_dir:
            self.config_dir = _derive_field(self.snowluma_framework_dir, "config")
        if not self.runtime_dir:
            self.runtime_dir = _derive_field(self.workspace_dir, "runtime")
        if not self.log_dir:
            self.log_dir = _derive_field(self.workspace_dir, "log")
        if not self.vnc_secret:
            self.vnc_secret = _derive_field(self.workspace_dir, "vnc.secret")
        if not self.webui_secret:
            self.webui_secret = _derive_field(self.workspace_dir, "webui.secret")
        if not self.daemon_launcher_script:
            self.daemon_launcher_script = _derive_field(
                self.workspace_dir, "snowluma_daemon_launcher.sh"
            )
        if not self.bot_launcher_script:
            self.bot_launcher_script = _derive_field(
                self.workspace_dir, "snowluma_bot_launcher.sh"
            )

        # 路径合法性校验 (拒绝 ``$()`` / 反引号 / ``;`` 等元字符)
        for field_name in (
            "base_dir",
            "workspace_dir",
            "snowluma_framework_dir",
            "config_dir",
            "runtime_dir",
            "log_dir",
            "vnc_secret",
            "webui_secret",
            "daemon_launcher_script",
            "bot_launcher_script",
        ):
            _validate_linux_path(
                field_name,
                getattr(self, field_name),
                cls_name="SnowLumaRemotePaths",
            )

    # ==================== 派生路径 (properties) ====================
    @property
    def pid_daemon(self) -> str:
        """daemon 总 pid 文件 (launcher 启动时写; stop 时按此判存活)."""
        return _derive_field(self.runtime_dir, "pid_daemon")

    @property
    def status_daemon(self) -> str:
        """daemon 汇总状态 JSON (含 5 个辅助进程 pid + ready 标志)."""
        return _derive_field(self.runtime_dir, "status_daemon.json")

    @property
    def log_daemon(self) -> str:
        """daemon 主日志 (Node + 5 辅助进程合写)."""
        return _derive_field(self.log_dir, "daemon.log")

    @property
    def dbus_env_file(self) -> str:
        """``dbus-launch --sh-syntax`` 输出的环境变量文件 (`source` 用)."""
        return _derive_field(self.runtime_dir, "dbus.env")

    def pid_bot(self, qq_id: str) -> str:
        """指定 QQ 号的 Bot pid 文件 (qq.exe 进程 pid)."""
        if not qq_id or not qq_id.isdigit():
            raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")
        return _derive_field(self.runtime_dir, f"pid_bot_{qq_id}")

    def status_bot(self, qq_id: str) -> str:
        """指定 QQ 号的 Bot 状态 JSON (含 pid + uin + started_at)."""
        if not qq_id or not qq_id.isdigit():
            raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")
        return _derive_field(self.runtime_dir, f"status_bot_{qq_id}.json")

    def log_bot(self, qq_id: str) -> str:
        """指定 QQ 号的 Bot 日志 (qq.exe stdout/err 落点)."""
        if not qq_id or not qq_id.isdigit():
            raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")
        return _derive_field(self.log_dir, f"bot_{qq_id}.log")

    def runtime_json(self) -> str:
        """``runtime.json`` 远端路径 (daemon 全局, 字段 ``webuiPort``)."""
        return _derive_field(self.config_dir, "runtime.json")

    def webui_json(self) -> str:
        """``webui.json`` 远端路径 (daemon 全局, 字段 ``password``)."""
        return _derive_field(self.config_dir, "webui.json")

    def onebot_json(self, uin: str) -> str:
        """``onebot_<uin>.json`` 远端路径 (per-Bot OneBot 配置)."""
        if not uin or not uin.isdigit():
            raise ValueError(f"uin 必须是非空数字字符串: {uin!r}")
        return _derive_field(self.config_dir, f"onebot_{uin}.json")

    # ==================== 构造工厂 ====================
    @classmethod
    def from_base(cls, base_dir: str = _DEFAULT_BASE_DIR) -> "SnowLumaRemotePaths":
        """便捷工厂: 仅传根目录, 其余字段全自动派生.

        Args:
            base_dir: SnowLuma 远端根目录; 必须以 ``$HOME`` 或绝对路径起头, 且不含
                shell 元字符. 默认 ``$HOME/snowluma-remote``.

        Returns:
            构造好的 :class:`SnowLumaRemotePaths`.

        Raises:
            ValueError: ``base_dir`` 非法或派生路径校验失败.
        """
        return cls(base_dir=base_dir)
