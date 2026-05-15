# -*- coding: utf-8 -*-
"""ContextLoader 单元测试.

测试用户上下文文件加载, mtime 缓存, 截断和文件不存在等场景. 
"""

from __future__ import annotations

import logging
from pathlib import Path

import pytest

from src.core.agent.context_loader import MAX_CONTEXT_LENGTH, ContextLoader


@pytest.fixture()
def context_file(tmp_path: Path) -> Path:
    """返回临时目录下的上下文文件路径 (不创建文件) ."""
    return tmp_path / "agent_context.md"


class TestContextLoaderFileNotExist:
    """文件不存在时的行为测试."""

    def test_returns_empty_string(self, context_file: Path) -> None:
        loader = ContextLoader(context_file)
        assert loader.load() == ""

    def test_no_warning_logged(self, context_file: Path, caplog: pytest.LogCaptureFixture) -> None:
        loader = ContextLoader(context_file)
        with caplog.at_level(logging.WARNING):
            loader.load()
        assert caplog.records == []


class TestContextLoaderBasicLoad:
    """基本文件加载测试."""

    def test_loads_file_content(self, context_file: Path) -> None:
        content = "# My Custom Context\nSome plugin knowledge."
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)
        assert loader.load() == content

    def test_loads_empty_file(self, context_file: Path) -> None:
        context_file.write_text("", encoding="utf-8")
        loader = ContextLoader(context_file)
        assert loader.load() == ""

    def test_loads_unicode_content(self, context_file: Path) -> None:
        content = "# 自定义上下文\n这是中文内容 🎉"
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)
        assert loader.load() == content


class TestContextLoaderMtimeCache:
    """基于 mtime 的缓存行为测试."""

    def test_returns_cached_content_when_mtime_unchanged(self, context_file: Path) -> None:
        context_file.write_text("original content", encoding="utf-8")
        loader = ContextLoader(context_file)

        # 第一次加载
        result1 = loader.load()
        assert result1 == "original content"

        # 第二次加载 (mtime 未变) 应返回缓存
        result2 = loader.load()
        assert result2 == "original content"

    def test_reloads_when_file_modified(self, context_file: Path) -> None:
        import os
        import time

        context_file.write_text("version 1", encoding="utf-8")
        loader = ContextLoader(context_file)
        assert loader.load() == "version 1"

        # 修改文件并确保 mtime 不同
        time.sleep(0.05)
        context_file.write_text("version 2", encoding="utf-8")
        # 强制设置不同的 mtime
        new_mtime = context_file.stat().st_mtime + 1
        os.utime(context_file, (new_mtime, new_mtime))

        assert loader.load() == "version 2"

    def test_returns_empty_when_file_deleted_after_cache(self, context_file: Path) -> None:
        context_file.write_text("some content", encoding="utf-8")
        loader = ContextLoader(context_file)
        assert loader.load() == "some content"

        # 删除文件
        context_file.unlink()
        assert loader.load() == ""


class TestContextLoaderTruncation:
    """超过最大长度时的截断行为测试."""

    def test_truncates_at_max_length(self, context_file: Path) -> None:
        # 创建超过限制的内容
        content = "A" * (MAX_CONTEXT_LENGTH + 100)
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)

        result = loader.load()
        assert len(result) == MAX_CONTEXT_LENGTH
        assert result == "A" * MAX_CONTEXT_LENGTH

    def test_logs_warning_on_truncation(
        self, context_file: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        content = "B" * (MAX_CONTEXT_LENGTH + 500)
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)

        with caplog.at_level(logging.WARNING):
            loader.load()

        assert len(caplog.records) == 1
        record = caplog.records[0]
        assert record.levelname == "WARNING"
        assert str(MAX_CONTEXT_LENGTH) in record.message
        assert str(MAX_CONTEXT_LENGTH + 500) in record.message

    def test_no_warning_at_exact_limit(self, context_file: Path, caplog: pytest.LogCaptureFixture) -> None:
        content = "C" * MAX_CONTEXT_LENGTH
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)

        with caplog.at_level(logging.WARNING):
            result = loader.load()

        assert len(result) == MAX_CONTEXT_LENGTH
        assert caplog.records == []

    def test_no_warning_below_limit(self, context_file: Path, caplog: pytest.LogCaptureFixture) -> None:
        content = "D" * (MAX_CONTEXT_LENGTH - 1)
        context_file.write_text(content, encoding="utf-8")
        loader = ContextLoader(context_file)

        with caplog.at_level(logging.WARNING):
            result = loader.load()

        assert len(result) == MAX_CONTEXT_LENGTH - 1
        assert caplog.records == []
