# -*- coding: utf-8 -*-
r"""[`RemoteRuntimeService.get_status_for_bot`](src/core/remote/status.py)
真实 cmdline 正则匹配回归测试.

线上现象 (P2): 远端 launcher 报告 ``[OK] qq=... started pid=...`` 后,
``get_status_for_bot`` 仍然返回 ``running=False``, 触发
``RemoteBackend.start_napcat`` 抛出 "launcher reported success but
status_for_bot says not running" 的错误.

根因: 旧实现使用 ``\b-q\s+{qq_id}\b`` 在 cmdline 上做二次校验, 但
``\b`` 仅在 word/non-word 边界触发, 而真实 cmdline 中 ``-q`` 前总是空白
(``-`` 与空格都是 non-word), 永远无法匹配, 导致所有候选 PID 被拒绝.

本测试覆盖 launcher 实际产生的三类候选进程 cmdline:
- xvfb-run wrapper: ``/bin/sh /usr/bin/xvfb-run -a /opt/QQ/qq --no-sandbox -q 2707600964``
- 真实 qq 进程: ``/opt/QQ/qq --no-sandbox -q 2707600964``
- 含尾随空白 / 换行的 ``ps`` 输出
并确保 qq_id 不会被前缀匹配为更长数字 (例如 ``2707600964`` 不应误匹配
``27076009644``).
"""

from __future__ import annotations

from dataclasses import dataclass, field

from src.core.remote.models import LinuxCorePaths, RemoteCommandResult
from src.core.remote.status import RemoteRuntimeService


@dataclass
class _ScriptedExecBackend:
    """按 (command_substring, RemoteCommandResult) 顺序匹配返回结果的执行后端替身.

    用 ``in`` 子串匹配, 避免对每个命令都精确写一次, 仍然足够区分
    ``pgrep`` / ``ps -o cmd= -p <pid>`` / ``test -f .../status_*.json`` /
    ``grep ... napcat.mjs``.
    """

    rules: list[tuple[str, RemoteCommandResult]] = field(default_factory=list)
    history: list[str] = field(default_factory=list)

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        del timeout, check
        self.history.append(command)
        for needle, result in self.rules:
            if needle in command:
                # 用 dataclass replace 模式重新封装 command, 保持 stdout/stderr/exit
                return RemoteCommandResult(
                    command=command,
                    exit_status=result.exit_status,
                    stdout=result.stdout,
                    stderr=result.stderr,
                )
        return RemoteCommandResult(command=command, exit_status=0)


def _make_runtime(rules: list[tuple[str, RemoteCommandResult]]) -> RemoteRuntimeService:
    backend = _ScriptedExecBackend(rules=rules)
    return RemoteRuntimeService(backend, LinuxCorePaths())


def _ok(stdout: str = "") -> RemoteCommandResult:
    return RemoteCommandResult(command="", exit_status=0, stdout=stdout)


class TestGetStatusForBotCmdlineRegex:
    """真实 ``ps -o cmd=`` 输出能否被识别为目标 qq 进程."""

    QQ_ID = "2707600964"

    def test_matches_real_qq_cmdline(self) -> None:
        """裸 ``qq --no-sandbox -q <id>`` 必须被识别为 running."""
        rules = [
            ("pgrep", _ok("13990\n")),
            ("ps -o cmd= -p 13990", _ok("/opt/QQ/qq --no-sandbox -q 2707600964\n")),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is True
        assert status.pid == 13990

    def test_matches_xvfb_run_wrapper_cmdline(self) -> None:
        """xvfb-run 包装器 cmdline (前面有空白) 也必须匹配."""
        cmdline = (
            "/bin/sh /usr/bin/xvfb-run -a /opt/QQ/qq --no-sandbox -q 2707600964\n"
        )
        rules = [
            ("pgrep", _ok("13824\n")),
            ("ps -o cmd= -p 13824", _ok(cmdline)),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is True
        assert status.pid == 13824

    def test_breaks_on_first_match_among_multiple_pids(self) -> None:
        """pgrep 返回多 PID 时, 第一个匹配项即停止后续 ps 调用."""
        rules = [
            ("pgrep", _ok("13824\n13838\n13990\n")),
            ("ps -o cmd= -p 13824", _ok(
                "/bin/sh /usr/bin/xvfb-run -a /opt/QQ/qq --no-sandbox -q 2707600964\n"
            )),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is True
        assert status.pid == 13824
        # 不应继续探活后续 PID
        history = runtime.backend.history  # type: ignore[attr-defined]
        assert not any("ps -o cmd= -p 13838" in cmd for cmd in history)
        assert not any("ps -o cmd= -p 13990" in cmd for cmd in history)

    def test_does_not_match_longer_qq_id_prefix(self) -> None:
        """``2707600964`` 不应误匹配 ``27076009644`` (更长的相似 qq)."""
        rules = [
            ("pgrep", _ok("13990\n")),
            ("ps -o cmd= -p 13990", _ok("/opt/QQ/qq --no-sandbox -q 27076009644\n")),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is False
        assert status.pid is None

    def test_no_pgrep_match_returns_not_running(self) -> None:
        """pgrep 无输出时直接判定 not running, 不再调用 ps."""
        rules = [
            ("pgrep", _ok("")),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is False
        assert status.pid is None
        history = runtime.backend.history  # type: ignore[attr-defined]
        assert not any("ps -o cmd= -p" in cmd for cmd in history)

    def test_empty_ps_output_does_not_crash(self) -> None:
        """``ps -o cmd= -p`` 在 PID 已退出时返回空, 应被跳过而非匹配."""
        rules = [
            ("pgrep", _ok("13824\n")),
            ("ps -o cmd= -p 13824", _ok("")),
        ]
        runtime = _make_runtime(rules)

        status = runtime.get_status_for_bot(self.QQ_ID)

        assert status.running is False
        assert status.pid is None
