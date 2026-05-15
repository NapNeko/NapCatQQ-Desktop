# -*- coding: utf-8 -*-
"""Bot 管理工具集.

实现 bot_status, bot_start, bot_stop, bot_restart, bot_logs 五个内置工具, 
提供 NapCat Bot 进程的状态查询, 启停控制和日志获取能力. 

工具通过 BotManagerInterface 协议与底层进程管理器交互, 
解耦具体实现 (如 BotProcessManager) , 便于测试和未来扩展. 
"""

from __future__ import annotations

from typing import Protocol, runtime_checkable

from pydantic import BaseModel, Field

from src.core.agent.tool import ToolDefinition, ToolResult


# ---------------------------------------------------------------------------
# BotManagerInterface Protocol
# ---------------------------------------------------------------------------


@runtime_checkable
class BotManagerInterface(Protocol):
    """Bot 进程管理器接口协议.

    定义 Agent 工具所需的最小接口, 解耦具体的 BotProcessManager 实现. 
    适配层负责将现有 BotProcessManager 的方法映射到此协议. 
    """

    def get_status(self) -> dict:
        """查询 Bot 进程状态.

        Returns:
            包含以下字段的字典:
            - state: "running" | "stopped" | "error"
            - uptime: int | None (运行秒数, 停止时为 None)
            - error: str | None (最近错误信息, 无错误时为 None)
        """
        ...

    async def start(self) -> None:
        """启动 Bot 进程.

        Raises:
            RuntimeError: 如果启动失败.
        """
        ...

    async def stop(self) -> None:
        """停止 Bot 进程.

        Raises:
            RuntimeError: 如果停止失败.
        """
        ...

    async def restart(self) -> None:
        """重启 Bot 进程 (先停止再启动) .

        Raises:
            RuntimeError: 如果重启失败.
        """
        ...

    def get_logs(self, lines: int = 50) -> list[str]:
        """获取最近 N 行日志.

        Args:
            lines: 要获取的日志行数, 默认 50.

        Returns:
            日志行列表, 最新的在最后.
        """
        ...


# ---------------------------------------------------------------------------
# Parameter models
# ---------------------------------------------------------------------------


class BotLogsParams(BaseModel):
    """bot_logs 工具参数."""

    lines: int = Field(
        default=50,
        ge=1,
        le=200,
        description="要获取的日志行数（1-200，默认 50）",
    )


class EmptyParams(BaseModel):
    """无参数工具的空参数模型."""

    pass


# ---------------------------------------------------------------------------
# BotStatusTool
# ---------------------------------------------------------------------------


class BotStatusTool(ToolDefinition):
    """查询 NapCat Bot 进程状态.

    返回进程运行状态, 运行时间和最近错误信息. 
    """

    tool_id = "bot_status"
    description = "查询 NapCat Bot 进程状态（running/stopped/error），返回运行时间和错误信息"
    parameters_schema = EmptyParams

    def __init__(self, bot_manager: BotManagerInterface) -> None:
        self._bot_manager = bot_manager

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行状态查询."""
        try:
            status = self._bot_manager.get_status()
        except Exception as exc:
            return ToolResult(
                output=f"查询 Bot 状态失败: {exc}",
                is_error=True,
            )

        state = status.get("state", "unknown")
        uptime = status.get("uptime")
        error = status.get("error")

        parts = [f"状态: {state}"]
        if uptime is not None:
            parts.append(f"运行时间: {uptime}秒")
        if error:
            parts.append(f"错误: {error}")

        return ToolResult(
            output="\n".join(parts),
            metadata=status,
        )


# ---------------------------------------------------------------------------
# BotStartTool
# ---------------------------------------------------------------------------


class BotStartTool(ToolDefinition):
    """启动 NapCat Bot 进程.

    使用现有的 QProcess 管理基础设施启动 Bot. 
    """

    tool_id = "bot_start"
    description = "启动 NapCat Bot 进程"
    parameters_schema = EmptyParams

    def __init__(self, bot_manager: BotManagerInterface) -> None:
        self._bot_manager = bot_manager

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行 Bot 启动."""
        # 先检查当前状态, 避免重复启动
        try:
            status = self._bot_manager.get_status()
            if status.get("state") == "running":
                return ToolResult(
                    output="Bot 已在运行中，无需重复启动",
                    is_error=False,
                )
        except Exception:
            pass  # 状态查询失败不阻止启动尝试

        try:
            await self._bot_manager.start()
        except Exception as exc:
            return ToolResult(
                output=f"启动 Bot 失败: {exc}",
                is_error=True,
            )

        return ToolResult(output="Bot 启动成功")


# ---------------------------------------------------------------------------
# BotStopTool
# ---------------------------------------------------------------------------


class BotStopTool(ToolDefinition):
    """停止 NapCat Bot 进程.

    使用 psutil 进程树清理机制停止 Bot. 
    """

    tool_id = "bot_stop"
    description = "停止正在运行的 NapCat Bot 进程"
    parameters_schema = EmptyParams

    def __init__(self, bot_manager: BotManagerInterface) -> None:
        self._bot_manager = bot_manager

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行 Bot 停止."""
        # 先检查当前状态
        try:
            status = self._bot_manager.get_status()
            if status.get("state") == "stopped":
                return ToolResult(
                    output="Bot 当前未运行",
                    is_error=False,
                )
        except Exception:
            pass  # 状态查询失败不阻止停止尝试

        try:
            await self._bot_manager.stop()
        except Exception as exc:
            return ToolResult(
                output=f"停止 Bot 失败: {exc}",
                is_error=True,
            )

        return ToolResult(output="Bot 已停止")


# ---------------------------------------------------------------------------
# BotRestartTool
# ---------------------------------------------------------------------------


class BotRestartTool(ToolDefinition):
    """重启 NapCat Bot 进程.

    先停止再启动 Bot 进程. 
    """

    tool_id = "bot_restart"
    description = "重启 NapCat Bot 进程（先停止再启动）"
    parameters_schema = EmptyParams

    def __init__(self, bot_manager: BotManagerInterface) -> None:
        self._bot_manager = bot_manager

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行 Bot 重启."""
        try:
            await self._bot_manager.restart()
        except Exception as exc:
            return ToolResult(
                output=f"重启 Bot 失败: {exc}",
                is_error=True,
            )

        return ToolResult(output="Bot 重启成功")


# ---------------------------------------------------------------------------
# BotLogsTool
# ---------------------------------------------------------------------------


class BotLogsTool(ToolDefinition):
    """获取 NapCat Bot 进程最近的日志输出.

    支持指定获取行数 (1-200, 默认 50) . 
    """

    tool_id = "bot_logs"
    description = "获取 NapCat Bot 进程最近 N 行日志（默认 50，最大 200）"
    parameters_schema = BotLogsParams

    def __init__(self, bot_manager: BotManagerInterface) -> None:
        self._bot_manager = bot_manager

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行日志获取."""
        assert isinstance(params, BotLogsParams)

        try:
            log_lines = self._bot_manager.get_logs(lines=params.lines)
        except Exception as exc:
            return ToolResult(
                output=f"获取 Bot 日志失败: {exc}",
                is_error=True,
            )

        if not log_lines:
            return ToolResult(
                output="（无日志输出）",
                metadata={"lines_returned": 0},
            )

        return ToolResult(
            output="\n".join(log_lines),
            metadata={"lines_returned": len(log_lines)},
        )
