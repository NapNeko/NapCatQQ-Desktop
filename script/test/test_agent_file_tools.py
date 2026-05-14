# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tools/file_tools.py.

验证 FileReadTool、FileWriteTool、FileEditTool 的核心功能，
包括路径遍历防护、文件读写、搜索替换逻辑。
"""

from __future__ import annotations

import asyncio
from pathlib import Path

import pytest
from pydantic import BaseModel

from src.core.agent.tools.file_tools import (
    FileEditParams,
    FileEditTool,
    FileReadParams,
    FileReadTool,
    FileWriteParams,
    FileWriteTool,
)


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """创建临时工作区目录."""
    return tmp_path


@pytest.fixture
def read_tool(workspace: Path) -> FileReadTool:
    """创建 FileReadTool 实例."""
    return FileReadTool(workspace_dir=workspace)


@pytest.fixture
def write_tool(workspace: Path) -> FileWriteTool:
    """创建 FileWriteTool 实例."""
    return FileWriteTool(workspace_dir=workspace)


@pytest.fixture
def edit_tool(workspace: Path) -> FileEditTool:
    """创建 FileEditTool 实例."""
    return FileEditTool(workspace_dir=workspace)


# ---------------------------------------------------------------------------
# FileReadTool tests
# ---------------------------------------------------------------------------


class TestFileReadTool:
    """FileReadTool 单元测试."""

    def test_read_existing_file(self, workspace: Path, read_tool: FileReadTool) -> None:
        """读取存在的文件应返回文件内容."""
        (workspace / "hello.txt").write_text("Hello, World!", encoding="utf-8")
        params = FileReadParams(path="hello.txt")
        result = _run(read_tool.execute(params))
        assert result.is_error is False
        assert result.output == "Hello, World!"

    def test_read_file_in_subdirectory(self, workspace: Path, read_tool: FileReadTool) -> None:
        """读取子目录中的文件."""
        sub = workspace / "src" / "core"
        sub.mkdir(parents=True)
        (sub / "main.py").write_text("print('hi')", encoding="utf-8")
        params = FileReadParams(path="src/core/main.py")
        result = _run(read_tool.execute(params))
        assert result.is_error is False
        assert result.output == "print('hi')"

    def test_read_nonexistent_file(self, read_tool: FileReadTool) -> None:
        """读取不存在的文件应返回错误."""
        params = FileReadParams(path="nonexistent.txt")
        result = _run(read_tool.execute(params))
        assert result.is_error is True
        assert "not found" in result.output.lower()

    def test_path_traversal_blocked(self, workspace: Path, read_tool: FileReadTool) -> None:
        """路径遍历攻击应被阻止."""
        params = FileReadParams(path="../../../etc/passwd")
        result = _run(read_tool.execute(params))
        assert result.is_error is True
        assert "traversal" in result.output.lower()

    def test_path_traversal_with_dotdot_in_middle(
        self, workspace: Path, read_tool: FileReadTool
    ) -> None:
        """中间包含 .. 的路径遍历应被阻止."""
        params = FileReadParams(path="subdir/../../outside.txt")
        result = _run(read_tool.execute(params))
        assert result.is_error is True
        assert "traversal" in result.output.lower()

    def test_truncation_at_max_chars(self, workspace: Path, read_tool: FileReadTool) -> None:
        """超过 100000 字符的文件应被截断."""
        content = "A" * 150_000
        (workspace / "large.txt").write_text(content, encoding="utf-8")
        params = FileReadParams(path="large.txt")
        result = _run(read_tool.execute(params))
        assert result.is_error is False
        assert len(result.output) == 100_000
        assert result.metadata is not None
        assert result.metadata["truncated"] is True

    def test_read_file_within_max_chars(self, workspace: Path, read_tool: FileReadTool) -> None:
        """不超过 100000 字符的文件不应被截断."""
        content = "B" * 100_000
        (workspace / "exact.txt").write_text(content, encoding="utf-8")
        params = FileReadParams(path="exact.txt")
        result = _run(read_tool.execute(params))
        assert result.is_error is False
        assert len(result.output) == 100_000
        assert result.metadata is None

    def test_tool_id_and_description(self, read_tool: FileReadTool) -> None:
        """验证 tool_id 和 description 属性."""
        assert read_tool.tool_id == "file_read"
        assert len(read_tool.description) > 0

    def test_parameters_schema_is_pydantic_model(self, read_tool: FileReadTool) -> None:
        """验证 parameters_schema 是 pydantic BaseModel 子类."""
        assert issubclass(read_tool.parameters_schema, BaseModel)


# ---------------------------------------------------------------------------
# FileWriteTool tests
# ---------------------------------------------------------------------------


class TestFileWriteTool:
    """FileWriteTool 单元测试."""

    def test_write_new_file(self, workspace: Path, write_tool: FileWriteTool) -> None:
        """写入新文件应成功."""
        params = FileWriteParams(path="output.txt", content="Hello!")
        result = _run(write_tool.execute(params))
        assert result.is_error is False
        assert (workspace / "output.txt").read_text(encoding="utf-8") == "Hello!"

    def test_write_creates_parent_directories(
        self, workspace: Path, write_tool: FileWriteTool
    ) -> None:
        """写入时应自动创建父目录."""
        params = FileWriteParams(path="a/b/c/deep.txt", content="deep content")
        result = _run(write_tool.execute(params))
        assert result.is_error is False
        assert (workspace / "a" / "b" / "c" / "deep.txt").exists()
        assert (workspace / "a" / "b" / "c" / "deep.txt").read_text(encoding="utf-8") == "deep content"

    def test_write_overwrites_existing_file(
        self, workspace: Path, write_tool: FileWriteTool
    ) -> None:
        """写入已存在的文件应覆盖内容."""
        (workspace / "exist.txt").write_text("old", encoding="utf-8")
        params = FileWriteParams(path="exist.txt", content="new")
        result = _run(write_tool.execute(params))
        assert result.is_error is False
        assert (workspace / "exist.txt").read_text(encoding="utf-8") == "new"

    def test_write_path_traversal_blocked(
        self, workspace: Path, write_tool: FileWriteTool
    ) -> None:
        """路径遍历攻击应被阻止."""
        params = FileWriteParams(path="../outside.txt", content="malicious")
        result = _run(write_tool.execute(params))
        assert result.is_error is True
        assert "traversal" in result.output.lower()

    def test_write_returns_metadata(self, workspace: Path, write_tool: FileWriteTool) -> None:
        """写入成功应返回 bytes_written 元数据."""
        params = FileWriteParams(path="meta.txt", content="abc")
        result = _run(write_tool.execute(params))
        assert result.is_error is False
        assert result.metadata is not None
        assert result.metadata["bytes_written"] == 3

    def test_tool_id_and_description(self, write_tool: FileWriteTool) -> None:
        """验证 tool_id 和 description 属性."""
        assert write_tool.tool_id == "file_write"
        assert len(write_tool.description) > 0


# ---------------------------------------------------------------------------
# FileEditTool tests
# ---------------------------------------------------------------------------


class TestFileEditTool:
    """FileEditTool 单元测试."""

    def test_edit_single_occurrence(self, workspace: Path, edit_tool: FileEditTool) -> None:
        """恰好匹配一次时应成功替换."""
        (workspace / "code.py").write_text("def hello():\n    pass\n", encoding="utf-8")
        params = FileEditParams(path="code.py", old_str="pass", new_str="return 42")
        result = _run(edit_tool.execute(params))
        assert result.is_error is False
        content = (workspace / "code.py").read_text(encoding="utf-8")
        assert "return 42" in content
        assert "pass" not in content

    def test_edit_old_str_not_found(self, workspace: Path, edit_tool: FileEditTool) -> None:
        """old_str 不存在时应返回错误."""
        (workspace / "file.txt").write_text("hello world", encoding="utf-8")
        params = FileEditParams(path="file.txt", old_str="xyz", new_str="abc")
        result = _run(edit_tool.execute(params))
        assert result.is_error is True
        assert "not found" in result.output.lower()

    def test_edit_old_str_multiple_occurrences(
        self, workspace: Path, edit_tool: FileEditTool
    ) -> None:
        """old_str 出现多次时应返回错误."""
        (workspace / "dup.txt").write_text("foo bar foo baz foo", encoding="utf-8")
        params = FileEditParams(path="dup.txt", old_str="foo", new_str="qux")
        result = _run(edit_tool.execute(params))
        assert result.is_error is True
        assert "3" in result.output  # found 3 times

    def test_edit_nonexistent_file(self, edit_tool: FileEditTool) -> None:
        """编辑不存在的文件应返回错误."""
        params = FileEditParams(path="ghost.txt", old_str="a", new_str="b")
        result = _run(edit_tool.execute(params))
        assert result.is_error is True
        assert "not found" in result.output.lower()

    def test_edit_path_traversal_blocked(
        self, workspace: Path, edit_tool: FileEditTool
    ) -> None:
        """路径遍历攻击应被阻止."""
        params = FileEditParams(path="../../etc/hosts", old_str="a", new_str="b")
        result = _run(edit_tool.execute(params))
        assert result.is_error is True
        assert "traversal" in result.output.lower()

    def test_edit_multiline_replacement(
        self, workspace: Path, edit_tool: FileEditTool
    ) -> None:
        """多行文本替换应正常工作."""
        original = "line1\nline2\nline3\n"
        (workspace / "multi.txt").write_text(original, encoding="utf-8")
        params = FileEditParams(path="multi.txt", old_str="line2\nline3", new_str="replaced")
        result = _run(edit_tool.execute(params))
        assert result.is_error is False
        content = (workspace / "multi.txt").read_text(encoding="utf-8")
        assert content == "line1\nreplaced\n"

    def test_edit_returns_metadata(self, workspace: Path, edit_tool: FileEditTool) -> None:
        """编辑成功应返回 replacements 元数据."""
        (workspace / "m.txt").write_text("hello", encoding="utf-8")
        params = FileEditParams(path="m.txt", old_str="hello", new_str="world")
        result = _run(edit_tool.execute(params))
        assert result.is_error is False
        assert result.metadata is not None
        assert result.metadata["replacements"] == 1

    def test_tool_id_and_description(self, edit_tool: FileEditTool) -> None:
        """验证 tool_id 和 description 属性."""
        assert edit_tool.tool_id == "file_edit"
        assert len(edit_tool.description) > 0
