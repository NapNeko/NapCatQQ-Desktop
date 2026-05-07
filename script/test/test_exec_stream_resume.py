# -*- coding: utf-8 -*-
"""[`SSHClient.exec_stream_resilient`](src/core/remote/ssh_client.py) 单测 (P4 W4·F7).

设计要点
========

- 不连接真实 SSH; ``SSHClient`` 用 ``__new__`` 跳过 ``__init__``, 仅替换 ``exec_stream``
  / ``ensure_alive`` 这两个被 helper 调到的方法, 验证:
    1. ``max_retries=0`` 退化为单次, 失败时直接抛
    2. 多次失败 + 最终成功时, ``open_command`` 收到累计 ``last_resume``
    3. ``progress_marker`` 解析失败 / ``on_stdout_line`` 抛异常都不应中断流
    4. ``RemoteCommandError`` 不会触发重试 (业务错误一律上抛)
    5. ``sleeper`` 注入路径; 真实代码使用 ``time.sleep``, 测试用 list 记录调用
"""
from __future__ import annotations

# 标准库导入
from typing import Any

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.remote.errors import RemoteCommandError, SSHConnectionError
from src.core.remote.models import RemoteCommandResult
from src.core.remote.ssh_client import SSHClient


# ==================== 测试夹具 ====================
def _make_client() -> SSHClient:
    """构造一个最小化的 SSHClient, 跳过 __init__ 副作用."""
    client = SSHClient.__new__(SSHClient)
    return client


def _ok_result(stdout: str = "ok") -> RemoteCommandResult:
    return RemoteCommandResult(
        command="<test>",
        exit_status=0,
        stdout=stdout,
        stderr="",
    )


# ==================== max_retries=0: 单次行为 ====================
def test_default_max_retries_zero_propagates_first_failure() -> None:
    client = _make_client()
    calls: list[str] = []

    def _fake_exec_stream(cmd: str, **_kwargs: Any) -> RemoteCommandResult:
        calls.append(cmd)
        raise SSHConnectionError("conn lost")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    with pytest.raises(SSHConnectionError):
        client.exec_stream_resilient(
            open_command=lambda offset: f"cmd --resume {offset}",
            sleeper=lambda _s: None,
        )
    # 仅尝试一次
    assert calls == ["cmd --resume 0"]


def test_max_retries_zero_returns_success_directly() -> None:
    client = _make_client()
    seen_cmds: list[str] = []

    def _fake_exec_stream(cmd: str, **_kwargs: Any) -> RemoteCommandResult:
        seen_cmds.append(cmd)
        return _ok_result("first-ok")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    result = client.exec_stream_resilient(open_command=lambda _o: "echo first")
    assert result.stdout == "first-ok"
    assert seen_cmds == ["echo first"]


# ==================== 多次重连 + last_resume 透传 ====================
def test_resume_offset_passed_to_open_command_after_retries() -> None:
    client = _make_client()
    cmds_seen: list[str] = []
    sleeps: list[float] = []
    reconnects: list[bool] = []

    state = {"attempt": 0}

    def _fake_exec_stream(cmd: str, *, on_stdout_line=None, **_kwargs: Any) -> RemoteCommandResult:
        cmds_seen.append(cmd)
        # 模拟前 2 次断开 (但断开前喂了几行进度, 让 progress_marker 推进 last_resume),
        # 第 3 次成功完成.
        if on_stdout_line is not None:
            on_stdout_line("[PROGRESS] 10 stage_a")
            on_stdout_line("[PROGRESS] 25 stage_b")
        if state["attempt"] < 2:
            state["attempt"] += 1
            raise SSHConnectionError(f"flap #{state['attempt']}")
        return _ok_result("done")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: reconnects.append(True)  # type: ignore[method-assign]

    def _progress_marker(line: str) -> int | None:
        if line.startswith("[PROGRESS] "):
            return int(line.split()[1])
        return None

    result = client.exec_stream_resilient(
        open_command=lambda offset: f"bash deploy.sh --resume {offset}",
        progress_marker=_progress_marker,
        max_retries=3,
        delays=(0.0, 0.0, 0.0),
        sleeper=sleeps.append,
    )

    assert result.stdout == "done"
    # 第 1 次 offset=0; 第 2/3 次 offset 必须 >= 25 (progress_marker 推进过)
    assert cmds_seen[0] == "bash deploy.sh --resume 0"
    assert cmds_seen[1] == "bash deploy.sh --resume 25"
    assert cmds_seen[2] == "bash deploy.sh --resume 25"
    # 重连了 2 次
    assert len(reconnects) == 2
    # sleeper 被调用 2 次 (前 2 次重试前 sleep)
    assert len(sleeps) == 2


# ==================== max_retries 用尽抛最后一次异常 ====================
def test_max_retries_exhausted_raises_last_exception() -> None:
    client = _make_client()
    attempts: list[int] = []

    def _fake_exec_stream(cmd: str, **_kwargs: Any) -> RemoteCommandResult:
        attempts.append(1)
        raise SSHConnectionError(f"persistent fail #{len(attempts)}")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    with pytest.raises(SSHConnectionError, match="persistent fail #3"):
        client.exec_stream_resilient(
            open_command=lambda _o: "cmd",
            max_retries=2,
            delays=(0.0,),
            sleeper=lambda _s: None,
        )
    assert len(attempts) == 3  # max_retries=2 -> 总 attempt = 3


# ==================== 业务错误不触发重试 ====================
def test_remote_command_error_is_not_retried() -> None:
    client = _make_client()
    calls: list[str] = []

    def _fake_exec_stream(cmd: str, **_kwargs: Any) -> RemoteCommandResult:
        calls.append(cmd)
        raise RemoteCommandError(command=cmd, exit_status=2, stderr="bad arg")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    with pytest.raises(RemoteCommandError):
        client.exec_stream_resilient(
            open_command=lambda _o: "cmd",
            max_retries=5,
            delays=(0.0,),
            sleeper=lambda _s: None,
        )
    # 业务错误直接抛, 不重试
    assert len(calls) == 1


# ==================== progress_marker / on_stdout_line 异常不阻断 ====================
def test_progress_marker_exception_is_swallowed() -> None:
    client = _make_client()

    def _fake_exec_stream(cmd: str, *, on_stdout_line=None, **_kw: Any) -> RemoteCommandResult:
        if on_stdout_line is not None:
            on_stdout_line("trigger-bad-parse")
            on_stdout_line("[PROGRESS] 50 stage_x")
        return _ok_result("done")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    def _bad_marker(line: str) -> int | None:
        if "bad" in line:
            raise ValueError("kaboom")
        if line.startswith("[PROGRESS] "):
            return int(line.split()[1])
        return None

    received: list[str] = []
    result = client.exec_stream_resilient(
        open_command=lambda _o: "cmd",
        progress_marker=_bad_marker,
        on_stdout_line=received.append,
        sleeper=lambda _s: None,
    )

    assert result.stdout == "done"
    assert received == ["trigger-bad-parse", "[PROGRESS] 50 stage_x"]


def test_on_stdout_line_exception_does_not_break_stream() -> None:
    client = _make_client()

    def _fake_exec_stream(cmd: str, *, on_stdout_line=None, **_kw: Any) -> RemoteCommandResult:
        if on_stdout_line is not None:
            on_stdout_line("first")
            on_stdout_line("second")
        return _ok_result("done")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = lambda **_kw: None  # type: ignore[method-assign]

    def _bad_listener(line: str) -> None:
        if line == "first":
            raise RuntimeError("listener boom")

    result = client.exec_stream_resilient(
        open_command=lambda _o: "cmd",
        on_stdout_line=_bad_listener,
        sleeper=lambda _s: None,
    )
    assert result.stdout == "done"


# ==================== reconnect 失败仍能继续下一轮 attempt ====================
def test_reconnect_failure_does_not_block_next_attempt() -> None:
    client = _make_client()
    calls = {"attempt": 0, "reconnect": 0}

    def _fake_exec_stream(cmd: str, **_kw: Any) -> RemoteCommandResult:
        calls["attempt"] += 1
        if calls["attempt"] < 2:
            raise SSHConnectionError("flap")
        return _ok_result("recovered")

    def _fake_ensure_alive(**_kw: Any) -> None:
        calls["reconnect"] += 1
        raise SSHConnectionError("first reconnect failed")

    client.exec_stream = _fake_exec_stream  # type: ignore[method-assign]
    client.ensure_alive = _fake_ensure_alive  # type: ignore[method-assign]

    result = client.exec_stream_resilient(
        open_command=lambda _o: "cmd",
        max_retries=2,
        delays=(0.0,),
        sleeper=lambda _s: None,
    )
    assert result.stdout == "recovered"
    # 第二次 attempt 即成功
    assert calls["attempt"] == 2
    assert calls["reconnect"] == 1
