# -*- coding: utf-8 -*-
"""远程管理数据模型. """

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Literal


SSHAuthMethod = Literal["password", "key"]

# P5 F2.3: ``LinuxCorePaths`` 字段白名单正则.
#
# 允许:
# - 可选 ``$HOME`` 前缀 (后必须跟 ``/`` 或字符串结束)
# - 字母数字 / 下划线 / ``.`` / ``-`` / ``/``
#
# 显式拒绝: ``$()`` / 反引号 / ``;`` / ``&`` / ``|`` / ``>`` / ``<`` / 换行 / ``"`` /
# ``'`` / ``\`` / ``$<其他变量>``. 防 ``servers.json`` 被改后绕过 UI 校验,
# 让恶意 workspace_dir 流到 inject_script_variables / _quote_remote_argument.
_LINUX_PATH_PATTERN: re.Pattern[str] = re.compile(
    r"^(?:\$HOME(?:/[A-Za-z0-9._\-/]+)?|/[A-Za-z0-9._\-/]+)$"
)


def _validate_linux_path(field_name: str, value: str) -> None:
    """校验单个路径字段; 不合法时抛 ``ValueError``.

    供 [`LinuxCorePaths.__post_init__`] 与 UI 同源校验复用 (后者通过
    [`is_valid_linux_path`] 包装为布尔判断).
    """
    if not isinstance(value, str) or not value:
        raise ValueError(f"LinuxCorePaths.{field_name} 不能为空")
    if not _LINUX_PATH_PATTERN.match(value):
        raise ValueError(
            f"LinuxCorePaths.{field_name} 含非法字符或格式 (允许 $HOME 前缀 + 字母数字 _./-/): {value!r}"
        )


def is_valid_linux_path(value: str) -> bool:
    """UI 同源校验入口; 不抛异常, 仅返回布尔."""
    if not isinstance(value, str) or not value:
        return False
    return _LINUX_PATH_PATTERN.match(value) is not None
# P4 F5.1: 新增 ``"interactive"`` 政策, 让首次连接通过
# [`HostKeyConfirmDialog`](src/ui/components/host_key_confirm_dialog.py)
# 弹窗确认; 旧值保留, 已存盘配置 (``reject`` / ``auto_add`` / ``warning``) 完全兼容.
HostKeyPolicy = Literal["reject", "auto_add", "warning", "interactive"]


@dataclass(slots=True)
class SSHCredentials:
    """SSH 连接凭据. 

    安全默认值: 
    - 默认拒绝未知主机指纹
    - 默认关闭 SSH agent 与本地自动找钥匙
    - 不要求必须持久化密码
    """

    host: str
    username: str
    port: int = 22
    auth_method: SSHAuthMethod = "key"
    password: str | None = None
    private_key_path: str | None = None
    private_key_passphrase: str | None = None
    connect_timeout: float = 10.0
    command_timeout: float = 20.0
    # 部署脚本类长耗时命令的超时(秒); apt-get / curl 大文件下载等可能远超 command_timeout
    # 默认 30 分钟, 覆盖最坏情况(慢网络下完整安装)
    script_timeout: float = 1800.0
    host_key_policy: HostKeyPolicy = "reject"
    allow_agent: bool = False
    look_for_keys: bool = False

    def validate(self) -> None:
        """校验凭据配置是否合法. """
        if not self.host.strip():
            raise ValueError("SSH 主机地址不能为空")
        if not self.username.strip():
            raise ValueError("SSH 用户名不能为空")
        if self.port <= 0:
            raise ValueError("SSH 端口必须大于 0")
        if self.connect_timeout <= 0:
            raise ValueError("SSH 连接超时时间必须大于 0")
        if self.command_timeout <= 0:
            raise ValueError("SSH 命令超时时间必须大于 0")
        if self.script_timeout <= 0:
            raise ValueError("SSH 脚本超时时间必须大于 0")

        if self.auth_method == "password":
            if not self.password:
                raise ValueError("密码认证模式下必须提供密码")
            return

        if not self.private_key_path:
            raise ValueError("密钥认证模式下必须提供私钥路径")
        if not self.private_key_file or not self.private_key_file.exists():
            raise ValueError("密钥认证模式下提供的私钥文件不存在")

    @property
    def private_key_file(self) -> Path | None:
        """返回私钥文件路径. """
        if not self.private_key_path:
            return None
        return Path(self.private_key_path)


@dataclass(slots=True)
class LinuxCorePaths:
    """Linux Core 的远端目录布局. 

    适配标准 NapCat 安装器路径:
    - 基础目录: $HOME/Napcat
    - QQ 安装: $HOME/Napcat/opt/QQ
    - NapCat: $HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat
    - 运行目录: $HOME/Napcat/run
    - 日志目录: $HOME/Napcat/log
    """

    workspace_dir: str = "$HOME/Napcat"
    runtime_dir: str = "$HOME/Napcat/run"
    config_dir: str = "$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/config"
    log_dir: str = "$HOME/Napcat/log"
    tmp_dir: str = "$HOME/Napcat/tmp"
    package_dir: str = "$HOME/Napcat/packages"

    def __post_init__(self) -> None:
        """P5 F2.3: 严格校验所有路径字段, 拒绝 shell 元字符注入."""
        for field_name in (
            "workspace_dir",
            "runtime_dir",
            "config_dir",
            "log_dir",
            "tmp_dir",
            "package_dir",
        ):
            _validate_linux_path(field_name, getattr(self, field_name))

    @property
    def install_base_dir(self) -> str:
        """Rootless LinuxQQ/NapCat 安装目录. """
        return self.workspace_dir

    @property
    def qq_base_path(self) -> str:
        """Rootless LinuxQQ 基础目录. """
        return f"{self.workspace_dir}/opt/QQ"

    @property
    def target_folder(self) -> str:
        """NapCat 注入目录. """
        return f"{self.qq_base_path}/resources/app/app_launcher"

    @property
    def napcat_dir(self) -> str:
        """NapCat 安装目录. """
        return f"{self.target_folder}/napcat"

    @property
    def qq_executable(self) -> str:
        """LinuxQQ 可执行文件路径. """
        return f"{self.qq_base_path}/qq"

    @property
    def launcher_script(self) -> str:
        """远端标准启动脚本路径. """
        return f"{self.workspace_dir}/napcat.sh"

    @property
    def qq_package_json_path(self) -> str:
        """QQ package.json 文件路径. """
        return f"{self.qq_base_path}/resources/app/package.json"

    @property
    def pid_file(self) -> str:
        """NapCat 远端 PID 文件路径. """
        return f"{self.runtime_dir}/napcat.pid"

    @property
    def status_file(self) -> str:
        """NapCat 远端状态文件路径. """
        return f"{self.runtime_dir}/status.json"

    @property
    def log_file(self) -> str:
        """NapCat 远端日志文件路径. """
        return f"{self.log_dir}/napcat.log"


@dataclass(slots=True)
class RemoteCommandResult:
    """远程命令执行结果. """

    command: str
    exit_status: int
    stdout: str = ""
    stderr: str = ""

    @property
    def ok(self) -> bool:
        """命令是否执行成功. """
        return self.exit_status == 0
