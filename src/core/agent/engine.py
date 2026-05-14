# -*- coding: utf-8 -*-
"""AgentEngine 核心调度引擎.

实现 Agent 能力框架的核心引擎，负责协调 Provider、Tool、Session 等子模块
完成 LLM 流式交互、工具循环执行和多 Agent 调度。

通过 AdapterRegistry 和 ProtocolAdapter 抽象层支持多 LLM 提供商的原生协议通信，
包括 OpenAI、Anthropic、Google Gemini 和 Azure OpenAI。

Requirements: 5.1, 5.3, 5.4, 5.5, 5.6, 5.7, 5.8, 6.2, 6.6, 6.7, 6.8, 6.9, 6.10, 7.4, 7.5, 7.6
Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 2.2, 2.6
"""

from __future__ import annotations

import json
import logging
from datetime import datetime, timezone
from typing import AsyncIterator, Callable
from uuid import UUID, uuid4

import httpx

from src.core.agent.adapters import (
    AnthropicAdapter,
    AzureAdapter,
    GeminiAdapter,
    OpenAIAdapter,
)
from src.core.agent.agent_def import AgentDefinition
from src.core.agent.content_safety import (
    get_content_safety_prompt,
    get_napcat_plugin_dev_prompt,
)
from src.core.agent.context_loader import ContextLoader
from src.core.agent.errors import AgentError, PermissionDeniedError
from src.core.agent.permission import PermissionRule, evaluate_permission
from src.core.agent.protocol import AdapterRegistry, ProtocolAdapter
from src.core.agent.provider import ModelConfig, Provider, ProviderRegistry
from src.core.agent.session import Message, SessionManager, ToolCallInfo
from src.core.agent.stream import (
    PermissionAskEvent,
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    TextDelta,
    ToolCallComplete,
)
from src.core.agent.tool import ToolRegistry, ToolResult

logger = logging.getLogger(__name__)

#: 工具循环最大迭代次数
MAX_TOOL_LOOP_ITERATIONS: int = 25

#: 子 Agent 最大嵌套深度
MAX_SUBAGENT_NESTING_DEPTH: int = 3

#: httpx 不活动超时时间（秒）
INACTIVITY_TIMEOUT_SECONDS: float = 30.0


class AgentEngine:
    """Agent 核心引擎，协调 Provider、Tool、Session 完成 LLM 交互.

    负责：
    - 构建请求 payload（system_prompt + message_history + tool_definitions）
    - 通过 httpx async streaming 调用 LLM API
    - 工具循环：ToolCallComplete → Permission check → Tool execute → ToolResult → 重新提交
    - 多 Agent 管理：get_agent, list_agents, select_agent
    - System prompt 组装：content_safety + napcat_plugin_dev + user_context

    Args:
        provider_registry: Provider 注册表实例.
        tool_registry: Tool 注册表实例.
        session_manager: Session 管理器实例.
        context_loader: 用户上下文加载器实例（可选）.
    """

    def __init__(
        self,
        provider_registry: ProviderRegistry,
        tool_registry: ToolRegistry,
        session_manager: SessionManager,
        context_loader: ContextLoader | None = None,
    ) -> None:
        self._provider_registry = provider_registry
        self._tool_registry = tool_registry
        self._session_manager = session_manager
        self._context_loader = context_loader
        self._agents: dict[str, AgentDefinition] = {}

        # 初始化协议适配器注册表，注册所有内置适配器
        self._adapter_registry = AdapterRegistry()
        self._adapter_registry.register("openai", OpenAIAdapter())
        self._adapter_registry.register("anthropic", AnthropicAdapter())
        self._adapter_registry.register("gemini", GeminiAdapter())
        self._adapter_registry.register("azure", AzureAdapter())

        # 注册默认 Agent
        self._register_default_agent()

    def _register_default_agent(self) -> None:
        """注册默认的 napcat-plugin-dev Agent."""
        default_agent = AgentDefinition(
            name="napcat-plugin-dev",
            description="NapCat 插件开发助手",
            mode="primary",
            system_prompt="",  # system prompt 在运行时动态组装
            tool_ids=[
                "file_read",
                "file_write",
                "file_edit",
                "grep_search",
                "list_directory",
                "shell_exec",
                "bot_status",
                "bot_start",
                "bot_stop",
                "bot_restart",
                "bot_logs",
                "webui_list_plugins",
                "webui_reload_plugin",
                "webui_plugin_config",
                "webui_bot_info",
                "webui_send_test_message",
                "open_context_file",
            ],
            permission_rules=[
                PermissionRule(pattern="*", target="*", action="allow"),
            ],
        )
        self._agents[default_agent.name] = default_agent

    def get_agent(self, name: str) -> AgentDefinition:
        """获取指定名称的 Agent 定义.

        Args:
            name: Agent 名称.

        Returns:
            对应的 AgentDefinition 实例.

        Raises:
            KeyError: 如果 Agent 名称不存在.
        """
        if name not in self._agents:
            raise KeyError(f"Agent '{name}' not found.")
        return self._agents[name]

    def list_agents(self) -> list[AgentDefinition]:
        """列出所有已注册的 Agent 定义.

        Returns:
            所有 AgentDefinition 实例的列表.
        """
        return list(self._agents.values())

    def select_agent(self, session_id: UUID, agent_name: str) -> None:
        """为指定会话切换 Agent.

        Args:
            session_id: 目标会话 ID.
            agent_name: 要切换到的 Agent 名称.

        Raises:
            KeyError: 如果 Agent 名称不存在.
            SessionNotFoundError: 如果 session_id 不存在.
        """
        if agent_name not in self._agents:
            raise KeyError(f"Agent '{agent_name}' not found.")

        session = self._session_manager.get(session_id)
        session.agent_name = agent_name
        session.last_updated = datetime.now(timezone.utc)
        # 持久化更新
        self._session_manager._persist(session)

    def register_agent(self, agent: AgentDefinition) -> None:
        """注册一个新的 Agent 定义.

        Args:
            agent: 要注册的 AgentDefinition 实例.
        """
        self._agents[agent.name] = agent

    def _build_system_prompt(self, agent: AgentDefinition) -> str:
        """组装完整的 system prompt.

        组装顺序：content_safety + napcat_plugin_dev + user_context
        内容安全 prompt 始终在最前面，不可被覆盖或移除。

        Args:
            agent: 当前使用的 Agent 定义.

        Returns:
            完整的 system prompt 字符串.
        """
        parts: list[str] = []

        # 1. 不可修改的内容安全 prompt（始终在最前面）
        try:
            content_safety = get_content_safety_prompt()
            if content_safety:
                parts.append(content_safety)
        except FileNotFoundError:
            logger.warning("内容安全 prompt 文件未找到，跳过。")

        # 2. 内置 NapCat 插件开发知识库
        try:
            napcat_dev = get_napcat_plugin_dev_prompt()
            if napcat_dev:
                parts.append(napcat_dev)
        except FileNotFoundError:
            logger.warning("NapCat 插件开发知识库 prompt 文件未找到，跳过。")

        # 3. 用户自定义上下文（如果有）
        if self._context_loader is not None:
            user_context = self._context_loader.load()
            if user_context:
                parts.append(user_context)

        # 4. Agent 自身的 system_prompt（如果有）
        if agent.system_prompt:
            parts.append(agent.system_prompt)

        return "\n\n".join(parts)

    def _get_tool_definitions_for_agent(self, agent: AgentDefinition) -> list[dict]:
        """获取 Agent 允许使用的工具定义列表（OpenAI function calling 格式）.

        根据 Agent 的 permission_rules 过滤工具。

        Args:
            agent: 当前使用的 Agent 定义.

        Returns:
            OpenAI tools 格式的工具定义列表.
        """
        all_tools = self._tool_registry.list_all()
        permitted_tools: list[dict] = []

        for tool_id, description in all_tools.items():
            # 检查工具是否在 agent 的 tool_ids 列表中
            if agent.tool_ids and tool_id not in agent.tool_ids:
                continue

            # 检查权限规则
            permission = evaluate_permission(tool_id, agent.permission_rules)
            if permission == "deny":
                continue

            # 获取工具的参数 schema
            try:
                tool_def = self._tool_registry.get(tool_id)
            except KeyError:
                continue

            # 构建 OpenAI function calling 格式
            parameters: dict
            if isinstance(tool_def.parameters_schema, type):
                # pydantic model → JSON Schema
                parameters = tool_def.parameters_schema.model_json_schema()
            elif isinstance(tool_def.parameters_schema, dict):
                parameters = tool_def.parameters_schema
            else:
                parameters = {"type": "object", "properties": {}}

            permitted_tools.append(
                {
                    "type": "function",
                    "function": {
                        "name": tool_id,
                        "description": description,
                        "parameters": parameters,
                    },
                }
            )

        return permitted_tools

    def _build_messages_payload(self, system_prompt: str, messages: list[Message]) -> list[dict]:
        """构建 OpenAI API 消息格式的 payload.

        Args:
            system_prompt: 完整的 system prompt.
            messages: 会话消息历史.

        Returns:
            OpenAI messages 格式的列表.
        """
        payload: list[dict] = []

        # System message
        payload.append({"role": "system", "content": system_prompt})

        # 历史消息
        for msg in messages:
            entry: dict = {"role": msg.role, "content": msg.content}

            # assistant 消息可能包含 tool_calls
            if msg.role == "assistant" and msg.tool_calls:
                entry["tool_calls"] = [
                    {
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.function_name,
                            "arguments": tc.arguments,
                        },
                    }
                    for tc in msg.tool_calls
                ]
                # 如果有 tool_calls，content 可能为空
                if not msg.content:
                    entry["content"] = None

            # tool 消息需要 tool_call_id
            if msg.role == "tool" and msg.tool_call_id:
                entry["tool_call_id"] = msg.tool_call_id
                if msg.tool_name:
                    entry["name"] = msg.tool_name

            payload.append(entry)

        return payload

    async def submit(
        self,
        session_id: UUID,
        user_message: str,
        on_event: Callable[[StreamEvent], None] | None = None,
        _nesting_depth: int = 0,
    ) -> None:
        """提交用户消息并启动 LLM 流式交互.

        主入口方法。构建请求 payload，调用 LLM API，处理流式响应，
        执行工具循环（最多 25 次迭代）。

        Args:
            session_id: 目标会话 ID.
            user_message: 用户消息内容.
            on_event: 事件回调函数，每当产生新的流式事件时调用.
            _nesting_depth: 内部参数，子 Agent 嵌套深度计数.

        Raises:
            SessionNotFoundError: 如果 session_id 不存在.
            NoActiveProviderError: 如果未设置活跃 Provider.
        """
        # 获取会话
        session = self._session_manager.get(session_id)

        # 追加用户消息
        user_msg = Message(
            id=uuid4(),
            role="user",
            content=user_message,
            timestamp=datetime.now(timezone.utc),
        )
        self._session_manager.append_message(session_id, user_msg)

        # 获取 Agent 定义
        agent = self._agents.get(session.agent_name)
        if agent is None:
            agent = self._agents.get("napcat-plugin-dev")
            if agent is None:
                # 不应该发生，但防御性处理
                raise AgentError("No agent available for session.")

        # 构建 system prompt
        system_prompt = self._build_system_prompt(agent)

        # 获取工具定义
        tool_definitions = self._get_tool_definitions_for_agent(agent)

        # 获取活跃 Provider 和 ModelConfig，解析协议适配器
        # 在整个工具循环中使用同一个 adapter 实例（Requirement 7.5）
        provider, model_config = self._provider_registry.get_active()
        adapter = self._adapter_registry.resolve(provider.protocol_type)

        # 工具循环
        for iteration in range(MAX_TOOL_LOOP_ITERATIONS):
            # 重新加载会话以获取最新消息
            session = self._session_manager.get(session_id)

            # 构建请求 payload
            messages_payload = self._build_messages_payload(system_prompt, session.messages)

            # 调用 LLM API
            tool_calls_completed = await self._call_llm_streaming(
                session_id=session_id,
                messages_payload=messages_payload,
                tool_definitions=tool_definitions,
                agent=agent,
                on_event=on_event,
                nesting_depth=_nesting_depth,
                adapter=adapter,
                provider=provider,
                model_config=model_config,
            )

            # 如果没有工具调用完成，流已结束
            if not tool_calls_completed:
                return

            # 处理工具调用
            has_pending_tools = await self._execute_tool_calls(
                session_id=session_id,
                tool_calls=tool_calls_completed,
                agent=agent,
                on_event=on_event,
                nesting_depth=_nesting_depth,
            )

            # 如果没有需要继续的工具调用，结束循环
            if not has_pending_tools:
                return

        # 达到最大迭代次数
        end_event = StreamEnd(reason="max_iterations")
        if on_event:
            on_event(end_event)

        # 追加一条 assistant 消息说明达到迭代上限
        limit_msg = Message(
            id=uuid4(),
            role="assistant",
            content="已达到工具调用最大迭代次数（25次），停止继续执行。",
            timestamp=datetime.now(timezone.utc),
        )
        self._session_manager.append_message(session_id, limit_msg)

    async def _call_llm_streaming(
        self,
        session_id: UUID,
        messages_payload: list[dict],
        tool_definitions: list[dict],
        agent: AgentDefinition,
        on_event: Callable[[StreamEvent], None] | None,
        nesting_depth: int,
        adapter: ProtocolAdapter,
        provider: Provider,
        model_config: ModelConfig,
    ) -> list[ToolCallComplete]:
        """通过 httpx async streaming 调用 LLM API.

        使用协议适配器构建请求和解析流式响应，支持多提供商原生协议。

        Args:
            session_id: 会话 ID.
            messages_payload: OpenAI 格式的消息列表.
            tool_definitions: 工具定义列表.
            agent: 当前 Agent 定义.
            on_event: 事件回调.
            nesting_depth: 嵌套深度.
            adapter: 协议适配器实例.
            provider: 当前活跃的 Provider.
            model_config: 当前活跃的 ModelConfig.

        Returns:
            本次流式调用中完成的 ToolCallComplete 事件列表.
            空列表表示流正常结束（无工具调用）或发生错误。
        """
        from src.core.agent.session import Message as MessageType

        # 使用适配器构建 HTTP 请求（Requirement 7.2）
        # 获取会话中的原始消息
        session = self._session_manager.get(session_id)

        # 构建包含 system prompt 的完整消息列表
        # 提取 system prompt（messages_payload 的第一条消息）
        system_content = ""
        if messages_payload and messages_payload[0].get("role") == "system":
            system_content = messages_payload[0]["content"]

        all_messages: list[MessageType] = []

        # 创建 system message（使用 model_construct 绕过 Literal 验证）
        if system_content:
            system_msg = MessageType.model_construct(
                id=uuid4(),
                role="system",
                content=system_content,
                timestamp=datetime.now(timezone.utc),
                tool_call_id=None,
                tool_name=None,
                tool_calls=None,
            )
            all_messages.append(system_msg)

        # 添加会话中的消息
        all_messages.extend(session.messages)

        # 使用适配器构建请求
        request_spec = adapter.build_request(
            messages=all_messages,
            tool_definitions=tool_definitions,
            model_config=model_config,
            provider=provider,
        )

        # 收集工具调用完成事件
        tool_calls_completed: list[ToolCallComplete] = []
        assistant_content_parts: list[str] = []
        stream_ended_normally = False
        error_occurred = False

        try:
            async with httpx.AsyncClient(
                timeout=httpx.Timeout(
                    connect=10.0,
                    read=INACTIVITY_TIMEOUT_SECONDS,
                    write=10.0,
                    pool=10.0,
                )
            ) as client:
                async with client.stream(
                    request_spec.method,
                    request_spec.url,
                    headers=request_spec.headers,
                    json=request_spec.body,
                ) as response:
                    # 检查 HTTP 状态码
                    if response.status_code != 200:
                        error_body = ""
                        async for chunk in response.aiter_bytes():
                            error_body += chunk.decode("utf-8", errors="replace")
                        error_event = StreamErrorEvent(
                            status_code=response.status_code,
                            message=f"LLM API 返回错误: {error_body[:500]}",
                        )
                        if on_event:
                            on_event(error_event)
                        error_occurred = True
                        return []

                    # 使用适配器解析流式响应（Requirement 7.3）
                    async def _response_lines() -> AsyncIterator[str]:
                        """将 httpx 响应行包装为异步迭代器."""
                        async for line in response.aiter_lines():
                            yield line

                    async for event in adapter.parse_stream(_response_lines()):
                        # 处理各类事件
                        if isinstance(event, StreamErrorEvent):
                            # StreamErrorEvent: 中止当前调用，保留对话历史，传播错误（Requirement 7.7）
                            error_occurred = True
                            if on_event:
                                on_event(event)
                            # 保留已累积的部分响应
                            self._persist_partial_response(
                                session_id, assistant_content_parts, tool_calls_completed
                            )
                            return []

                        if isinstance(event, ToolCallComplete):
                            tool_calls_completed.append(event)
                        elif isinstance(event, StreamEnd):
                            stream_ended_normally = True
                        elif isinstance(event, TextDelta):
                            assistant_content_parts.append(event.text)

                        # 转发事件给外部回调
                        if on_event:
                            on_event(event)

        except httpx.TimeoutException as exc:
            error_event = StreamErrorEvent(
                status_code=None,
                message=f"连接超时（{INACTIVITY_TIMEOUT_SECONDS}秒无活动）: {exc}",
            )
            if on_event:
                on_event(error_event)
            # 保留已累积的消息
            self._persist_partial_response(session_id, assistant_content_parts, tool_calls_completed)
            return []

        except httpx.HTTPError as exc:
            error_event = StreamErrorEvent(
                status_code=None,
                message=f"HTTP 连接错误: {exc}",
            )
            if on_event:
                on_event(error_event)
            # 保留已累积的消息
            self._persist_partial_response(session_id, assistant_content_parts, tool_calls_completed)
            return []

        # 如果有文本内容或工具调用，追加 assistant 消息
        if assistant_content_parts or tool_calls_completed:
            assistant_msg = Message(
                id=uuid4(),
                role="assistant",
                content="".join(assistant_content_parts),
                timestamp=datetime.now(timezone.utc),
                tool_calls=[
                    ToolCallInfo(
                        id=tc.tool_call_id,
                        function_name=tc.function_name,
                        arguments=tc.arguments,
                    )
                    for tc in tool_calls_completed
                ]
                if tool_calls_completed
                else None,
            )
            self._session_manager.append_message(session_id, assistant_msg)

        if error_occurred:
            return []

        return tool_calls_completed

    def _persist_partial_response(
        self,
        session_id: UUID,
        content_parts: list[str],
        tool_calls: list[ToolCallComplete],
    ) -> None:
        """在错误发生时保留已累积的部分响应.

        Args:
            session_id: 会话 ID.
            content_parts: 已累积的文本片段.
            tool_calls: 已完成的工具调用.
        """
        content = "".join(content_parts)
        if content or tool_calls:
            partial_msg = Message(
                id=uuid4(),
                role="assistant",
                content=content if content else "[响应中断]",
                timestamp=datetime.now(timezone.utc),
                tool_calls=[
                    ToolCallInfo(
                        id=tc.tool_call_id,
                        function_name=tc.function_name,
                        arguments=tc.arguments,
                    )
                    for tc in tool_calls
                ]
                if tool_calls
                else None,
            )
            try:
                self._session_manager.append_message(session_id, partial_msg)
            except Exception:
                logger.warning("保存部分响应失败，session_id=%s", session_id)

    async def _execute_tool_calls(
        self,
        session_id: UUID,
        tool_calls: list[ToolCallComplete],
        agent: AgentDefinition,
        on_event: Callable[[StreamEvent], None] | None,
        nesting_depth: int,
    ) -> bool:
        """执行工具调用并将结果追加到会话.

        Args:
            session_id: 会话 ID.
            tool_calls: 要执行的工具调用列表.
            agent: 当前 Agent 定义.
            on_event: 事件回调.
            nesting_depth: 当前嵌套深度.

        Returns:
            True 表示有工具被执行（需要重新提交给 LLM），False 表示无需继续.
        """
        any_executed = False

        for tc in tool_calls:
            # 权限检查
            permission = evaluate_permission(tc.function_name, agent.permission_rules)

            if permission == "deny":
                # 权限被拒绝
                result = ToolResult(
                    output=f"权限被拒绝：工具 '{tc.function_name}' 没有匹配的允许规则。",
                    is_error=True,
                )
            elif permission == "ask":
                # 简化处理：发射 PermissionAskEvent，当前版本直接拒绝
                # 未来版本将实现用户交互确认
                if on_event:
                    ask_event = PermissionAskEvent(
                        tool_id=tc.function_name,
                        pattern="",
                        description=f"工具 '{tc.function_name}' 请求执行权限确认",
                    )
                    on_event(ask_event)
                # 简化版本：直接拒绝（未来实现用户交互）
                result = ToolResult(
                    output=f"权限确认超时：工具 '{tc.function_name}' 的执行请求未获得用户确认。",
                    is_error=True,
                )
            else:
                # permission == "allow"
                # 检查是否是子 Agent 调用
                if tc.function_name in self._agents and tc.function_name != agent.name:
                    result = await self._execute_subagent(
                        session_id=session_id,
                        subagent_name=tc.function_name,
                        arguments=tc.arguments,
                        on_event=on_event,
                        nesting_depth=nesting_depth,
                    )
                else:
                    # 普通工具执行
                    try:
                        params = json.loads(tc.arguments) if tc.arguments else {}
                    except json.JSONDecodeError:
                        params = {}

                    try:
                        result = await self._tool_registry.invoke(tc.function_name, params)
                    except KeyError:
                        result = ToolResult(
                            output=f"工具 '{tc.function_name}' 未注册。",
                            is_error=True,
                        )

            # 追加 tool 消息到会话
            tool_msg = Message(
                id=uuid4(),
                role="tool",
                content=result.output,
                timestamp=datetime.now(timezone.utc),
                tool_call_id=tc.tool_call_id,
                tool_name=tc.function_name,
            )
            self._session_manager.append_message(session_id, tool_msg)
            any_executed = True

        return any_executed

    async def _execute_subagent(
        self,
        session_id: UUID,
        subagent_name: str,
        arguments: str,
        on_event: Callable[[StreamEvent], None] | None,
        nesting_depth: int,
    ) -> ToolResult:
        """执行子 Agent 调用.

        Args:
            session_id: 父会话 ID.
            subagent_name: 子 Agent 名称.
            arguments: 调用参数 JSON 字符串.
            on_event: 事件回调.
            nesting_depth: 当前嵌套深度.

        Returns:
            子 Agent 执行结果.
        """
        # 检查嵌套深度
        if nesting_depth >= MAX_SUBAGENT_NESTING_DEPTH:
            return ToolResult(
                output=f"子 Agent 嵌套深度超过最大限制（{MAX_SUBAGENT_NESTING_DEPTH}层）。",
                is_error=True,
            )

        # 获取子 Agent 定义
        subagent = self._agents.get(subagent_name)
        if subagent is None:
            return ToolResult(
                output=f"子 Agent '{subagent_name}' 未找到。",
                is_error=True,
            )

        # 解析参数
        try:
            params = json.loads(arguments) if arguments else {}
        except json.JSONDecodeError:
            params = {}

        user_message = params.get("message", params.get("input", str(params)))

        # 创建子会话
        child_session = self._session_manager.create(subagent_name)

        try:
            # 递归调用 submit
            await self.submit(
                session_id=child_session.session_id,
                user_message=user_message,
                on_event=on_event,
                _nesting_depth=nesting_depth + 1,
            )

            # 获取子会话的最后一条 assistant 消息作为结果
            child_session = self._session_manager.get(child_session.session_id)
            last_assistant_msg = None
            for msg in reversed(child_session.messages):
                if msg.role == "assistant":
                    last_assistant_msg = msg
                    break

            if last_assistant_msg:
                return ToolResult(
                    output=last_assistant_msg.content,
                    is_error=False,
                )
            else:
                return ToolResult(
                    output="子 Agent 未产生响应。",
                    is_error=True,
                )

        except Exception as exc:
            return ToolResult(
                output=f"子 Agent '{subagent_name}' 执行失败: {type(exc).__name__}: {exc}",
                is_error=True,
            )
