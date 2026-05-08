# -*- coding: utf-8 -*-
"""[`SSHClient._dispatch_stream_line`](src/core/remote/ssh_client.py) 与
 ``on_stdout_progress`` 路由的回归保护.

背景: ``dnf`` / ``apt`` / ``curl`` 等工具使用 ``\\r`` (carriage return) 在同一行原地
刷新进度条, 如果 UI 把每一帧都当成新行追加, 用户会看到同一个包的 ``Installing``
被堆成上千条. 部署控制台依赖 [`exec_stream`](src/core/remote/ssh_client.py) 的
``on_stdout_progress`` 回调把 ``\\r`` 终止的瞬时刷新行单独路由出来, 自己做
"原地覆盖上一行" 渲染.

本测试直接调用静态方法 [`_dispatch_stream_line`](src/core/remote/ssh_client.py),
锁死如下契约:

- ``\\r`` 终止 **且** 调用方提供 ``on_stdout_progress``: 只走 progress 回调,
  既不进入 ``captured_stdout`` 也不触发 ``on_stdout_line``; 避免 dnf 进度条
  在 ``RemoteCommandResult.stdout`` 与上层日志链路各刷一遍.
- ``\\r`` 终止但 **未** 提供 ``on_stdout_progress``: 回退到旧行为 (进 captured_stdout
  + on_stdout_line), 保证老调用方零回归.
- ``\\n`` / ``\\r\\n`` / ``""`` (flush 残留) 终止: 始终走 captured_stdout + on_stdout_line.
"""
from __future__ import annotations

from src.core.remote.ssh_client import SSHClient


def _dispatch(
    line: str,
    *,
    terminator: str,
    on_stdout_line=None,
    on_stdout_progress=None,
):
    """薄包装: 让每个用例共用同一个 ``captured_stdout`` 列表, 方便断言."""
    captured: list[str] = []
    SSHClient._dispatch_stream_line(  # noqa: SLF001 - 测试专门校验静态分派助手
        line,
        terminator=terminator,
        captured_stdout=captured,
        on_stdout_line=on_stdout_line,
        on_stdout_progress=on_stdout_progress,
    )
    return captured


class TestDispatchTransientLine:
    """``\\r`` 瞬时刷新行的分派行为."""

    def test_transient_line_only_goes_to_progress_when_callback_set(self) -> None:
        line_calls: list[str] = []
        progress_calls: list[str] = []

        captured = _dispatch(
            "Installing : pkg [===] 1/10",
            terminator="\r",
            on_stdout_line=line_calls.append,
            on_stdout_progress=progress_calls.append,
        )

        # progress 回调收到, line 回调和 captured_stdout 都没收到
        assert progress_calls == ["Installing : pkg [===] 1/10"]
        assert line_calls == []
        assert captured == []

    def test_transient_line_falls_back_to_line_when_no_progress_callback(self) -> None:
        """不提供 on_stdout_progress 时, 保留旧行为 (瞬时行也进 line / stdout)."""
        line_calls: list[str] = []

        captured = _dispatch(
            "Installing : pkg [===] 1/10",
            terminator="\r",
            on_stdout_line=line_calls.append,
            on_stdout_progress=None,
        )

        assert line_calls == ["Installing : pkg [===] 1/10"]
        assert captured == ["Installing : pkg [===] 1/10"]

    def test_transient_line_swallowed_when_no_callbacks(self) -> None:
        """两个回调都没设时, 瞬时行应进入 captured_stdout (兼容旧 exec_stream 语义)."""
        captured = _dispatch(
            "progress tick",
            terminator="\r",
            on_stdout_line=None,
            on_stdout_progress=None,
        )
        # 没有 progress 回调 -> 走"最终行"路径, 进 captured_stdout
        assert captured == ["progress tick"]


class TestDispatchFinalLine:
    """``\\n`` / ``\\r\\n`` / flush 残留的已提交最终行分派行为."""

    def test_lf_line_goes_to_line_callback_and_captured(self) -> None:
        line_calls: list[str] = []
        progress_calls: list[str] = []

        captured = _dispatch(
            "[INFO] installing",
            terminator="\n",
            on_stdout_line=line_calls.append,
            on_stdout_progress=progress_calls.append,
        )

        assert line_calls == ["[INFO] installing"]
        assert progress_calls == []
        assert captured == ["[INFO] installing"]

    def test_crlf_line_goes_to_line_callback(self) -> None:
        """PTY 模式下 ``\\r\\n`` 很常见, 必须按最终行处理."""
        line_calls: list[str] = []

        captured = _dispatch(
            "PTY line",
            terminator="\r\n",
            on_stdout_line=line_calls.append,
            on_stdout_progress=lambda _l: None,
        )

        assert line_calls == ["PTY line"]
        assert captured == ["PTY line"]

    def test_flush_residual_is_treated_as_final(self) -> None:
        """``flush`` 残留 (terminator="") 应按最终行处理, 而不是瞬时行."""
        line_calls: list[str] = []
        progress_calls: list[str] = []

        captured = _dispatch(
            "partial-final",
            terminator="",
            on_stdout_line=line_calls.append,
            on_stdout_progress=progress_calls.append,
        )

        assert line_calls == ["partial-final"]
        assert progress_calls == []
        assert captured == ["partial-final"]


class TestDispatchCallbackIsolation:
    """回调抛异常时不应中断整个流式命令."""

    def test_line_callback_exception_does_not_break_capture(self) -> None:
        def _raising(_line: str) -> None:
            raise RuntimeError("boom")

        captured = _dispatch(
            "ok",
            terminator="\n",
            on_stdout_line=_raising,
        )
        # 即便回调抛错, captured_stdout 仍然累积, 供 RemoteCommandResult.stdout 使用
        assert captured == ["ok"]

    def test_progress_callback_exception_does_not_leak(self) -> None:
        def _raising(_line: str) -> None:
            raise RuntimeError("boom")

        line_calls: list[str] = []

        captured = _dispatch(
            "tick",
            terminator="\r",
            on_stdout_line=line_calls.append,
            on_stdout_progress=_raising,
        )
        # progress 回调 raise 了, 但既不该降级到 line 回调, 也不该污染 captured
        assert line_calls == []
        assert captured == []
