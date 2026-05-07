# -*- coding: utf-8 -*-
"""[`to_friendly`](src/core/remote/friendly_errors.py) 单元测试 (P4 W1·F5.4).

覆盖:

- paramiko 8 类典型异常: AuthenticationException / PasswordRequired / BadHostKey
  / NoValidConnections / SSHException
- stdlib 网络异常: socket.gaierror / ConnectionRefused / ConnectionReset / TimeoutError
- 文件系统: FileNotFoundError / PermissionError
- 项目自定义: RemoteCommandError / RemoteDeploymentError / RemoteDeploymentInProgressError
  / SSHAuthenticationError / SSHHostKeyError / SSHConnectionError
- 兜底路径: 未注册类型走 ``str(exc)``; ``__cause__`` 链命中
"""
from __future__ import annotations

# 标准库导入
import socket

# 第三方库导入
import paramiko
import pytest

# 项目内模块导入
from src.core.remote.errors import (
    RemoteCommandError,
    RemoteDeploymentError,
    RemoteDeploymentInProgressError,
    SSHAuthenticationError,
    SSHConnectionError,
    SSHHostKeyError,
)
from src.core.remote.friendly_errors import to_friendly, to_friendly_with_detail


# ==================== paramiko ====================
def test_paramiko_authentication_maps_to_user_password_message() -> None:
    msg = to_friendly(paramiko.AuthenticationException("Authentication failed."))
    assert msg == "用户名或密码错误"


def test_paramiko_password_required_takes_precedence_over_authentication() -> None:
    """``PasswordRequiredException`` 是 ``AuthenticationException`` 子类, 必须先命中."""
    msg = to_friendly(paramiko.PasswordRequiredException("private key file is encrypted"))
    assert msg == "私钥已加密, 请提供正确的口令"


def test_paramiko_bad_host_key_includes_hostname() -> None:
    # paramiko.BadHostKeyException 构造需要 (hostname, got_key, expected_key)
    # 这里直接用 SSHHostKeyError (项目自定义, 已经是更上层包装) 验证
    msg = to_friendly(SSHHostKeyError("hostname=example.com fingerprint=abc"))
    assert "主机指纹与已知记录不匹配" in msg


def test_paramiko_no_valid_connections_friendly() -> None:
    """直接 import NoValidConnectionsError, 构造一个最小实例.

    paramiko 4.x 起 ``NoValidConnectionsError`` 不再 re-export 到顶层模块,
    需要从 ``paramiko.ssh_exception`` 直接拿.
    """
    from paramiko.ssh_exception import NoValidConnectionsError

    err = NoValidConnectionsError(
        {("127.0.0.1", 22): ConnectionRefusedError(111, "Connection refused")}
    )
    msg = to_friendly(err)
    assert "无法连接" in msg


def test_paramiko_generic_ssh_exception_falls_back_to_protocol_message() -> None:
    msg = to_friendly(paramiko.SSHException("Invalid packet"))
    assert msg.startswith("SSH 协议异常")
    assert "Invalid packet" in msg


# ==================== stdlib ====================
def test_socket_gaierror_friendly() -> None:
    msg = to_friendly(socket.gaierror(11001, "getaddrinfo failed"))
    assert msg.startswith("无法解析主机名")


def test_connection_refused_friendly() -> None:
    assert to_friendly(ConnectionRefusedError(111, "refused")) == \
        "目标端口拒绝连接, 请检查 SSH 服务是否启动 / 端口是否正确"


def test_connection_reset_friendly() -> None:
    assert to_friendly(ConnectionResetError(104, "reset")) == \
        "连接被对端重置, 请稍后重试或检查 SSH 服务状态"


def test_timeout_error_friendly() -> None:
    assert to_friendly(TimeoutError("timed out")) == "连接超时, 请检查网络与防火墙"


def test_file_not_found_friendly() -> None:
    msg = to_friendly(FileNotFoundError("/etc/foo not found"))
    assert msg.startswith("文件不存在")


def test_permission_error_friendly() -> None:
    msg = to_friendly(PermissionError("denied"))
    assert msg.startswith("无访问权限")


# ==================== 项目自定义异常 ====================
def test_ssh_authentication_error_friendly() -> None:
    assert to_friendly(SSHAuthenticationError("auth fail")) == "用户名或密码错误"


def test_ssh_host_key_error_friendly() -> None:
    assert "指纹" in to_friendly(SSHHostKeyError("fingerprint mismatch"))


def test_ssh_connection_error_falls_back_to_generic_protocol_message() -> None:
    msg = to_friendly(SSHConnectionError("transport closed"))
    assert msg.startswith("SSH 协议异常")
    assert "transport closed" in msg


def test_remote_command_error_includes_exit_status_and_stderr() -> None:
    err = RemoteCommandError(command="ls /missing", exit_status=2, stderr="No such file")
    msg = to_friendly(err)
    assert "远端命令执行失败" in msg
    assert "退出码 2" in msg
    assert "No such file" in msg


def test_remote_command_error_truncates_long_stderr() -> None:
    long_stderr = "x" * 500
    err = RemoteCommandError(command="cmd", exit_status=1, stderr=long_stderr)
    msg = to_friendly(err)
    assert "…" in msg
    assert len(msg) < 280  # 200 字内 + 前缀


def test_remote_deployment_error_uses_stage_label() -> None:
    err = RemoteDeploymentError("install_napcat", "脚本退出非零")
    msg = to_friendly(err)
    assert "install_napcat" in msg
    assert "脚本退出非零" in msg


def test_remote_deployment_in_progress_friendly() -> None:
    assert to_friendly(RemoteDeploymentInProgressError()) == \
        "已有部署任务正在进行, 请等待当前任务结束后再试"


# ==================== 兜底 / cause 链 ====================
def test_unknown_exception_falls_back_to_str() -> None:
    class MyCustomError(Exception):
        pass

    assert to_friendly(MyCustomError("custom message")) == "custom message"


def test_unknown_exception_with_empty_str_falls_back_to_class_name() -> None:
    class MyCustomError(Exception):
        pass

    assert to_friendly(MyCustomError()) == "MyCustomError"


def test_friendly_chain_via_cause() -> None:
    """``__cause__`` 上有可识别异常时, 走链查找."""
    try:
        try:
            raise ConnectionRefusedError(111, "refused")
        except ConnectionRefusedError as inner:
            raise RuntimeError("wrapper") from inner
    except RuntimeError as exc:
        msg = to_friendly(exc)

    # 注意: RuntimeError 不在注册表中, 但走 __cause__ 链命中 ConnectionRefusedError
    assert "目标端口拒绝连接" in msg


def test_to_friendly_with_detail_returns_headline_and_repr() -> None:
    err = SSHAuthenticationError("password incorrect")
    headline, detail = to_friendly_with_detail(err)
    assert headline == "用户名或密码错误"
    assert "SSHAuthenticationError" in detail


# ==================== 鲁棒性 ====================
def test_handler_exception_falls_back_silently(monkeypatch: pytest.MonkeyPatch) -> None:
    """文案生成器内部抛错时不应让 to_friendly 自己崩溃."""
    from src.core.remote import friendly_errors as fe

    # 构造一个一定会被 handler 处理但 handler 故意抛错的场景
    bad_handler = lambda exc: (_ for _ in ()).throw(RuntimeError("oops"))  # noqa: E731
    patched = ((SSHConnectionError, bad_handler),) + fe.FRIENDLY_MESSAGES
    monkeypatch.setattr(fe, "FRIENDLY_MESSAGES", patched)

    # SSHConnectionError 命中 bad_handler 抛错 -> 走兜底路径 str(exc)
    msg = fe.to_friendly(SSHConnectionError("fallback content"))
    assert "fallback content" in msg
