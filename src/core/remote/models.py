# -*- coding: utf-8 -*-
"""远程管理数据模型。"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Literal


SSHAuthMethod = Literal["password", "key"]
HostKeyPolicy = Literal["reject", "auto_add", "warning"]


@dataclass(slots=True)
class SSHCredentials:
    """SSH 连接凭据。

    安全默认值：
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
    host_key_policy: HostKeyPolicy = "reject"
    allow_agent: bool = False
    look_for_keys: bool = False

    def validate(self) -> None:
        """校验凭据配置是否合法。"""
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
        """返回私钥文件路径。"""
        if not self.private_key_path:
            return None
        return Path(self.private_key_path)


@dataclass(slots=True)
class LinuxCorePaths:
    """Linux Core 的远端目录布局。"""

    workspace_dir: str = "$HOME/NapCatCore"
    runtime_dir: str = "$HOME/NapCatCore/runtime"
    config_dir: str = "$HOME/NapCatCore/runtime/config"
    log_dir: str = "$HOME/NapCatCore/runtime/log"
    tmp_dir: str = "$HOME/NapCatCore/runtime/tmp"
    package_dir: str = "$HOME/NapCatCore/packages"

    @property
    def pid_file(self) -> str:
        """NapCat 远端 PID 文件路径。"""
        return f"{self.runtime_dir}/napcat.pid"

    @property
    def status_file(self) -> str:
        """NapCat 远端状态文件路径。"""
        return f"{self.runtime_dir}/status.json"


@dataclass(slots=True)
class RemoteCommandResult:
    """远程命令执行结果。"""

    command: str
    exit_status: int
    stdout: str = ""
    stderr: str = ""

    @property
    def ok(self) -> bool:
        """命令是否执行成功。"""
        return self.exit_status == 0
