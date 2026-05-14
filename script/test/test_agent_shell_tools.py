# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tools/shell_tools.py.

验证 ShellExecTool 的核心功能，包括命令执行、超时处理、
工作目录验证和路径遍历防护。
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path

import pytest
from pydantic import BaseModel

from src.core.agent.tools.shell_tools import (
    ShellExecParams,
    ShellExecTool,
)


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    """创建临时工作区目录."""
    return tmp_path


@pytest.fixture
def shell_tool(workspace: Path) -> ShellExecTool:
    """创建 ShellExecTool 实例."""
    return ShellExecTool(workspace_dir=workspace)


# ---------------------------------------------------------------------------
# Basic execution tests
# ---------------------------------------------------------------------------


class TestShellExecTool:
    """ShellExecTool 单元测试."""

    def test_simple_echo_command(self, workspace: Path, shell_tool: ShellExecTool) -> None:
        """执行简单 echo 命令应返回 stdout."""
        if sys.platform == "win32":
            params = ShellExecParams(command="echo hello")
        else:
            params = ShellExecParams(command="echo hello")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert "hello" in result.output
        assert "exit_code: 0" in result.output

    def test_command_with_exit_code(self, workspace: Path, shell_tool: ShellExecTool) -> None:
        """执行返回非零退出码的命令."""
        if sys.platform == "win32":
            params = ShellExecParams(command="cmd /c exit 42")
        else:
            params = ShellExecParams(command="exit 42")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert "exit_code: 42" in result.output
        assert result.metadata is not None
        assert result.metadata["exit_code"] == 42

    def test_command_with_stderr(self, workspace: Path, shell_tool: ShellExecTool) -> None:
        """执行产生 stderr 输出的命令."""
        if sys.platform == "win32":
            params = ShellExecParams(command="echo error_msg 1>&2")
        else:
            params = ShellExecParams(command="echo error_msg >&2")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert "stderr:" in result.output
        assert "error_msg" in result.output

    def test_command_in_subdirectory(self, workspace: Path, shell_tool: ShellExecTool) -> None:
        """在子目录中执行命令."""
        sub = workspace / "subdir"
        sub.mkdir()
        (sub / "marker.txt").write_text("found", encoding="utf-8")

        if sys.platform == "win32":
            params = ShellExecParams(command="type marker.txt", cwd="subdir")
        else:
            params = ShellExecParams(command="cat marker.txt", cwd="subdir")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert "found" in result.output

    def test_default_cwd_is_workspace(self, workspace: Path, shell_tool: ShellExecTool) -> None:
        """默认工作目录应为工作区根目录."""
        (workspace / "root_file.txt").write_text("root_content", encoding="utf-8")

        if sys.platform == "win32":
            params = ShellExecParams(command="type root_file.txt")
        else:
            params = ShellExecParams(command="cat root_file.txt")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert "root_content" in result.output

    def test_cwd_path_traversal_blocked(
        self, workspace: Path, shell_tool: ShellExecTool
    ) -> None:
        """cwd 路径遍历攻击应被阻止."""
        params = ShellExecParams(command="echo hi", cwd="../../../")
        result = _run(shell_tool.execute(params))
        assert result.is_error is True
        assert "traversal" in result.output.lower()

    def test_cwd_nonexistent_directory(
        self, workspace: Path, shell_tool: ShellExecTool
    ) -> None:
        """不存在的工作目录应返回错误."""
        params = ShellExecParams(command="echo hi", cwd="nonexistent_dir")
        result = _run(shell_tool.execute(params))
        assert result.is_error is True
        assert "does not exist" in result.output.lower()

    def test_timeout_terminates_process(
        self, workspace: Path, shell_tool: ShellExecTool, monkeypatch
    ) -> None:
        """超时时应终止进程并返回 is_error=True."""
        import src.core.agent.tools.shell_tools as shell_module

        # 将超时设置为 1 秒以加速测试
        monkeypatch.setattr(shell_module, "_TIMEOUT_SECONDS", 1)

        if sys.platform == "win32":
            params = ShellExecParams(command="ping -n 60 127.0.0.1")
        else:
            params = ShellExecParams(command="sleep 60")
        result = _run(shell_tool.execute(params))
        assert result.is_error is True
        assert "timed out" in result.output.lower() or "timeout" in result.output.lower()
        assert result.metadata is not None
        assert result.metadata["timeout_seconds"] == 1

    def test_tool_id_and_description(self, shell_tool: ShellExecTool) -> None:
        """验证 tool_id 和 description 属性."""
        assert shell_tool.tool_id == "shell_exec"
        assert len(shell_tool.description) > 0

    def test_parameters_schema_is_pydantic_model(self, shell_tool: ShellExecTool) -> None:
        """验证 parameters_schema 是 pydantic BaseModel 子类."""
        assert issubclass(shell_tool.parameters_schema, BaseModel)

    def test_metadata_contains_exit_code(
        self, workspace: Path, shell_tool: ShellExecTool
    ) -> None:
        """成功执行时 metadata 应包含 exit_code."""
        if sys.platform == "win32":
            params = ShellExecParams(command="echo ok")
        else:
            params = ShellExecParams(command="echo ok")
        result = _run(shell_tool.execute(params))
        assert result.is_error is False
        assert result.metadata is not None
        assert "exit_code" in result.metadata
        assert result.metadata["exit_code"] == 0
