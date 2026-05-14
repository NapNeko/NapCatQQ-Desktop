# -*- coding: utf-8 -*-
"""搜索工具集.

实现 grep_search 和 list_directory 两个内置工具，
提供工作区内的正则搜索和目录列表能力。
"""

from __future__ import annotations

import os
import re
from pathlib import Path

from pydantic import BaseModel, Field

from src.core.agent.tool import ToolDefinition, ToolResult


# ---------------------------------------------------------------------------
# Parameter models
# ---------------------------------------------------------------------------


class GrepSearchParams(BaseModel):
    """grep_search 工具参数."""

    pattern: str = Field(description="正则表达式搜索模式")
    path: str = Field(default=".", description="相对于工作区的搜索路径（默认为工作区根目录）")


class ListDirectoryParams(BaseModel):
    """list_directory 工具参数."""

    path: str = Field(default=".", description="相对于工作区的目录路径（默认为工作区根目录）")
    depth: int = Field(default=1, ge=1, le=3, description="列出目录的深度（默认 1，最大 3）")


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

_MAX_GREP_RESULTS = 50

# 搜索时跳过的目录名
_SKIP_DIRS = frozenset({
    ".git", ".hg", ".svn", "__pycache__", "node_modules",
    ".venv", "venv", ".tox", ".mypy_cache", ".pytest_cache",
    "dist", "build", ".hypothesis",
})

# 搜索时跳过的文件扩展名（二进制文件）
_SKIP_EXTENSIONS = frozenset({
    ".pyc", ".pyo", ".so", ".dll", ".exe", ".bin",
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico",
    ".zip", ".tar", ".gz", ".bz2", ".7z", ".rar",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx",
    ".woff", ".woff2", ".ttf", ".eot",
    ".mp3", ".mp4", ".avi", ".mov", ".wav",
})


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------


def _resolve_and_check(workspace_dir: Path, relative_path: str) -> Path | ToolResult:
    """解析相对路径并检查路径遍历.

    Args:
        workspace_dir: 工作区根目录（已 resolve）.
        relative_path: 用户提供的相对路径.

    Returns:
        解析后的绝对路径，或路径遍历时返回 ToolResult(is_error=True).
    """
    try:
        resolved = (workspace_dir / relative_path).resolve()
    except (OSError, ValueError) as exc:
        return ToolResult(
            output=f"Invalid path '{relative_path}': {exc}",
            is_error=True,
        )

    # 路径遍历检查：resolved 必须在 workspace_dir 内部
    try:
        resolved.relative_to(workspace_dir)
    except ValueError:
        return ToolResult(
            output=f"Path traversal violation: '{relative_path}' resolves outside workspace",
            is_error=True,
        )

    return resolved


def _should_skip_file(file_path: Path) -> bool:
    """判断文件是否应跳过搜索."""
    return file_path.suffix.lower() in _SKIP_EXTENSIONS


def _walk_files(root: Path, workspace_dir: Path) -> list[Path]:
    """递归遍历目录下的所有文件，跳过不需要搜索的目录和文件."""
    files: list[Path] = []
    try:
        for entry in os.scandir(root):
            if entry.is_dir(follow_symlinks=False):
                if entry.name in _SKIP_DIRS:
                    continue
                # 确保子目录仍在 workspace 内
                sub_path = Path(entry.path)
                try:
                    sub_path.resolve().relative_to(workspace_dir)
                except ValueError:
                    continue
                files.extend(_walk_files(sub_path, workspace_dir))
            elif entry.is_file(follow_symlinks=False):
                file_path = Path(entry.path)
                if not _should_skip_file(file_path):
                    files.append(file_path)
    except PermissionError:
        pass
    return files


# ---------------------------------------------------------------------------
# GrepSearchTool
# ---------------------------------------------------------------------------


class GrepSearchTool(ToolDefinition):
    """在工作区内搜索匹配正则表达式的文件内容.

    返回匹配行及其文件路径和行号，最多返回 50 个结果。
    """

    tool_id = "grep_search"
    description = "在工作区内搜索匹配正则表达式的文件内容，返回匹配行、文件路径和行号（最多 50 结果）"
    parameters_schema = GrepSearchParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行正则搜索."""
        assert isinstance(params, GrepSearchParams)

        # 验证正则表达式
        try:
            regex = re.compile(params.pattern)
        except re.error as exc:
            return ToolResult(
                output=f"Invalid regex pattern '{params.pattern}': {exc}",
                is_error=True,
            )

        # 解析搜索路径
        result = _resolve_and_check(self._workspace_dir, params.path)
        if isinstance(result, ToolResult):
            return result
        search_root = result

        if not search_root.exists():
            return ToolResult(
                output=f"Path not found: '{params.path}'",
                is_error=True,
            )

        # 收集要搜索的文件
        if search_root.is_file():
            files = [search_root]
        else:
            files = _walk_files(search_root, self._workspace_dir)

        # 搜索匹配
        matches: list[str] = []
        for file_path in files:
            if len(matches) >= _MAX_GREP_RESULTS:
                break

            try:
                content = file_path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue

            for line_num, line in enumerate(content.splitlines(), start=1):
                if regex.search(line):
                    # 使用相对路径显示
                    try:
                        rel_path = file_path.relative_to(self._workspace_dir)
                    except ValueError:
                        rel_path = file_path
                    matches.append(f"{rel_path}:{line_num}: {line}")
                    if len(matches) >= _MAX_GREP_RESULTS:
                        break

        if not matches:
            return ToolResult(
                output=f"No matches found for pattern '{params.pattern}'",
                metadata={"match_count": 0},
            )

        output = "\n".join(matches)
        return ToolResult(
            output=output,
            metadata={
                "match_count": len(matches),
                "truncated": len(matches) >= _MAX_GREP_RESULTS,
            },
        )


# ---------------------------------------------------------------------------
# ListDirectoryTool
# ---------------------------------------------------------------------------


class ListDirectoryTool(ToolDefinition):
    """列出工作区内指定目录的内容.

    支持 depth 参数控制递归深度（默认 1，最大 3）。
    """

    tool_id = "list_directory"
    description = "列出工作区内指定目录的文件和子目录（支持 depth 参数，默认 1，最大 3）"
    parameters_schema = ListDirectoryParams

    def __init__(self, workspace_dir: Path) -> None:
        self._workspace_dir = workspace_dir.resolve()

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行目录列表."""
        assert isinstance(params, ListDirectoryParams)

        # 解析目录路径
        result = _resolve_and_check(self._workspace_dir, params.path)
        if isinstance(result, ToolResult):
            return result
        target_dir = result

        if not target_dir.exists():
            return ToolResult(
                output=f"Directory not found: '{params.path}'",
                is_error=True,
            )

        if not target_dir.is_dir():
            return ToolResult(
                output=f"Path is not a directory: '{params.path}'",
                is_error=True,
            )

        # 递归列出目录内容
        entries = self._list_recursive(target_dir, params.depth, current_depth=0)

        if not entries:
            return ToolResult(
                output=f"Directory is empty: '{params.path}'",
                metadata={"entry_count": 0},
            )

        output = "\n".join(entries)
        return ToolResult(
            output=output,
            metadata={"entry_count": len(entries)},
        )

    def _list_recursive(
        self, directory: Path, max_depth: int, current_depth: int
    ) -> list[str]:
        """递归列出目录内容.

        Args:
            directory: 要列出的目录.
            max_depth: 最大递归深度.
            current_depth: 当前递归深度.

        Returns:
            格式化的目录条目列表.
        """
        entries: list[str] = []
        indent = "  " * current_depth

        try:
            items = sorted(directory.iterdir(), key=lambda p: (not p.is_dir(), p.name.lower()))
        except PermissionError:
            entries.append(f"{indent}[permission denied]")
            return entries

        for item in items:
            # 路径遍历检查
            try:
                item.resolve().relative_to(self._workspace_dir)
            except ValueError:
                continue

            try:
                rel_path = item.relative_to(self._workspace_dir)
            except ValueError:
                rel_path = item.name

            if item.is_dir():
                entries.append(f"{indent}{rel_path}/")
                if current_depth < max_depth - 1:
                    entries.extend(
                        self._list_recursive(item, max_depth, current_depth + 1)
                    )
            else:
                entries.append(f"{indent}{rel_path}")

        return entries
