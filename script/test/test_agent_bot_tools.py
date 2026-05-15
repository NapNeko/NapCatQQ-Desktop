# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tools/bot_tools.py.

验证 BotStatusTool, BotStartTool, BotStopTool, BotRestartTool, BotLogsTool
的核心行为, 使用 mock BotManagerInterface 实现. 
"""

from __future__ import annotations

# 标准库导入
import asyncio
from typing import Any

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.agent.tools.bot_tools import (
    BotLogsTool,
    BotManagerInterface,
    BotRestartTool,
    BotStartTool,
    BotStatusTool,
    BotStopTool,
    EmptyParams,
    BotLogsParams,
)


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# Mock BotManagerInterface
# ---------------------------------------------------------------------------


class MockBotManager:
    """测试用 BotManagerInterface 实现."""

    def __init__(
        self,
        state: str = "stopped",
        uptime: int | None = None,
        error: str | None = None,
        logs: list[str] | None = None,
        start_error: Exception | None = None,
        stop_error: Exception | None = None,
        restart_error: Exception | None = None,
        status_error: Exception | None = None,
        logs_error: Exception | None = None,
    ) -> None:
        self._state = state
        self._uptime = uptime
        self._error = error
        self._logs = logs or []
        self._start_error = start_error
        self._stop_error = stop_error
        self._restart_error = restart_error
        self._status_error = status_error
        self._logs_error = logs_error
        self.start_called = False
        self.stop_called = False
        self.restart_called = False

    def get_status(self) -> dict[str, Any]:
        if self._status_error:
            raise self._status_error
        return {
            "state": self._state,
            "uptime": self._uptime,
            "error": self._error,
        }

    async def start(self) -> None:
        self.start_called = True
        if self._start_error:
            raise self._start_error

    async def stop(self) -> None:
        self.stop_called = True
        if self._stop_error:
            raise self._stop_error

    async def restart(self) -> None:
        self.restart_called = True
        if self._restart_error:
            raise self._restart_error

    def get_logs(self, lines: int = 50) -> list[str]:
        if self._logs_error:
            raise self._logs_error
        return self._logs[-lines:]


# ---------------------------------------------------------------------------
# Protocol conformance
# ---------------------------------------------------------------------------


class TestBotManagerInterfaceProtocol:
    """验证 MockBotManager 符合 BotManagerInterface 协议."""

    def test_mock_is_protocol_instance(self) -> None:
        manager = MockBotManager()
        assert isinstance(manager, BotManagerInterface)


# ---------------------------------------------------------------------------
# BotStatusTool
# ---------------------------------------------------------------------------


class TestBotStatusTool:
    """BotStatusTool 测试."""

    def test_tool_id(self) -> None:
        manager = MockBotManager()
        tool = BotStatusTool(bot_manager=manager)
        assert tool.tool_id == "bot_status"

    def test_status_running(self) -> None:
        manager = MockBotManager(state="running", uptime=120)
        tool = BotStatusTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "running" in result.output
        assert "120" in result.output
        assert result.metadata is not None
        assert result.metadata["state"] == "running"

    def test_status_stopped(self) -> None:
        manager = MockBotManager(state="stopped")
        tool = BotStatusTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "stopped" in result.output

    def test_status_error_state(self) -> None:
        manager = MockBotManager(state="error", error="进程崩溃")
        tool = BotStatusTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "error" in result.output
        assert "进程崩溃" in result.output

    def test_status_query_exception(self) -> None:
        manager = MockBotManager(status_error=RuntimeError("连接失败"))
        tool = BotStatusTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert result.is_error
        assert "查询 Bot 状态失败" in result.output


# ---------------------------------------------------------------------------
# BotStartTool
# ---------------------------------------------------------------------------


class TestBotStartTool:
    """BotStartTool 测试."""

    def test_tool_id(self) -> None:
        manager = MockBotManager()
        tool = BotStartTool(bot_manager=manager)
        assert tool.tool_id == "bot_start"

    def test_start_success(self) -> None:
        manager = MockBotManager(state="stopped")
        tool = BotStartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "启动成功" in result.output
        assert manager.start_called

    def test_start_already_running(self) -> None:
        manager = MockBotManager(state="running", uptime=60)
        tool = BotStartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "已在运行" in result.output
        assert not manager.start_called

    def test_start_failure(self) -> None:
        manager = MockBotManager(
            state="stopped",
            start_error=RuntimeError("端口被占用"),
        )
        tool = BotStartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert result.is_error
        assert "启动 Bot 失败" in result.output
        assert "端口被占用" in result.output

    def test_start_status_check_fails_still_attempts(self) -> None:
        """状态查询失败时仍尝试启动."""
        manager = MockBotManager(
            status_error=RuntimeError("状态不可用"),
        )
        tool = BotStartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert manager.start_called


# ---------------------------------------------------------------------------
# BotStopTool
# ---------------------------------------------------------------------------


class TestBotStopTool:
    """BotStopTool 测试."""

    def test_tool_id(self) -> None:
        manager = MockBotManager()
        tool = BotStopTool(bot_manager=manager)
        assert tool.tool_id == "bot_stop"

    def test_stop_success(self) -> None:
        manager = MockBotManager(state="running", uptime=300)
        tool = BotStopTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "已停止" in result.output
        assert manager.stop_called

    def test_stop_already_stopped(self) -> None:
        manager = MockBotManager(state="stopped")
        tool = BotStopTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "未运行" in result.output
        assert not manager.stop_called

    def test_stop_failure(self) -> None:
        manager = MockBotManager(
            state="running",
            stop_error=RuntimeError("进程无响应"),
        )
        tool = BotStopTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert result.is_error
        assert "停止 Bot 失败" in result.output

    def test_stop_status_check_fails_still_attempts(self) -> None:
        """状态查询失败时仍尝试停止."""
        manager = MockBotManager(
            status_error=RuntimeError("状态不可用"),
        )
        tool = BotStopTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert manager.stop_called


# ---------------------------------------------------------------------------
# BotRestartTool
# ---------------------------------------------------------------------------


class TestBotRestartTool:
    """BotRestartTool 测试."""

    def test_tool_id(self) -> None:
        manager = MockBotManager()
        tool = BotRestartTool(bot_manager=manager)
        assert tool.tool_id == "bot_restart"

    def test_restart_success(self) -> None:
        manager = MockBotManager(state="running")
        tool = BotRestartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert not result.is_error
        assert "重启成功" in result.output
        assert manager.restart_called

    def test_restart_failure(self) -> None:
        manager = MockBotManager(
            restart_error=RuntimeError("重启超时"),
        )
        tool = BotRestartTool(bot_manager=manager)
        result = _run(tool.execute(EmptyParams()))
        assert result.is_error
        assert "重启 Bot 失败" in result.output
        assert "重启超时" in result.output


# ---------------------------------------------------------------------------
# BotLogsTool
# ---------------------------------------------------------------------------


class TestBotLogsTool:
    """BotLogsTool 测试."""

    def test_tool_id(self) -> None:
        manager = MockBotManager()
        tool = BotLogsTool(bot_manager=manager)
        assert tool.tool_id == "bot_logs"

    def test_logs_default_lines(self) -> None:
        logs = [f"[INFO] line {i}" for i in range(100)]
        manager = MockBotManager(logs=logs)
        tool = BotLogsTool(bot_manager=manager)
        result = _run(tool.execute(BotLogsParams()))
        assert not result.is_error
        # 默认 50 行
        lines_in_output = result.output.split("\n")
        assert len(lines_in_output) == 50
        assert result.metadata is not None
        assert result.metadata["lines_returned"] == 50

    def test_logs_custom_lines(self) -> None:
        logs = [f"[INFO] line {i}" for i in range(100)]
        manager = MockBotManager(logs=logs)
        tool = BotLogsTool(bot_manager=manager)
        result = _run(tool.execute(BotLogsParams(lines=10)))
        assert not result.is_error
        lines_in_output = result.output.split("\n")
        assert len(lines_in_output) == 10

    def test_logs_empty(self) -> None:
        manager = MockBotManager(logs=[])
        tool = BotLogsTool(bot_manager=manager)
        result = _run(tool.execute(BotLogsParams()))
        assert not result.is_error
        assert "无日志输出" in result.output
        assert result.metadata is not None
        assert result.metadata["lines_returned"] == 0

    def test_logs_failure(self) -> None:
        manager = MockBotManager(logs_error=RuntimeError("日志文件不可读"))
        tool = BotLogsTool(bot_manager=manager)
        result = _run(tool.execute(BotLogsParams()))
        assert result.is_error
        assert "获取 Bot 日志失败" in result.output

    def test_logs_max_200(self) -> None:
        """验证最大 200 行限制通过 pydantic 验证."""
        logs = [f"[INFO] line {i}" for i in range(300)]
        manager = MockBotManager(logs=logs)
        tool = BotLogsTool(bot_manager=manager)
        result = _run(tool.execute(BotLogsParams(lines=200)))
        assert not result.is_error
        lines_in_output = result.output.split("\n")
        assert len(lines_in_output) == 200


class TestBotLogsParamsValidation:
    """BotLogsParams 参数验证测试."""

    def test_default_value(self) -> None:
        params = BotLogsParams()
        assert params.lines == 50

    def test_valid_range(self) -> None:
        params = BotLogsParams(lines=1)
        assert params.lines == 1
        params = BotLogsParams(lines=200)
        assert params.lines == 200

    def test_below_minimum(self) -> None:
        from pydantic import ValidationError as PydanticValidationError

        with pytest.raises(PydanticValidationError):
            BotLogsParams(lines=0)

    def test_above_maximum(self) -> None:
        from pydantic import ValidationError as PydanticValidationError

        with pytest.raises(PydanticValidationError):
            BotLogsParams(lines=201)
