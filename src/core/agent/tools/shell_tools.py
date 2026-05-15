# -*- coding: utf-8 -*-
"""Shell 执行工具.

实现 shell_exec 内置工具, 提供在工作区目录内执行 shell 命令的能力, 
支持 30 秒超时保护. 
"""

from __future__ import annotations

import asyncio
from pathlib import Path

from pydantic import BaseModel, Field

from src.core.agent.tool import ToolDefinition, ToolResult


# ---------------------------------------------------------------------------
# Parameter model
# ---------------------------------------------------------------------------


class ShellExecParams(BaseModel):
    """shell_exec 工具参数."""

    command: str = Field(description="要执行的 shell 命令")
    cwd: str = Field(default=".", description="命令执行的工作目录（相对于工作区根目录）")


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

_TIMEOUT_SECONDS = 30


# ---------------------------------------------------------------------------
# ShellExecTool
# ---------------------------------------------------------------------------


class ShellExecTool(ToolDefinition):
    """在工作区目录内执行 shell 命令.

    支持 30 秒超时保护, 超时时终止进程并返回错误. 
    返回 stdout, stderr 和 exit_code. 
    """

    tool_id = "shell_exec"
    description = "在工作区目录内执行 shell 命令（30 秒超时），返回 stdout/stderr/exit_code"
    parameters_schema = ShellExecParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行 shell 命令."""
        assert isinstance(params, ShellExecParams)

        # 解析工作目录
        try:
            cwd = (self._workspace_dir / params.cwd).resolve()
        except (OSError, ValueError) as exc:
            return ToolResult(
                output=f"Invalid working directory '{params.cwd}': {exc}",
                is_error=True,
            )

        # 确保工作目录在工作区内
        try:
            cwd.relative_to(self._workspace_dir)
        except ValueError:
            return ToolResult(
                output=f"Path traversal violation: cwd '{params.cwd}' resolves outside workspace",
                is_error=True,
            )

        # 确保工作目录存在
        if not cwd.is_dir():
            return ToolResult(
                output=f"Working directory does not exist: '{params.cwd}'",
                is_error=True,
            )

        # 创建子进程
        try:
            process = await asyncio.create_subprocess_shell(
                params.command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=str(cwd),
            )
        except OSError as exc:
            return ToolResult(
                output=f"Failed to start process: {exc}",
                is_error=True,
            )

        # 等待进程完成, 带超时
        try:
            stdout_bytes, stderr_bytes = await asyncio.wait_for(
                process.communicate(),
                timeout=_TIMEOUT_SECONDS,
            )
        except asyncio.TimeoutError:
            # 超时: 终止进程
            try:
                process.kill()
            except ProcessLookupError:
                pass  # 进程已退出
            # 等待进程清理 (避免僵尸进程) 
            try:
                await asyncio.wait_for(process.wait(), timeout=5)
            except asyncio.TimeoutError:
                pass

            return ToolResult(
                output=f"Command timed out after {_TIMEOUT_SECONDS} seconds: '{params.command}'",
                is_error=True,
                metadata={"timeout_seconds": _TIMEOUT_SECONDS},
            )

        # 解码输出
        stdout = stdout_bytes.decode("utf-8", errors="replace")
        stderr = stderr_bytes.decode("utf-8", errors="replace")
        exit_code = process.returncode or 0

        # 构建输出
        output_parts: list[str] = []
        output_parts.append(f"exit_code: {exit_code}")
        if stdout:
            output_parts.append(f"stdout:\n{stdout}")
        if stderr:
            output_parts.append(f"stderr:\n{stderr}")

        output = "\n".join(output_parts)

        return ToolResult(
            output=output,
            metadata={"exit_code": exit_code},
        )
