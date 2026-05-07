# -*- coding: utf-8 -*-
"""[`LinuxCoreDeployment.install_napcat`](src/core/remote/deployment.py)
expected_sha512 注入 + 退出码 36 处理 单测 (P5 安全收尾 F1.4).

不依赖真实 SSH; 用替身 ``ExecutionBackend`` 捕获命令字符串与退出码.
"""
from __future__ import annotations

# 标准库导入
from typing import Any

# 第三方库导入
import pytest


# ==================== 替身基础设施 ====================
class _FakeExecutionBackend:
    """实现 ``RemoteExecutionBackend`` 接口的最小替身.

    属性:
        commands: 按调用顺序记录所有 ``run`` / ``run_stream`` 命令字符串
        exit_status: 下一次 install_napcat 主命令的模拟退出码
        stderr: 模拟 stderr 内容 (用于退出码 36 的场景)
    """

    def __init__(self, *, install_exit_status: int = 0, install_stderr: str = "") -> None:
        self.commands: list[str] = []
        self._install_exit_status = install_exit_status
        self._install_stderr = install_stderr

    # 下面这些方法签名都尽量贴近 ExecutionBackend / SSHClient 的子集
    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> "_FakeResult":
        self.commands.append(command)
        # 主脚本调用: 可能指定 install_exit_status / install_stderr (deployment 走
        # _run_script_with_progress 后退化到 backend.run 全同步路径中).
        if "remote_install_napcat.sh" in command and command.lstrip().startswith(
            ("bash ", "NAPCAT_", "FORCE_NAPCAT_")
        ):
            return _FakeResult(
                command=command,
                exit_status=self._install_exit_status,
                stdout="[PROGRESS] 100 install_napcat done\n",
                stderr=self._install_stderr,
            )
        # 其他辅助命令 (mkdir / chmod / etc.) 一律返回成功
        return _FakeResult(command=command, exit_status=0, stdout="", stderr="")

    def run_stream(
        self,
        command: str,
        *,
        on_stdout_line=None,
        on_stderr_line=None,
        timeout: float | None = None,
        check: bool = False,
        merge_stderr: bool = False,
    ) -> "_FakeResult":
        del on_stderr_line, timeout, check, merge_stderr  # 未用
        self.commands.append(command)
        if "remote_install_napcat.sh" in command:
            # 主脚本调用走配置的 install_exit_status / install_stderr
            return _FakeResult(
                command=command,
                exit_status=self._install_exit_status,
                stdout="[PROGRESS] 100 install_napcat done\n",
                stderr=self._install_stderr,
            )
        return _FakeResult(command=command, exit_status=0, stdout="", stderr="")

    def upload_file(self, local_path, remote_path: str) -> None:
        del local_path, remote_path

    def ensure_directory(self, remote_path: str) -> None:
        del remote_path


class _FakeResult:
    """``RemoteCommandResult`` 的兼容替身."""

    def __init__(self, command: str, exit_status: int, stdout: str = "", stderr: str = "") -> None:
        self.command = command
        self.exit_status = exit_status
        self.stdout = stdout
        self.stderr = stderr

    @property
    def ok(self) -> bool:
        return self.exit_status == 0


@pytest.fixture
def deployment_factory(monkeypatch: pytest.MonkeyPatch):
    """工厂函数: 注入 fake backend 并构造 LinuxCoreDeployment."""
    from src.core.remote.deployment import LinuxCoreDeployment
    from src.core.remote.models import LinuxCorePaths

    def _make(
        *,
        install_exit_status: int = 0,
        install_stderr: str = "",
    ) -> tuple[LinuxCoreDeployment, _FakeExecutionBackend]:
        backend = _FakeExecutionBackend(
            install_exit_status=install_exit_status,
            install_stderr=install_stderr,
        )
        # upload_launcher_script 会调用 backend.upload_file + run "chmod"; 不影响测试
        deployment = LinuxCoreDeployment(backend=backend, paths=LinuxCorePaths())
        # ``upload_launcher_script`` 用 SSHClient 的 SFTP 接口; fake backend 没有,
        # 用 monkeypatch 替换为 no-op
        monkeypatch.setattr(deployment, "upload_launcher_script", lambda *a, **kw: "")
        return deployment, backend

    return _make


# ==================== 辅助 ====================
def _select_install_commands(commands: list[str]) -> list[str]:
    """过滤出 ``bash ... remote_install_napcat.sh`` 主脚本调用 (排除 chmod / mkdir)."""
    return [
        c
        for c in commands
        if "remote_install_napcat.sh" in c
        and c.lstrip().startswith(("bash ", "NAPCAT_", "FORCE_NAPCAT_"))
    ]


# ==================== 测试 ====================
def test_install_napcat_injects_expected_sha512_env_var(deployment_factory) -> None:
    """``expected_sha512`` 提供时, 命令字符串必须包含 ``NAPCAT_EXPECTED_SHA512=...`` 环境变量."""
    deployment, backend = deployment_factory()
    expected = "ab" * 64  # 128 hex chars

    deployment.install_napcat(expected_sha512=expected)

    main_commands = _select_install_commands(backend.commands)
    assert len(main_commands) == 1
    cmd = main_commands[0]
    assert "NAPCAT_EXPECTED_SHA512=" in cmd
    # _shell_quote 会把 hex 用单引号包起来; 大小写归一为小写
    assert expected in cmd or expected.lower() in cmd


def test_install_napcat_omits_env_when_no_sha512(deployment_factory) -> None:
    """``expected_sha512=None`` 时, 命令字符串不应出现 ``NAPCAT_EXPECTED_SHA512`` 字眼."""
    deployment, backend = deployment_factory()
    deployment.install_napcat(expected_sha512=None)

    main_commands = _select_install_commands(backend.commands)
    assert len(main_commands) == 1
    assert "NAPCAT_EXPECTED_SHA512" not in main_commands[0]


def test_install_napcat_normalizes_uppercase_sha512(deployment_factory) -> None:
    """大写 hex 输入应在传入前归一为小写, 避免远端比较失败."""
    deployment, backend = deployment_factory()
    upper_hash = "AB" * 64

    deployment.install_napcat(expected_sha512=upper_hash)

    main_commands = _select_install_commands(backend.commands)
    assert len(main_commands) == 1
    assert upper_hash.lower() in main_commands[0]
    # 大写形式不应出现在最终命令里
    assert upper_hash not in main_commands[0]


def test_install_napcat_exit_status_36_translates_to_command_error(deployment_factory) -> None:
    """脚本退出码 36 (SHA512 mismatch) 应抛 ``RemoteCommandError`` (上层会转为 deployment_verify stage).

    ``LinuxCoreDeployment`` 自身不区分 stage; 36 -> RemoteCommandError, server_manager
    层根据 exit_status / stderr 关键字升级为 ``RemoteDeploymentError(stage="install_napcat_verify")``.
    """
    from src.core.remote.errors import RemoteCommandError

    deployment, _ = deployment_factory(
        install_exit_status=36,
        install_stderr="[ERROR] sha512 mismatch: expected=aa actual=bb",
    )

    with pytest.raises(RemoteCommandError) as exc_info:
        deployment.install_napcat(expected_sha512="aa" * 64)

    assert exc_info.value.exit_status == 36
    assert "sha512 mismatch" in exc_info.value.stderr.lower()


def test_install_napcat_exit_status_36_constant_is_documented(deployment_factory) -> None:
    """``INSTALL_NAPCAT_VERIFY_EXIT_CODE`` 常量与脚本约定一致."""
    deployment, _ = deployment_factory()
    assert deployment.INSTALL_NAPCAT_VERIFY_EXIT_CODE == 36
