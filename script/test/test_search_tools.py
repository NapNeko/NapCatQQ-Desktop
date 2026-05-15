# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tools/search_tools.py.

验证 GrepSearchTool 和 ListDirectoryTool 的核心功能. 
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest

from src.core.agent.tools.search_tools import (
    GrepSearchParams,
    GrepSearchTool,
    ListDirectoryParams,
    ListDirectoryTool,
)


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """创建一个包含测试文件的临时工作区."""
    # 创建目录结构
    (tmp_path / "src").mkdir()
    (tmp_path / "src" / "core").mkdir()
    (tmp_path / "docs").mkdir()

    # 创建测试文件
    (tmp_path / "main.py").write_text(
        "# main entry\nimport os\nprint('hello world')\n", encoding="utf-8"
    )
    (tmp_path / "src" / "utils.py").write_text(
        "def helper():\n    return 42\n\ndef another():\n    return 'TODO: fix'\n",
        encoding="utf-8",
    )
    (tmp_path / "src" / "core" / "engine.py").write_text(
        "class Engine:\n    pass\n", encoding="utf-8"
    )
    (tmp_path / "docs" / "readme.md").write_text(
        "# Project\nThis is a TODO item.\n", encoding="utf-8"
    )
    return tmp_path


# ---------------------------------------------------------------------------
# GrepSearchTool Tests
# ---------------------------------------------------------------------------


class TestGrepSearchTool:
    """GrepSearchTool 单元测试."""

    def test_basic_search(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="TODO")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "TODO" in result.output
        assert result.metadata is not None
        assert result.metadata["match_count"] >= 1

    def test_regex_pattern(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern=r"def \w+\(\)")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "def helper()" in result.output
        assert "def another()" in result.output

    def test_no_matches(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="nonexistent_pattern_xyz")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "No matches found" in result.output
        assert result.metadata is not None
        assert result.metadata["match_count"] == 0

    def test_invalid_regex(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="[invalid")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "Invalid regex" in result.output

    def test_path_traversal(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="test", path="../../../etc")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "traversal" in result.output.lower() or "outside" in result.output.lower()

    def test_search_in_subdirectory(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="class", path="src/core")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "Engine" in result.output

    def test_result_format_contains_path_and_line_number(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="import os")
        result = _run(tool.execute(params))

        assert not result.is_error
        # 结果格式应为 "path:line_num: content"
        lines = result.output.strip().split("\n")
        assert len(lines) >= 1
        parts = lines[0].split(":")
        assert len(parts) >= 3  # path:line_num: content

    def test_max_results_limit(self, workspace: Path) -> None:
        """测试结果限制为 50 条."""
        # 创建一个包含大量匹配行的文件
        many_lines = "\n".join([f"match_line_{i}" for i in range(100)])
        (workspace / "many_matches.txt").write_text(many_lines, encoding="utf-8")

        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="match_line_")
        result = _run(tool.execute(params))

        assert not result.is_error
        lines = result.output.strip().split("\n")
        assert len(lines) <= 50
        assert result.metadata is not None
        assert result.metadata["truncated"] is True

    def test_nonexistent_path(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="test", path="nonexistent_dir")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "not found" in result.output.lower()

    def test_search_single_file(self, workspace: Path) -> None:
        tool = GrepSearchTool(workspace_dir=workspace)
        params = GrepSearchParams(pattern="hello", path="main.py")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "hello" in result.output


# ---------------------------------------------------------------------------
# ListDirectoryTool Tests
# ---------------------------------------------------------------------------


class TestListDirectoryTool:
    """ListDirectoryTool 单元测试."""

    def test_list_root(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams()
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "src" in result.output
        assert "docs" in result.output
        assert "main.py" in result.output

    def test_list_subdirectory(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="src")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "core" in result.output
        assert "utils.py" in result.output

    def test_depth_1_default(self, workspace: Path) -> None:
        """默认 depth=1 只列出直接子项."""
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="src", depth=1)
        result = _run(tool.execute(params))

        assert not result.is_error
        # depth=1 应该列出 src/ 下的直接子项
        assert "core" in result.output
        assert "utils.py" in result.output
        # 但不应递归到 core/ 内部
        assert "engine.py" not in result.output

    def test_depth_2(self, workspace: Path) -> None:
        """depth=2 应递归到第二层."""
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="src", depth=2)
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "engine.py" in result.output

    def test_depth_max_3(self) -> None:
        """depth 最大为 3."""
        # pydantic 验证应拒绝 depth > 3
        with pytest.raises(Exception):
            ListDirectoryParams(path=".", depth=4)

    def test_depth_min_1(self) -> None:
        """depth 最小为 1."""
        with pytest.raises(Exception):
            ListDirectoryParams(path=".", depth=0)

    def test_nonexistent_directory(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="nonexistent")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "not found" in result.output.lower()

    def test_path_is_file(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="main.py")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "not a directory" in result.output.lower()

    def test_path_traversal(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="../../../")
        result = _run(tool.execute(params))

        assert result.is_error
        assert "traversal" in result.output.lower() or "outside" in result.output.lower()

    def test_empty_directory(self, workspace: Path) -> None:
        (workspace / "empty_dir").mkdir()
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams(path="empty_dir")
        result = _run(tool.execute(params))

        assert not result.is_error
        assert "empty" in result.output.lower()

    def test_directories_listed_before_files(self, workspace: Path) -> None:
        """目录应排在文件前面."""
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams()
        result = _run(tool.execute(params))

        assert not result.is_error
        lines = result.output.strip().split("\n")
        # 找到第一个目录和第一个文件的位置
        first_dir_idx = None
        first_file_idx = None
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.endswith("/") and first_dir_idx is None:
                first_dir_idx = i
            elif not stripped.endswith("/") and first_file_idx is None:
                first_file_idx = i
        # 目录应在文件之前
        if first_dir_idx is not None and first_file_idx is not None:
            assert first_dir_idx < first_file_idx

    def test_metadata_entry_count(self, workspace: Path) -> None:
        tool = ListDirectoryTool(workspace_dir=workspace)
        params = ListDirectoryParams()
        result = _run(tool.execute(params))

        assert not result.is_error
        assert result.metadata is not None
        assert result.metadata["entry_count"] > 0
