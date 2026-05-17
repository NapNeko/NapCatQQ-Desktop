# -*- coding: utf-8 -*-
"""Azure OpenAI Protocol Adapter.

将 Azure OpenAI Service 的 Chat Completions 流式逻辑封装为 ProtocolAdapter 实现. 
使用 Azure 特定的 URL 构造和 api-key 认证头, SSE 解析复用 StreamProcessor. 

Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6
"""

from __future__ import annotations

import json
from typing import AsyncIterator

from src.core.logging import LogSource, logger

from src.core.agent.api_key_pool import pick_api_key
from src.core.agent.errors import ValidationError
from src.core.agent.protocol import (
    HttpRequestSpec,
    ProtocolAdapter,
    _validate_messages_not_empty,
)
from src.core.agent.provider import ModelConfig, Provider
from src.core.agent.session import Message
from src.core.agent.stream import StreamErrorEvent, StreamEvent, StreamProcessor
from src.core.agent.tool import ToolResult


class AzureAdapter(ProtocolAdapter):
    """Azure OpenAI Service 协议适配器.

    使用 Azure 特定的 URL 模式和 api-key 认证头, 请求体格式与 OpenAI 相同. 
    SSE 流式解析委托给 StreamProcessor (与 OpenAI 适配器完全一致) . 
    """

    def build_request(
        self,
        messages: list[Message],
        tool_definitions: list[dict],
        model_config: ModelConfig,
        provider: Provider,
    ) -> HttpRequestSpec:
        """构建 Azure OpenAI Chat Completions API 请求.

        URL 模式: {resource_endpoint}/openai/deployments/{deployment_name}/chat/completions?api-version={api_version}

        Args:
            messages: 内部消息历史列表.
            tool_definitions: OpenAI 格式的工具定义列表.
            model_config: 模型配置 (model_id, temperature 等) .
            provider: Provider 实例 (用于 azure_config, 密钥等) .

        Returns:
            包含 method, url, headers, body 的 HttpRequestSpec.

        Raises:
            ValidationError: 如果 messages 列表为空.
            ValidationError: 如果 azure_config 缺失或缺少必需子字段.
        """
        _validate_messages_not_empty(messages)

        # Validate azure_config presence and required sub-fields
        azure_config = provider.azure_config
        if azure_config is None:
            raise ValidationError(
                field="azure_config",
                reason="azure_config is required for Azure provider but was not provided",
            )

        if not azure_config.resource_endpoint:
            raise ValidationError(
                field="azure_config.resource_endpoint",
                reason="resource_endpoint is required in azure_config",
            )

        if not azure_config.deployment_name:
            raise ValidationError(
                field="azure_config.deployment_name",
                reason="deployment_name is required in azure_config",
            )

        # Construct Azure URL - strip trailing slash to avoid double slashes
        resource_endpoint = azure_config.resource_endpoint.rstrip("/")
        deployment_name = azure_config.deployment_name
        api_version = azure_config.api_version

        url = (
            f"{resource_endpoint}/openai/deployments/{deployment_name}"
            f"/chat/completions?api-version={api_version}"
        )

        # Build headers with api-key authentication
        headers = self.build_headers(pick_api_key(provider.api_key_ref))

        # Build request body (same format as OpenAI)
        body: dict = {
            "model": model_config.model_id,
            "messages": self._convert_messages(messages),
            "stream": True,
        }

        # Optional fields - only include when explicitly provided
        if tool_definitions:
            body["tools"] = tool_definitions

        if model_config.temperature is not None:
            body["temperature"] = model_config.temperature
        if model_config.top_p is not None:
            body["top_p"] = model_config.top_p
        if model_config.max_tokens is not None:
            body["max_tokens"] = model_config.max_tokens

        return HttpRequestSpec(
            method="POST",
            url=url,
            headers=headers,
            body=body,
        )

    async def parse_stream(
        self, response_lines: AsyncIterator[str]
    ) -> AsyncIterator[StreamEvent]:
        """解析 Azure OpenAI SSE 流式响应为 StreamEvent 序列.

        复用 StreamProcessor 进行 SSE 解析 (与 OpenAI 适配器完全一致) . 

        Args:
            response_lines: SSE 行的异步迭代器.

        Yields:
            StreamEvent 实例.
        """
        processor = StreamProcessor()

        try:
            async for line in response_lines:
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
        """构建 Azure OpenAI API 认证请求头.

        使用 api-key 头而非 Authorization: Bearer 令牌. 

        Args:
            api_key: API 密钥字符串.

        Returns:
            包含 api-key 和 Content-Type 的请求头字典.
        """
        return {
            "api-key": api_key,
            "Content-Type": "application/json",
        }

    def build_tool_result_payload(
        self, tool_call_id: str, tool_result: ToolResult
    ) -> list[dict]:
        """构建 Azure OpenAI 格式的工具结果消息载荷.

        格式与 OpenAI 适配器完全相同 (tool 角色消息) . 

        Args:
            tool_call_id: 工具调用 ID.
            tool_result: 工具执行结果.

        Returns:
            包含 tool 结果消息的列表.
        """
        tool_message = {
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": tool_result.output,
        }

        return [tool_message]

    def _convert_messages(self, messages: list[Message]) -> list[dict]:
        """将内部 Message 列表转换为 OpenAI API 消息格式.

        Args:
            messages: 内部消息列表.

        Returns:
            OpenAI API 格式的消息字典列表.
        """
        result: list[dict] = []

        for msg in messages:
            converted = self._convert_single_message(msg)
            if converted is not None:
                result.append(converted)

        return result

    def _convert_single_message(self, msg: Message) -> dict | None:
        """将单个 Message 转换为 OpenAI API 消息格式.

        Args:
            msg: 内部消息.

        Returns:
            OpenAI 格式的消息字典, 或 None (如果无法转换) .
        """
        role = getattr(msg, "role", None)

        if role == "system":
            return {"role": "system", "content": msg.content}

        elif role == "user":
            return {"role": "user", "content": msg.content}

        elif role == "assistant":
            message_dict: dict = {"role": "assistant", "content": msg.content}

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
                "content": msg.content,
            }

        return None
