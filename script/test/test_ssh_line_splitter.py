# -*- coding: utf-8 -*-
"""[`_LineSplitter`](src/core/remote/ssh_client.py) 单元测试。

回归保护点:

- ``\\r\\n`` 行尾必须视为单换行(否则部署控制台每行后会出现空白行)
- 单独 ``\\r`` 也是换行(curl 进度条场景)
- ``\\r`` 落在缓冲末尾时不能立即切分(防止跨读取边界的 CRLF 退化为两次切分)
- ``flush`` 必须把残留缓冲作为最后一行返回
"""

from __future__ import annotations

from src.core.remote.ssh_client import _LineSplitter


class TestLineSplitterCRLF:
    def test_crlf_is_treated_as_single_line_break(self) -> None:
        splitter = _LineSplitter()
        # 模拟 PTY 模式下两行带 \r\n 的输出
        lines = splitter.feed("hello\r\nworld\r\n")
        assert lines == ["hello", "world"], "CRLF 不能切成两次, 否则会产生空白行"
        assert splitter.flush() == []

    def test_lf_only(self) -> None:
        splitter = _LineSplitter()
        assert splitter.feed("a\nb\nc\n") == ["a", "b", "c"]
        assert splitter.flush() == []

    def test_lone_cr_is_treated_as_line_break(self) -> None:
        # curl 进度条只用 \r 刷新行
        splitter = _LineSplitter()
        # 末尾不是 \r, 所有 \r 都会立即切分
        lines = splitter.feed("a\rb\rc\n")
        assert lines == ["a", "b", "c"]

    def test_cr_at_buffer_boundary_is_deferred(self) -> None:
        """跨读取边界的 CRLF 不能退化为两次切分。"""
        splitter = _LineSplitter()

        # 第一次读取以 \r 结尾(可能 \n 还在路上)
        first = splitter.feed("hello\r")
        # \r 落在末尾, 暂不切分, 等下一次
        assert first == [], f"\\r 末尾应延迟切分, 实际: {first!r}"

        # 第二次读取带来 \n
        second = splitter.feed("\nworld\r\n")
        assert second == ["hello", "world"], (
            f"跨边界的 CRLF 必须合并为一个换行, 实际: {second!r}"
        )

    def test_cr_at_boundary_followed_by_non_lf(self) -> None:
        """跨边界 \\r 后面如果不是 \\n, 就视作孤立 \\r 换行。"""
        splitter = _LineSplitter()
        assert splitter.feed("a\r") == []
        assert splitter.feed("b\n") == ["a", "b"]

    def test_flush_returns_residual_buffer(self) -> None:
        splitter = _LineSplitter()
        assert splitter.feed("partial") == []
        # 没有终结换行, flush 时把残留作为最后一行
        assert splitter.flush() == ["partial"]
        # 二次 flush 不应再返回内容
        assert splitter.flush() == []

    def test_mixed_long_chunk(self) -> None:
        splitter = _LineSplitter()
        # 模拟一段混合: PROGRESS 行 + curl 进度条 + 普通日志
        chunk = (
            "[PROGRESS] 10 step\r\n"
            "  0  100M  0  1024k    0     0   100k      0  0:17:00 --:--:--  0:17:00  100k\r"
            " 50  100M 50   50M    0     0  500k      0  0:00:50  0:00:30  0:00:20  500k\r\n"
            "[INFO] download complete\r\n"
        )
        lines = splitter.feed(chunk)
        # 期望: 1 个 PROGRESS, 2 个 curl 行, 1 个 INFO; 共 4 行, 没有空白行
        assert len(lines) == 4
        assert lines[0] == "[PROGRESS] 10 step"
        assert lines[3] == "[INFO] download complete"
        assert all(line != "" for line in lines), (
            "CRLF 不应产生空白行"
        )

    def test_empty_lines_preserved_when_explicit(self) -> None:
        """显式的双换行 \\n\\n 应当切出一个真实的空行。"""
        splitter = _LineSplitter()
        # \n\n 中间确实有一个空行(用户显式发的)
        lines = splitter.feed("a\n\nb\n")
        assert lines == ["a", "", "b"]
