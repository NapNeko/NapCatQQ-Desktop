# -*- coding: utf-8 -*-
"""OpenContextFileTool 单元测试.

测试上下文文件打开工具: 文件不存在时创建模板, 文件已存在时直接打开, 
平台感知的编辑器调用等场景. 
"""

from __future__ import annotations

import asyncio
from pathlib import Path
from unittest.mock import patch

import pytest

from src.core.agent.tools.context_tools import (
    OpenContextFileTool,
    OpenContextFileParams,
    _TEMPLATE_CONTENT,
)


@pytest.fixture()
def context_file(tmp_path: Path) -> Path:
    """返回临时目录下的上下文文件路径 (不创建文件) ."""
    return tmp_path / "agent_context.md"


@pytest.fixture()
def tool(context_file: Path) -> OpenContextFileTool:
    """创建 OpenContextFileTool 实例."""
    return OpenContextFileTool(context_file_path=context_file)


class TestOpenContextFileToolMetadata:
    """工具元数据测试."""

    def test_tool_id(self, tool: OpenContextFileTool) -> None:
        assert tool.tool_id == "open_context_file"

    def test_description_non_empty(self, tool: OpenContextFileTool) -> None:
        assert len(tool.description.strip()) > 0

    def test_parameters_schema_is_pydantic_model(self, tool: OpenContextFileTool) -> None:
        from pydantic import BaseModel

        assert issubclass(tool.parameters_schema, BaseModel)


class TestOpenContextFileToolCreatesFile:
    """文件不存在时创建模板的行为测试."""

    def test_creates_file_when_not_exists(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ) as mock_open:
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert context_file.exists()
        assert not result.is_error
        mock_open.assert_called_once_with(context_file)

    def test_created_file_has_template_content(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ):
            asyncio.run(tool.execute(OpenContextFileParams()))

        content = context_file.read_text(encoding="utf-8")
        assert content == _TEMPLATE_CONTENT

    def test_result_indicates_created(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ):
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert result.metadata is not None
        assert result.metadata["created"] is True
        assert "已创建" in result.output

    def test_creates_parent_directories(self, tmp_path: Path) -> None:
        nested_path = tmp_path / "sub" / "dir" / "agent_context.md"
        tool = OpenContextFileTool(context_file_path=nested_path)

        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ):
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert not result.is_error
        assert nested_path.exists()


class TestOpenContextFileToolOpensExisting:
    """文件已存在时直接打开的行为测试."""

    def test_opens_existing_file(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        context_file.write_text("existing content", encoding="utf-8")

        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ) as mock_open:
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert not result.is_error
        mock_open.assert_called_once_with(context_file)

    def test_does_not_overwrite_existing_file(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        original_content = "my custom context"
        context_file.write_text(original_content, encoding="utf-8")

        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ):
            asyncio.run(tool.execute(OpenContextFileParams()))

        assert context_file.read_text(encoding="utf-8") == original_content

    def test_result_indicates_not_created(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        context_file.write_text("content", encoding="utf-8")

        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor"
        ):
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert result.metadata is not None
        assert result.metadata["created"] is False
        assert "已打开" in result.output


class TestOpenContextFileToolErrors:
    """错误处理测试."""

    def test_returns_error_when_cannot_create_file(
        self, tmp_path: Path
    ) -> None:
        bad_path = tmp_path / "agent_context.md"
        tool = OpenContextFileTool(context_file_path=bad_path)

        with patch(
            "pathlib.Path.write_text", side_effect=OSError("Permission denied")
        ):
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert result.is_error
        assert "无法创建" in result.output

    def test_returns_error_when_cannot_open_editor(
        self, tool: OpenContextFileTool, context_file: Path
    ) -> None:
        context_file.write_text("content", encoding="utf-8")

        with patch(
            "src.core.agent.tools.context_tools._open_with_default_editor",
            side_effect=OSError("No editor found"),
        ):
            result = asyncio.run(tool.execute(OpenContextFileParams()))

        assert result.is_error
        assert "无法打开" in result.output


class TestOpenWithDefaultEditor:
    """平台感知的编辑器打开函数测试."""

    @patch("platform.system", return_value="Windows")
    @patch("os.startfile")
    def test_windows_uses_startfile(
        self, mock_startfile, mock_system, tmp_path: Path
    ) -> None:
        from src.core.agent.tools.context_tools import _open_with_default_editor

        file_path = tmp_path / "test.md"
        file_path.write_text("test", encoding="utf-8")

        _open_with_default_editor(file_path)
        mock_startfile.assert_called_once_with(str(file_path))

    @patch("platform.system", return_value="Darwin")
    @patch("subprocess.Popen")
    def test_macos_uses_open_command(
        self, mock_popen, mock_system, tmp_path: Path
    ) -> None:
        from src.core.agent.tools.context_tools import _open_with_default_editor

        file_path = tmp_path / "test.md"
        file_path.write_text("test", encoding="utf-8")

        _open_with_default_editor(file_path)
        mock_popen.assert_called_once()
        args = mock_popen.call_args[0][0]
        assert args[0] == "open"
        assert str(file_path) in args

    @patch("platform.system", return_value="Linux")
    @patch("subprocess.Popen")
    def test_linux_uses_xdg_open(
        self, mock_popen, mock_system, tmp_path: Path
    ) -> None:
        from src.core.agent.tools.context_tools import _open_with_default_editor

        file_path = tmp_path / "test.md"
        file_path.write_text("test", encoding="utf-8")

        _open_with_default_editor(file_path)
        mock_popen.assert_called_once()
        args = mock_popen.call_args[0][0]
        assert args[0] == "xdg-open"
        assert str(file_path) in args
