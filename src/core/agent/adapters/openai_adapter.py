# -*- coding: utf-8 -*-
"""OpenAI Protocol Adapter.

将现有 OpenAI Chat Completions 流式逻辑封装为 ProtocolAdapter 实现. 
通过委托给现有 StreamProcessor 进行 SSE 解析, 确保与当前行为完全一致. 

Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 4.6, 4.7
"""

from __future__ import annotations

import json
from typing import AsyncIterator

from src.core.logging import LogSource, logger

from src.core.agent.api_key_pool import pick_api_key
from src.core.agent.model_param_utils import flatten_array_content
from src.core.agent.protocol import (
    HttpRequestSpec,
    ProtocolAdapter,
    _validate_messages_not_empty,
)
from src.core.agent.provider import ModelConfig, Provider, ProviderApiOptions
from src.core.agent.session import Message
from src.core.agent.stream import StreamErrorEvent, StreamEvent, StreamProcessor
from src.core.agent.tool import ToolResult


class OpenAIAdapter(ProtocolAdapter):
    """OpenAI Chat Completions 协议适配器.

    将内部消息格式转换为 OpenAI API 请求载荷, 并委托 StreamProcessor
    解析 SSE 流式响应, 确保产生与现有实现完全相同的 StreamEvent 序列. 
    """

    def build_request(
        self,
        messages: list[Message],
        tool_definitions: list[dict],
        model_config: ModelConfig,
        provider: Provider,
    ) -> HttpRequestSpec:
        """构建 OpenAI Chat Completions API 请求.

        Args:
            messages: 内部消息历史列表.
            tool_definitions: OpenAI 格式的工具定义列表.
            model_config: 模型配置 (model_id, temperature 等) .
            provider: Provider 实例 (用于 URL, 密钥等) .

        Returns:
            包含 method, url, headers, body 的 HttpRequestSpec.

        Raises:
            ValidationError: 如果 messages 列表为空.
        """
        _validate_messages_not_empty(messages)

        # Resolve api_options: use provider's or default instance
        api_options = provider.api_options if provider.api_options is not None else ProviderApiOptions()

        # Construct URL
        base_url = str(provider.api_base_url).rstrip("/")
        url = f"{base_url}/chat/completions"

        # Build headers - 支持逗号分隔多密钥, 每次请求随机选择一个
        headers = self.build_headers(pick_api_key(provider.api_key_ref))

        # Merge custom_headers (覆盖同名默认头)
        if provider.custom_headers:
            headers.update(provider.custom_headers)

        # Build request body
        body: dict = {
            "model": model_config.model_id,
            "messages": self._convert_messages(messages, api_options),
            "stream": True,
        }

        # Conditionally add stream_options based on api_options
        if api_options.supports_stream_options:
            body["stream_options"] = {"include_usage": True}

        # Optional fields - only include when explicitly provided
        if tool_definitions:
            body["tools"] = tool_definitions

        # Include temperature, top_p, max_tokens from model_config
        if model_config.temperature is not None:
            body["temperature"] = model_config.temperature
        if model_config.top_p is not None:
            body["top_p"] = model_config.top_p
        if model_config.max_tokens is not None:
            body["max_tokens"] = model_config.max_tokens

        # Include reasoning_effort from ModelEntry if set
        model_entry = next(
            (m for m in provider.models if m.model_id == model_config.model_id),
            None,
        )
        if model_entry is not None and model_entry.reasoning_effort is not None:
            body["reasoning_effort"] = model_entry.reasoning_effort

        return HttpRequestSpec(
            method="POST",
            url=url,
            headers=headers,
            body=body,
        )

    async def parse_stream(
        self, response_lines: AsyncIterator[str]
    ) -> AsyncIterator[StreamEvent]:
        """解析 OpenAI SSE 流式响应为 StreamEvent 序列.

        委托给现有 StreamProcessor.feed_line 进行解析, 确保产生
        与当前实现完全相同的事件序列. 

        遇到无法解析为 JSON 的 SSE data 行时, 发射 StreamErrorEvent. 

        Args:
            response_lines: SSE 行的异步迭代器.

        Yields:
            StreamEvent 实例.
        """
        processor = StreamProcessor()

        try:
            async for line in response_lines:
                # Check for HTTP error lines (non-SSE error responses)
                # StreamProcessor handles normal SSE parsing
                events = processor.feed_line(line)
                for event in events:
                    yield event
                    # Stop on error or done
                    if isinstance(event, StreamErrorEvent):
                        return
                if processor.done:
                    return
        except json.JSONDecodeError as exc:
            error_event = StreamErrorEvent(
                status_code=None,
                message=f"Malformed JSON in SSE data: {exc}",
            )
            yield error_event
        except Exception as exc:
            error_event = StreamErrorEvent(
                status_code=None,
                message=f"Stream parsing error: {exc}",
            )
            yield error_event

    def build_headers(self, api_key: str) -> dict[str, str]:
        """构建 OpenAI API 认证请求头.

        Args:
            api_key: API 密钥字符串.

        Returns:
            包含 Authorization 和 Content-Type 的请求头字典.
        """
        return {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        }

    def build_tool_result_payload(
        self, tool_call_id: str, tool_result: ToolResult
    ) -> list[dict]:
        """构建 OpenAI 格式的工具结果消息载荷.

        格式为 assistant 消息 (含 tool_calls 数组) + tool 角色消息 (含结果) . 

        Args:
            tool_call_id: 工具调用 ID.
            tool_result: 工具执行结果.

        Returns:
            包含 assistant tool_calls 消息和 tool 结果消息的列表.
        """
        # Tool-role result message
        tool_message = {
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": tool_result.output,
        }

        return [tool_message]

    def _convert_messages(self, messages: list[Message], api_options: ProviderApiOptions) -> list[dict]:
        """将内部 Message 列表转换为 OpenAI API 消息格式.

        Args:
            messages: 内部消息列表.
            api_options: 供应商 API 兼容性选项.

        Returns:
            OpenAI API 格式的消息字典列表.
        """
        result: list[dict] = []

        for msg in messages:
            converted = self._convert_single_message(msg, api_options)
            if converted is not None:
                result.append(converted)

        return result

    def _convert_single_message(self, msg: Message, api_options: ProviderApiOptions) -> dict | None:
        """将单个 Message 转换为 OpenAI API 消息格式.

        根据 api_options 决定:
        - supports_developer_role=True 时将 system 角色替换为 developer
        - supports_array_content=False 时将数组格式内容转换为纯文本字符串

        Args:
            msg: 内部消息.
            api_options: 供应商 API 兼容性选项.

        Returns:
            OpenAI 格式的消息字典, 或 None (如果无法转换) .
        """
        role = getattr(msg, "role", None)

        # Resolve content: flatten array content if API doesn't support it
        content = msg.content
        if not api_options.supports_array_content and isinstance(content, list):
            content = flatten_array_content(content)

        if role == "system":
            # Replace system role with developer if supported
            actual_role = "developer" if api_options.supports_developer_role else "system"
            return {"role": actual_role, "content": content}

        elif role == "user":
            return {"role": "user", "content": content}

        elif role == "assistant":
            message_dict: dict = {"role": "assistant", "content": content}

            # Include tool_calls if present
            if msg.tool_calls:
                message_dict["tool_calls"] = [
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

            return message_dict

        elif role == "tool":
            return {
                "role": "tool",
                "tool_call_id": msg.tool_call_id or "",
                "content": content,
            }

        return None
