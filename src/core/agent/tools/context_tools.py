# -*- coding: utf-8 -*-
"""用户自定义上下文工具.

实现 open_context_file 工具，用于打开用户上下文文件进行编辑。
文件不存在时自动创建包含模板内容的文件。
"""

from __future__ import annotations

import platform
import subprocess
from pathlib import Path

from pydantic import BaseModel

from src.core.agent.tool import ToolDefinition, ToolResult

# ---------------------------------------------------------------------------
# Template content
# ---------------------------------------------------------------------------

_TEMPLATE_CONTENT = """\
# Agent 自定义上下文

在此文件中添加你的项目特定知识，Agent 会在对话中参考这些内容。

## 示例

- 项目使用的框架和版本
- 自定义 API 接口说明
- 编码规范和约定
"""


# ---------------------------------------------------------------------------
# Parameter model
# ---------------------------------------------------------------------------


class OpenContextFileParams(BaseModel):
    """open_context_file 工具参数.

    该工具无需参数，使用空模型。
    """

    pass


# ---------------------------------------------------------------------------
# OpenContextFileTool
# ---------------------------------------------------------------------------


class OpenContextFileTool(ToolDefinition):
    """打开用户自定义上下文文件.

    如果文件不存在，先创建包含模板头的文件，然后使用系统默认编辑器打开。
    支持 Windows、macOS 和 Linux 平台。
    """

    tool_id = "open_context_file"
    description = "打开用户自定义上下文文件（agent_context.md），不存在时自动创建模板"
    parameters_schema = OpenContextFileParams

    def __init__(self, context_file_path: Path) -> None:
        self._context_file_path = context_file_path

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行打开上下文文件操作."""
        file_path = self._context_file_path
        created = False

        # 文件不存在时创建模板
        if not file_path.exists():
            try:
                file_path.parent.mkdir(parents=True, exist_ok=True)
                file_path.write_text(_TEMPLATE_CONTENT, encoding="utf-8")
                created = True
            except OSError as exc:
                return ToolResult(
                    output=f"无法创建上下文文件 '{file_path}': {exc}",
                    is_error=True,
                )

        # 使用系统默认编辑器打开文件
        try:
            _open_with_default_editor(file_path)
        except OSError as exc:
            return ToolResult(
                output=f"无法打开文件 '{file_path}': {exc}",
                is_error=True,
            )

        if created:
            return ToolResult(
                output=f"已创建上下文文件并打开: {file_path}",
                metadata={"created": True, "path": str(file_path)},
            )

        return ToolResult(
            output=f"已打开上下文文件: {file_path}",
            metadata={"created": False, "path": str(file_path)},
        )


def _open_with_default_editor(file_path: Path) -> None:
    """使用系统默认编辑器打开文件.

    根据平台选择合适的打开方式：
    - Windows: os.startfile 或 start 命令
    - macOS: open 命令
    - Linux: xdg-open 命令

    Args:
        file_path: 要打开的文件路径.

    Raises:
        OSError: 如果无法打开文件.
    """
    system = platform.system()

    if system == "Windows":
        # Windows: 使用 os.startfile
        import os

        os.startfile(str(file_path))  # noqa: S606
    elif system == "Darwin":
        # macOS: 使用 open 命令
        subprocess.Popen(  # noqa: S603
            ["open", str(file_path)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    else:
        # Linux 及其他: 使用 xdg-open
        subprocess.Popen(  # noqa: S603
            ["xdg-open", str(file_path)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
