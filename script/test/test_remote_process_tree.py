# -*- coding: utf-8 -*-
"""[`fetch_process_tree_rss_bytes`](src/core/remote/process_tree.py) 单测.

NC / SL backend 都依赖此函数累加远端 Bot 进程树的 RSS, 是用户卡片"内存"列的
唯一真值来源. 这里覆盖正常 BFS / 非根 PID / ps 失败 / 无效行 / 单位换算等场景.
"""

from __future__ import annotations

from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import RemoteCommandResult
from src.core.remote.process_tree import (
    fetch_process_tree_rss_bytes,
    fetch_remote_total_memory_bytes,
)


class _FakeBackend(ExecutionBackend):
    """最小化的 ``ExecutionBackend`` mock; 仅 ``run`` 被调用, 其余 raise."""

    def __init__(self, ok: bool = True, stdout: str = "") -> None:
        self._ok = ok
        self._stdout = stdout
        self.run_calls: list[str] = []

    def run(  # type: ignore[override]
        self,
        command: str,
        *,
        timeout: float | None = None,  # noqa: ARG002
        check: bool = False,  # noqa: ARG002
    ) -> RemoteCommandResult:
        self.run_calls.append(command)
        return RemoteCommandResult(
            command=command,
            exit_status=0 if self._ok else 1,
            stdout=self._stdout,
            stderr="",
        )

    def ensure_directory(self, *args, **kwargs):  # noqa: ANN001
        raise NotImplementedError

    def upload_file(self, *args, **kwargs):  # noqa: ANN001
        raise NotImplementedError

    def download_file(self, *args, **kwargs):  # noqa: ANN001
        raise NotImplementedError


# ==================== 正常路径 ====================
class TestFetchProcessTreeRssBytes:
    def test_root_pid_only(self) -> None:
        """单进程, 无子进程: RSS = 单进程 RSS (KiB→字节)."""
        backend = _FakeBackend(stdout="123 1 8192\n")
        rss = fetch_process_tree_rss_bytes(backend, 123)
        assert rss == 8192 * 1024

    def test_root_with_direct_children(self) -> None:
        """root + 2 个直接子进程: RSS = 三者之和."""
        backend = _FakeBackend(
            stdout=(
                "123 1 8192\n"
                "456 123 4096\n"
                "789 123 2048\n"
            )
        )
        rss = fetch_process_tree_rss_bytes(backend, 123)
        assert rss == (8192 + 4096 + 2048) * 1024

    def test_root_with_grandchildren(self) -> None:
        """root → child → grandchild 三层: 全部累加."""
        backend = _FakeBackend(
            stdout=(
                "100 1   1000\n"
                "200 100 2000\n"
                "300 200 3000\n"
                "400 300 4000\n"
            )
        )
        rss = fetch_process_tree_rss_bytes(backend, 100)
        assert rss == (1000 + 2000 + 3000 + 4000) * 1024

    def test_unrelated_processes_excluded(self) -> None:
        """同一台机的其他进程不应被算进来."""
        backend = _FakeBackend(
            stdout=(
                "100 1   1000\n"
                "200 100 2000\n"
                "999 1   9999999\n"  # 无关进程, 必须排除
            )
        )
        rss = fetch_process_tree_rss_bytes(backend, 100)
        assert rss == (1000 + 2000) * 1024

    def test_pid_not_in_ps_returns_none(self) -> None:
        """pid 已退出 (不在 ps 输出): 返 None 让 UI 显示"未知"."""
        backend = _FakeBackend(stdout="100 1 1000\n")
        rss = fetch_process_tree_rss_bytes(backend, 999)
        assert rss is None

    def test_ps_fails_returns_none(self) -> None:
        """``ps`` 失败 (网络断 / 命令缺失): 返 None."""
        backend = _FakeBackend(ok=False, stdout="")
        rss = fetch_process_tree_rss_bytes(backend, 100)
        assert rss is None

    def test_malformed_lines_skipped(self) -> None:
        """ps 输出含杂行 (header 残留 / 空行): 解析器静默跳过."""
        backend = _FakeBackend(
            stdout=(
                "  PID  PPID  RSS\n"  # 残留 header
                "\n"
                "100 1 1000\n"
                "garbage line\n"
                "200 100 2000\n"
            )
        )
        rss = fetch_process_tree_rss_bytes(backend, 100)
        assert rss == (1000 + 2000) * 1024

    def test_command_uses_ps_full_listing(self) -> None:
        """实现必须用 ``ps -e -o pid=,ppid=,rss=`` 拉全量, 而不是
        ``ps -p <pid>`` 单进程查询; 否则会漏 helper 子进程.
        """
        backend = _FakeBackend(stdout="100 1 1000\n")
        fetch_process_tree_rss_bytes(backend, 100)
        assert any("ps -e -o pid=,ppid=,rss=" in c for c in backend.run_calls)

    def test_circular_parent_does_not_loop(self) -> None:
        """异常 ps 输出 (假设有自指 ppid): BFS 用 visited 集合保证不死循环."""
        backend = _FakeBackend(
            stdout=(
                "100 100 1000\n"  # ppid 等于自身 (异常但不应 hang)
                "200 100 2000\n"
            )
        )
        rss = fetch_process_tree_rss_bytes(backend, 100)
        # 仅算到 100 + 200; 自环不会让 100 重复计入
        assert rss == (1000 + 2000) * 1024


# ==================== 远端总内存 ====================
class TestFetchRemoteTotalMemoryBytes:
    """``fetch_remote_total_memory_bytes`` 单测; 用于 Bot 卡片 ``X / Y MB`` 的 Y."""

    def test_normal_meminfo_parses(self) -> None:
        """标准 ``/proc/meminfo`` 输出: 取 ``MemTotal`` 行 KiB × 1024."""
        backend = _FakeBackend(
            stdout=(
                "MemTotal:        2048576 kB\n"
                "MemFree:         1234567 kB\n"
                "MemAvailable:    1500000 kB\n"
            )
        )
        total = fetch_remote_total_memory_bytes(backend)
        assert total == 2048576 * 1024  # 2 GiB 服务器

    def test_command_uses_proc_meminfo(self) -> None:
        backend = _FakeBackend(stdout="MemTotal:        1024 kB\n")
        fetch_remote_total_memory_bytes(backend)
        assert any("/proc/meminfo" in c for c in backend.run_calls)

    def test_cat_fails_returns_none(self) -> None:
        """``cat`` 失败 (无 /proc/meminfo / SSH 闪断): 返 None 让 UI fallback."""
        backend = _FakeBackend(ok=False, stdout="")
        assert fetch_remote_total_memory_bytes(backend) is None

    def test_missing_memtotal_line_returns_none(self) -> None:
        """``/proc/meminfo`` 没有 ``MemTotal`` 行 (异常容器环境): 返 None."""
        backend = _FakeBackend(
            stdout="MemFree:         1024 kB\nBuffers:         512 kB\n"
        )
        assert fetch_remote_total_memory_bytes(backend) is None

    def test_empty_stdout_returns_none(self) -> None:
        backend = _FakeBackend(stdout="")
        assert fetch_remote_total_memory_bytes(backend) is None

    def test_memtotal_with_extra_whitespace(self) -> None:
        """字段间空白宽窄不一: 正则用 ``\\s*`` 兼容."""
        backend = _FakeBackend(stdout="MemTotal:    16384 kB\n")
        assert fetch_remote_total_memory_bytes(backend) == 16384 * 1024
