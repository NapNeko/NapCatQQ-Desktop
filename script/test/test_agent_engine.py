# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/engine.py.

测试 AgentEngine 核心引擎的关键行为: 
1. 工具循环达到 25 次上限时正确停止
2. StreamError 事件保留已有消息
3. Subagent 嵌套深度限制 (最大 3 层) 

Requirements: 5.4, 5.5, 5.6, 5.7, 6.7, 6.9
"""

from __future__ import annotations

import asyncio
import json
import tempfile
from pathlib import Path
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

import pytest
from pydantic import BaseModel

from src.core.agent.agent_def import AgentDefinition
from src.core.agent.engine import (
    MAX_SUBAGENT_NESTING_DEPTH,
    MAX_TOOL_LOOP_ITERATIONS,
    AgentEngine,
)
from src.core.agent.permission import PermissionRule
from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry
from src.core.agent.session import SessionManager
from src.core.agent.stream import (
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    TextDelta,
    ToolCallComplete,
)
from src.core.agent.tool import ToolDefinition, ToolRegistry, ToolResult


# ============================================================
# 测试辅助类
# ============================================================


class _EchoParams(BaseModel):
    """测试用参数模型."""

    message: str = "hello"


class _EchoTool(ToolDefinition):
    """测试用工具: 回显消息."""

    tool_id = "echo_tool"
    description = "Echo the message"
    parameters_schema = _EchoParams

    async def execute(self, params: BaseModel) -> ToolResult:
        p: _EchoParams = params  # type: ignore[assignment]
        return ToolResult(output=f"echo: {p.message}")


class _CounterTool(ToolDefinition):
    """测试用工具: 计数器, 每次调用递增."""

    tool_id = "counter_tool"
    description = "Increment counter"
    parameters_schema = _EchoParams

    def __init__(self) -> None:
        self.call_count = 0

    async def execute(self, params: BaseModel) -> ToolResult:
        self.call_count += 1
        return ToolResult(output=f"count: {self.call_count}")


def _create_provider_registry() -> ProviderRegistry:
    """创建一个已注册 Provider 并设置活跃模型的 ProviderRegistry."""
    registry = ProviderRegistry()
    provider = Provider(
        provider_id="test_provider",
        name="Test Provider",
        api_base_url="https://api.test.com/v1",
        api_key_ref="test_api_key",
        models=[
            ModelEntry(
                model_id="test_model",
                display_name="Test Model",
                max_tokens=4096,
                supports_streaming=True,
                supports_tools=True,
            )
        ],
    )
    registry.register(provider)
    registry.set_active("test_provider", "test_model")
    return registry


def _create_tool_registry() -> ToolRegistry:
    """创建一个包含测试工具的 ToolRegistry."""
    registry = ToolRegistry()
    registry.register(_EchoTool())
    registry.register(_CounterTool())
    return registry


def _create_session_manager() -> SessionManager:
    """创建一个使用临时目录的 SessionManager."""
    tmp_dir = tempfile.mkdtemp()
    return SessionManager(storage_dir=Path(tmp_dir))


def _make_sse_response_with_tool_call(
    tool_call_id: str = "call_001",
    function_name: str = "echo_tool",
    arguments: str = '{"message": "hello"}',
) -> str:
    """构造一个包含工具调用的 SSE 响应."""
    # 第一个 chunk: tool_call start
    chunk1 = json.dumps(
        {
            "choices": [
                {
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": tool_call_id,
                                "type": "function",
                                "function": {
                                    "name": function_name,
                                    "arguments": "",
                                },
                            }
                        ]
                    },
                    "finish_reason": None,
                }
            ]
        }
    )
    # 第二个 chunk: tool_call arguments
    chunk2 = json.dumps(
        {
            "choices": [
                {
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "function": {"arguments": arguments},
                            }
                        ]
                    },
                    "finish_reason": None,
                }
            ]
        }
    )
    # 第三个 chunk: finish_reason = tool_calls
    chunk3 = json.dumps(
        {
            "choices": [
                {
                    "delta": {},
                    "finish_reason": "tool_calls",
                }
            ]
        }
    )
    return f"data: {chunk1}\n\ndata: {chunk2}\n\ndata: {chunk3}\n\n"


def _make_sse_response_text_and_stop(text: str = "Hello!") -> str:
    """构造一个包含文本响应并正常结束的 SSE 响应."""
    chunk1 = json.dumps(
        {
            "choices": [
                {
                    "delta": {"content": text},
                    "finish_reason": None,
                }
            ]
        }
    )
    chunk2 = json.dumps(
        {
            "choices": [
                {
                    "delta": {},
                    "finish_reason": "stop",
                }
            ]
        }
    )
    return f"data: {chunk1}\n\ndata: {chunk2}\n\n"


def _make_sse_response_text_partial(text: str = "Partial") -> str:
    """构造一个只有部分文本 (没有结束信号) 的 SSE 响应."""
    chunk1 = json.dumps(
        {
            "choices": [
                {
                    "delta": {"content": text},
                    "finish_reason": None,
                }
            ]
        }
    )
    return f"data: {chunk1}\n\n"


# ============================================================
# Test 1: 工具循环达到 25 次上限时正确停止
# ============================================================


class TestToolLoopMaxIterations:
    """测试工具循环达到 25 次上限时正确停止.

    Requirements: 5.4, 5.5
    """

    def test_max_iterations_constant(self) -> None:
        """验证 MAX_TOOL_LOOP_ITERATIONS 常量为 25."""
        assert MAX_TOOL_LOOP_ITERATIONS == 25

    def test_tool_loop_reaches_max_emits_stream_end(self) -> None:
        """工具循环达到 25 次上限时, 应发射 StreamEnd(reason='max_iterations')
        并追加一条 assistant 消息说明达到迭代上限."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        # 创建会话
        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        # 收集事件
        events: list[StreamEvent] = []

        def on_event(event: StreamEvent) -> None:
            events.append(event)

        # 记录 _call_llm_streaming 被调用的次数
        call_count = 0

        async def mock_call_llm_streaming(
            session_id,
            messages_payload,
            tool_definitions,
            agent,
            on_event,
            nesting_depth,
            adapter,
            provider,
            model_config,
        ):
            """模拟 LLM 每次都返回一个工具调用, 使循环持续."""
            nonlocal call_count
            call_count += 1

            # 每次都返回一个 ToolCallComplete, 使循环继续
            tc = ToolCallComplete(
                tool_call_id=f"call_{call_count:03d}",
                function_name="echo_tool",
                arguments='{"message": "loop"}',
            )

            # 模拟追加 assistant 消息 (engine 正常流程中会做这个) 
            from datetime import datetime, timezone

            from src.core.agent.session import Message, ToolCallInfo

            assistant_msg = Message(
                id=uuid4(),
                role="assistant",
                content=f"调用工具 echo_tool (iteration {call_count})",
                timestamp=datetime.now(timezone.utc),
                tool_calls=[
                    ToolCallInfo(
                        id=tc.tool_call_id,
                        function_name=tc.function_name,
                        arguments=tc.arguments,
                    )
                ],
            )
            session_manager.append_message(session_id, assistant_msg)

            return [tc]

        # Patch _call_llm_streaming 方法
        with patch.object(
            engine, "_call_llm_streaming", side_effect=mock_call_llm_streaming
        ):
            asyncio.run(
                engine.submit(
                    session_id=session_id,
                    user_message="trigger loop",
                    on_event=on_event,
                )
            )

        # 验证循环执行了 25 次
        assert call_count == MAX_TOOL_LOOP_ITERATIONS

        # 验证发射了 StreamEnd(reason="max_iterations")
        stream_end_events = [
            e for e in events if isinstance(e, StreamEnd) and e.reason == "max_iterations"
        ]
        assert len(stream_end_events) == 1

        # 验证追加了 assistant 消息说明达到迭代上限
        final_session = session_manager.get(session_id)
        last_msg = final_session.messages[-1]
        assert last_msg.role == "assistant"
        assert "25" in last_msg.content or "迭代" in last_msg.content


# ============================================================
# Test 2: StreamError 事件保留已有消息
# ============================================================


class TestStreamErrorPreservesMessages:
    """测试 StreamError 事件保留已累积的消息.

    Requirements: 5.6, 5.7
    """

    def test_http_error_preserves_accumulated_messages(self) -> None:
        """当 LLM API 返回 HTTP 错误时, 已累积的消息应被保留."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        events: list[StreamEvent] = []

        def on_event(event: StreamEvent) -> None:
            events.append(event)

        # 模拟 _call_llm_streaming: 先产生部分文本, 然后发射错误
        async def mock_call_llm_streaming_with_error(
            session_id,
            messages_payload,
            tool_definitions,
            agent,
            on_event,
            nesting_depth,
            adapter,
            provider,
            model_config,
        ):
            """模拟: 先产生部分文本内容, 然后遇到错误."""
            from datetime import datetime, timezone

            from src.core.agent.session import Message

            # 模拟已经累积了部分 assistant 消息
            partial_msg = Message(
                id=uuid4(),
                role="assistant",
                content="这是部分响应内容",
                timestamp=datetime.now(timezone.utc),
            )
            session_manager.append_message(session_id, partial_msg)

            # 发射错误事件
            error_event = StreamErrorEvent(
                status_code=500,
                message="LLM API 返回错误: Internal Server Error",
            )
            if on_event:
                on_event(error_event)

            # 返回空列表表示流结束 (有错误) 
            return []

        with patch.object(
            engine, "_call_llm_streaming", side_effect=mock_call_llm_streaming_with_error
        ):
            asyncio.run(
                engine.submit(
                    session_id=session_id,
                    user_message="test error",
                    on_event=on_event,
                )
            )

        # 验证发射了 StreamErrorEvent
        error_events = [e for e in events if isinstance(e, StreamErrorEvent)]
        assert len(error_events) == 1
        assert error_events[0].status_code == 500

        # 验证会话中保留了消息 (user + partial assistant) 
        final_session = session_manager.get(session_id)
        assert len(final_session.messages) >= 2  # user msg + partial assistant msg
        # 找到 assistant 消息
        assistant_msgs = [m for m in final_session.messages if m.role == "assistant"]
        assert len(assistant_msgs) >= 1
        assert "部分响应内容" in assistant_msgs[0].content

    def test_timeout_error_preserves_accumulated_messages(self) -> None:
        """当 httpx 连接超时时, 已累积的消息应被保留."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        events: list[StreamEvent] = []

        def on_event(event: StreamEvent) -> None:
            events.append(event)

        async def mock_call_llm_streaming_timeout(
            session_id,
            messages_payload,
            tool_definitions,
            agent,
            on_event,
            nesting_depth,
            adapter,
            provider,
            model_config,
        ):
            """模拟: 先产生部分内容, 然后超时."""
            from datetime import datetime, timezone

            from src.core.agent.session import Message

            # 模拟已经累积了部分 assistant 消息
            partial_msg = Message(
                id=uuid4(),
                role="assistant",
                content="超时前的部分内容",
                timestamp=datetime.now(timezone.utc),
            )
            session_manager.append_message(session_id, partial_msg)

            # 发射超时错误事件
            error_event = StreamErrorEvent(
                status_code=None,
                message="连接超时（30秒无活动）",
            )
            if on_event:
                on_event(error_event)

            return []

        with patch.object(
            engine, "_call_llm_streaming", side_effect=mock_call_llm_streaming_timeout
        ):
            asyncio.run(
                engine.submit(
                    session_id=session_id,
                    user_message="test timeout",
                    on_event=on_event,
                )
            )

        # 验证发射了 StreamErrorEvent (无 status_code) 
        error_events = [e for e in events if isinstance(e, StreamErrorEvent)]
        assert len(error_events) == 1
        assert error_events[0].status_code is None

        # 验证会话中保留了消息
        final_session = session_manager.get(session_id)
        assistant_msgs = [m for m in final_session.messages if m.role == "assistant"]
        assert len(assistant_msgs) >= 1
        assert "超时前的部分内容" in assistant_msgs[0].content

    def test_persist_partial_response_on_error(self) -> None:
        """测试 _persist_partial_response 方法在错误时保留部分响应."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        # 直接调用 _persist_partial_response
        content_parts = ["Hello", " World", "!"]
        tool_calls: list[ToolCallComplete] = []

        engine._persist_partial_response(session_id, content_parts, tool_calls)

        # 验证消息被保存
        final_session = session_manager.get(session_id)
        assert len(final_session.messages) == 1
        assert final_session.messages[0].content == "Hello World!"
        assert final_session.messages[0].role == "assistant"

    def test_persist_partial_response_with_tool_calls(self) -> None:
        """测试 _persist_partial_response 保留工具调用信息."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        content_parts = ["部分"]
        tool_calls = [
            ToolCallComplete(
                tool_call_id="call_001",
                function_name="echo_tool",
                arguments='{"message": "test"}',
            )
        ]

        engine._persist_partial_response(session_id, content_parts, tool_calls)

        final_session = session_manager.get(session_id)
        assert len(final_session.messages) == 1
        msg = final_session.messages[0]
        assert msg.content == "部分"
        assert msg.tool_calls is not None
        assert len(msg.tool_calls) == 1
        assert msg.tool_calls[0].function_name == "echo_tool"

    def test_persist_partial_response_empty_content_uses_placeholder(self) -> None:
        """测试 _persist_partial_response 空内容时使用占位符."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        # 空内容但有工具调用
        content_parts: list[str] = []
        tool_calls = [
            ToolCallComplete(
                tool_call_id="call_001",
                function_name="echo_tool",
                arguments='{"message": "test"}',
            )
        ]

        engine._persist_partial_response(session_id, content_parts, tool_calls)

        final_session = session_manager.get(session_id)
        assert len(final_session.messages) == 1
        assert final_session.messages[0].content == "[响应中断]"


# ============================================================
# Test 3: Subagent 嵌套深度限制 (最大 3 层) 
# ============================================================


class TestSubagentNestingDepthLimit:
    """测试 subagent 嵌套深度限制 (最大 3 层) .

    Requirements: 6.7, 6.9
    """

    def test_max_nesting_depth_constant(self) -> None:
        """验证 MAX_SUBAGENT_NESTING_DEPTH 常量为 3."""
        assert MAX_SUBAGENT_NESTING_DEPTH == 3

    def test_subagent_nesting_at_max_depth_returns_error(self) -> None:
        """当嵌套深度达到最大值时, _execute_subagent 应返回 is_error=True."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        # 注册一个 subagent
        subagent = AgentDefinition(
            name="test_subagent",
            description="Test subagent",
            mode="subagent",
            system_prompt="You are a test subagent.",
            tool_ids=["echo_tool"],
            permission_rules=[
                PermissionRule(pattern="*", target="*", action="allow"),
            ],
        )
        engine.register_agent(subagent)

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        # 调用 _execute_subagent, nesting_depth 已经达到最大值
        result = asyncio.run(
            engine._execute_subagent(
                session_id=session_id,
                subagent_name="test_subagent",
                arguments='{"message": "test"}',
                on_event=None,
                nesting_depth=MAX_SUBAGENT_NESTING_DEPTH,  # 已达到最大深度
            )
        )

        assert result.is_error is True
        assert "嵌套深度" in result.output or str(MAX_SUBAGENT_NESTING_DEPTH) in result.output

    def test_subagent_nesting_below_max_depth_proceeds(self) -> None:
        """当嵌套深度低于最大值时, _execute_subagent 应正常执行."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        # 注册一个 subagent
        subagent = AgentDefinition(
            name="test_subagent",
            description="Test subagent",
            mode="subagent",
            system_prompt="You are a test subagent.",
            tool_ids=["echo_tool"],
            permission_rules=[
                PermissionRule(pattern="*", target="*", action="allow"),
            ],
        )
        engine.register_agent(subagent)

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        # Mock submit 方法以避免实际 LLM 调用
        # 当 submit 被递归调用时, 模拟子 Agent 产生一条 assistant 消息
        original_submit = engine.submit

        async def mock_submit(session_id, user_message, on_event=None, _nesting_depth=0):
            """模拟子 Agent 执行: 直接追加一条 assistant 消息."""
            from datetime import datetime, timezone

            from src.core.agent.session import Message

            assistant_msg = Message(
                id=uuid4(),
                role="assistant",
                content="子 Agent 响应内容",
                timestamp=datetime.now(timezone.utc),
            )
            session_manager.append_message(session_id, assistant_msg)

        with patch.object(engine, "submit", side_effect=mock_submit):
            result = asyncio.run(
                engine._execute_subagent(
                    session_id=session_id,
                    subagent_name="test_subagent",
                    arguments='{"message": "test"}',
                    on_event=None,
                    nesting_depth=1,  # 低于最大深度
                )
            )

        # 应该成功执行
        assert result.is_error is False
        assert "子 Agent 响应内容" in result.output

    def test_subagent_nesting_at_depth_2_proceeds(self) -> None:
        """当嵌套深度为 2 (低于最大值 3) 时, 应正常执行."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        subagent = AgentDefinition(
            name="test_subagent",
            description="Test subagent",
            mode="subagent",
            system_prompt="You are a test subagent.",
            tool_ids=["echo_tool"],
            permission_rules=[
                PermissionRule(pattern="*", target="*", action="allow"),
            ],
        )
        engine.register_agent(subagent)

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        async def mock_submit(session_id, user_message, on_event=None, _nesting_depth=0):
            from datetime import datetime, timezone

            from src.core.agent.session import Message

            assistant_msg = Message(
                id=uuid4(),
                role="assistant",
                content="depth 2 response",
                timestamp=datetime.now(timezone.utc),
            )
            session_manager.append_message(session_id, assistant_msg)

        with patch.object(engine, "submit", side_effect=mock_submit):
            result = asyncio.run(
                engine._execute_subagent(
                    session_id=session_id,
                    subagent_name="test_subagent",
                    arguments='{"message": "test"}',
                    on_event=None,
                    nesting_depth=2,  # depth 2, max is 3
                )
            )

        assert result.is_error is False
        assert "depth 2 response" in result.output

    def test_subagent_not_found_returns_error(self) -> None:
        """当子 Agent 名称不存在时, 应返回 is_error=True."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        result = asyncio.run(
            engine._execute_subagent(
                session_id=session_id,
                subagent_name="nonexistent_agent",
                arguments='{"message": "test"}',
                on_event=None,
                nesting_depth=0,
            )
        )

        assert result.is_error is True
        assert "nonexistent_agent" in result.output

    def test_subagent_execution_failure_returns_error(self) -> None:
        """当子 Agent 执行过程中抛出异常时, 应返回 is_error=True."""
        provider_registry = _create_provider_registry()
        tool_registry = _create_tool_registry()
        session_manager = _create_session_manager()

        engine = AgentEngine(
            provider_registry=provider_registry,
            tool_registry=tool_registry,
            session_manager=session_manager,
        )

        subagent = AgentDefinition(
            name="failing_subagent",
            description="Failing subagent",
            mode="subagent",
            system_prompt="",
            tool_ids=[],
            permission_rules=[
                PermissionRule(pattern="*", target="*", action="allow"),
            ],
        )
        engine.register_agent(subagent)

        session = session_manager.create("napcat-plugin-dev")
        session_id = session.session_id

        async def mock_submit_raises(session_id, user_message, on_event=None, _nesting_depth=0):
            raise RuntimeError("Subagent crashed")

        with patch.object(engine, "submit", side_effect=mock_submit_raises):
            result = asyncio.run(
                engine._execute_subagent(
                    session_id=session_id,
                    subagent_name="failing_subagent",
                    arguments='{"message": "test"}',
                    on_event=None,
                    nesting_depth=0,
                )
            )

        assert result.is_error is True
        assert "RuntimeError" in result.output or "执行失败" in result.output
