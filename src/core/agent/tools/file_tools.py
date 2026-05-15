# -*- coding: utf-8 -*-
"""文件操作工具集.

实现 file_read, file_write, file_edit 三个内置工具, 
提供工作区内文件的读取, 写入和编辑能力. 
"""

from __future__ import annotations

from pathlib import Path

from pydantic import BaseModel, Field

from src.core.agent.tool import ToolDefinition, ToolResult


# ---------------------------------------------------------------------------
# Parameter models
# ---------------------------------------------------------------------------


class FileReadParams(BaseModel):
    """file_read 工具参数."""

    path: str = Field(description="相对于工作区的文件路径")


class FileWriteParams(BaseModel):
    """file_write 工具参数."""

    path: str = Field(description="相对于工作区的文件路径")
    content: str = Field(description="要写入的文件内容")


class FileEditParams(BaseModel):
    """file_edit 工具参数."""

    path: str = Field(description="相对于工作区的文件路径")
    old_str: str = Field(description="要被替换的原始文本")
    new_str: str = Field(description="替换后的新文本")


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

_MAX_READ_CHARS = 100_000


def _resolve_and_check(workspace_dir: Path, relative_path: str) -> Path | ToolResult:
    """解析相对路径并检查路径遍历.

    Args:
        workspace_dir: 工作区根目录 (已 resolve) .
        relative_path: 用户提供的相对路径.

    Returns:
        解析后的绝对路径, 或路径遍历时返回 ToolResult(is_error=True).
    """
    try:
        resolved = (workspace_dir / relative_path).resolve()
    except (OSError, ValueError) as exc:
        return ToolResult(
            output=f"Invalid path '{relative_path}': {exc}",
            is_error=True,
        )

    # 路径遍历检查: resolved 必须在 workspace_dir 内部
    try:
        resolved.relative_to(workspace_dir)
    except ValueError:
        return ToolResult(
            output=f"Path traversal violation: '{relative_path}' resolves outside workspace",
            is_error=True,
        )

    return resolved


# ---------------------------------------------------------------------------
# FileReadTool
# ---------------------------------------------------------------------------


class FileReadTool(ToolDefinition):
    """读取工作区内文件内容.

    支持最大 100000 字符, 超出部分截断. 
    对路径遍历攻击进行防护. 
    """

    tool_id = "file_read"
    description = "读取工作区内指定路径的文件内容（最大 100000 字符）"
    parameters_schema = FileReadParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行文件读取."""
        assert isinstance(params, FileReadParams)

        result = _resolve_and_check(self._workspace_dir, params.path)
        if isinstance(result, ToolResult):
            return result
        resolved = result

        if not resolved.is_file():
            return ToolResult(
                output=f"File not found: '{params.path}'",
                is_error=True,
            )

        try:
            content = resolved.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            # 尝试以 latin-1 回退读取
            try:
                content = resolved.read_text(encoding="latin-1")
            except Exception as exc:
                return ToolResult(
                    output=f"Failed to read file '{params.path}': {exc}",
                    is_error=True,
                )
        except OSError as exc:
            return ToolResult(
                output=f"Failed to read file '{params.path}': {exc}",
                is_error=True,
            )

        if len(content) > _MAX_READ_CHARS:
            content = content[:_MAX_READ_CHARS]
            return ToolResult(
                output=content,
                metadata={"truncated": True, "max_chars": _MAX_READ_CHARS},
            )

        return ToolResult(output=content)


# ---------------------------------------------------------------------------
# FileWriteTool
# ---------------------------------------------------------------------------


class FileWriteTool(ToolDefinition):
    """写入文件到工作区.

    自动创建父目录 (如果不存在) . 
    """

    tool_id = "file_write"
    description = "将内容写入工作区内指定路径的文件，自动创建父目录"
    parameters_schema = FileWriteParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行文件写入."""
        assert isinstance(params, FileWriteParams)

        result = _resolve_and_check(self._workspace_dir, params.path)
        if isinstance(result, ToolResult):
            return result
        resolved = result

        try:
            resolved.parent.mkdir(parents=True, exist_ok=True)
            resolved.write_text(params.content, encoding="utf-8")
        except OSError as exc:
            return ToolResult(
                output=f"Failed to write file '{params.path}': {exc}",
                is_error=True,
            )

        return ToolResult(
            output=f"文件已写入: {params.path}",
            metadata={"bytes_written": len(params.content.encode("utf-8"))},
        )


# ---------------------------------------------------------------------------
# FileEditTool
# ---------------------------------------------------------------------------


class FileEditTool(ToolDefinition):
    """对工作区内文件执行搜索替换.

    old_str 必须在文件中恰好出现一次, 否则返回错误. 
    """

    tool_id = "file_edit"
    description = "对工作区内文件执行搜索替换操作，old_str 必须恰好匹配一次"
    parameters_schema = FileEditParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行文件编辑."""
        assert isinstance(params, FileEditParams)

        result = _resolve_and_check(self._workspace_dir, params.path)
        if isinstance(result, ToolResult):
            return result
        resolved = result

        if not resolved.is_file():
            return ToolResult(
                output=f"File not found: '{params.path}'",
                is_error=True,
            )

        try:
            content = resolved.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError) as exc:
            return ToolResult(
                output=f"Failed to read file '{params.path}': {exc}",
                is_error=True,
            )

        count = content.count(params.old_str)

        if count == 0:
            return ToolResult(
                output=f"old_str not found in '{params.path}'",
                is_error=True,
            )

        if count > 1:
            return ToolResult(
                output=f"old_str found {count} times in '{params.path}', expected exactly 1",
                is_error=True,
            )

        # 恰好匹配一次, 执行替换
        new_content = content.replace(params.old_str, params.new_str, 1)

        try:
            resolved.write_text(new_content, encoding="utf-8")
        except OSError as exc:
            return ToolResult(
                output=f"Failed to write file '{params.path}': {exc}",
                is_error=True,
            )

        return ToolResult(
            output=f"文件已编辑: {params.path}",
            metadata={"replacements": 1},
        )
