# -*- coding: utf-8 -*-
"""Google Gemini Protocol Adapter.

实现 Google Gemini GenerativeAI 流式协议适配器，将内部消息/工具表示
转换为 Gemini API 请求载荷，并解析 Gemini SSE 流式响应为 StreamEvent 序列。

Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.7, 5.8, 5.9, 5.10, 5.11, 5.12
"""

from __future__ import annotations

import json
import logging
from typing import AsyncIterator

from src.core.agent.protocol import (
    HttpRequestSpec,
    ProtocolAdapter,
    _validate_messages_not_empty,
)
from src.core.agent.provider import ModelConfig, Provider
from src.core.agent.session import Message
from src.core.agent.stream import (
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    TextDelta,
    ToolCallComplete,
    ToolCallStart,
)
from src.core.agent.tool import ToolResult

logger = logging.getLogger(__name__)


class GeminiAdapter(ProtocolAdapter):
    """Google Gemini GenerativeAI 协议适配器.

    将内部消息格式转换为 Gemini API 请求载荷（contents/parts 格式），
    并解析 Gemini SSE 流式响应为 StreamEvent 序列。

    URL 模式: {base_url}/models/{model_id}:streamGenerateContent?alt=sse&key={api_key}
    角色映射: 内部 "assistant" → Gemini "model", "user" → "user"
    API 密钥通过 URL 查询参数传递，而非请求头。
    """

    def build_request(
        self,
        messages: list[Message],
        tool_definitions: list[dict],
        model_config: ModelConfig,
        provider: Provider,
    ) -> HttpRequestSpec:
        """构建 Gemini GenerativeAI API 请求.

        Args:
            messages: 内部消息历史列表.
            tool_definitions: OpenAI 格式的工具定义列表.
            model_config: 模型配置（model_id, temperature 等）.
            provider: Provider 实例（用于 URL、密钥等）.

        Returns:
            包含 method, url, headers, body 的 HttpRequestSpec.

        Raises:
            ValidationError: 如果 messages 列表为空.
        """
        _validate_messages_not_empty(messages)

        # Construct URL with API key as query parameter
        base_url = str(provider.api_base_url).rstrip("/")
        api_key = provider.api_key_ref
        url = (
            f"{base_url}/models/{model_config.model_id}"
            f":streamGenerateContent?alt=sse&key={api_key}"
        )

        # Build headers (minimal - API key is in URL)
        headers = self.build_headers(api_key)

        # Build request body
        # Extract system messages and put them in systemInstruction
        system_content = None
        non_system_messages = []
        for msg in messages:
            if getattr(msg, "role", None) == "system":
                system_content = msg.content
            else:
                non_system_messages.append(msg)

        body: dict = {
            "contents": self._convert_messages(non_system_messages),
        }

        # Add system instruction if present
        if system_content:
            body["systemInstruction"] = {
                "parts": [{"text": system_content}]
            }

        # generationConfig - include temperature, topP, maxOutputTokens
        generation_config: dict = {}
        if model_config.temperature is not None:
            generation_config["temperature"] = model_config.temperature
        if model_config.top_p is not None:
            generation_config["topP"] = model_config.top_p
        if model_config.max_tokens is not None:
            generation_config["maxOutputTokens"] = model_config.max_tokens

        if generation_config:
            body["generationConfig"] = generation_config

        # Tool definitions - convert to functionDeclarations format
        if tool_definitions:
            body["tools"] = [
                {
                    "functionDeclarations": self._convert_tool_definitions(
                        tool_definitions
                    )
                }
            ]

        return HttpRequestSpec(
            method="POST",
            url=url,
            headers=headers,
            body=body,
        )

    async def parse_stream(
        self, response_lines: AsyncIterator[str]
    ) -> AsyncIterator[StreamEvent]:
        """解析 Gemini SSE 流式响应为 StreamEvent 序列.

        Gemini 流式响应格式为 SSE，每个 data 行包含一个 JSON 对象。
        解析 candidates[0].content.parts 中的 text 和 functionCall 部分，
        以及 candidates[0].finishReason 来确定流结束原因。

        Args:
            response_lines: SSE 行的异步迭代器.

        Yields:
            StreamEvent 实例.
        """
        try:
            async for line in response_lines:
                line = line.rstrip("\r\n")

                # Skip empty lines and comments
                if not line or line.startswith(":"):
                    continue

                # Parse SSE "data:" prefix
                if not line.startswith("data:"):
                    continue

                # Extract data content
                data = line[5:].lstrip(" ")

                # Skip empty data
                if not data:
                    continue

                # Parse JSON chunk
                try:
                    chunk = json.loads(data)
                except (json.JSONDecodeError, ValueError) as exc:
                    yield StreamErrorEvent(
                        status_code=None,
                        message=f"Malformed JSON in Gemini SSE data: {exc}",
                    )
                    return

                # Process the chunk and yield events
                async for event in self._process_chunk(chunk):
                    yield event
                    # Stop on error
                    if isinstance(event, (StreamErrorEvent, StreamEnd)):
                        return

        except Exception as exc:
            yield StreamErrorEvent(
                status_code=None,
                message=f"Gemini stream parsing error: {exc}",
            )

    def build_headers(self, api_key: str) -> dict[str, str]:
        """构建 Gemini API 请求头.

        Gemini API 密钥通过 URL 查询参数传递，请求头仅需 Content-Type。

        Args:
            api_key: API 密钥字符串（未使用于请求头）.

        Returns:
            包含 Content-Type 的请求头字典.
        """
        return {
            "Content-Type": "application/json",
        }

    def build_tool_result_payload(
        self, tool_call_id: str, tool_result: ToolResult
    ) -> list[dict]:
        """构建 Gemini 格式的工具结果消息载荷.

        格式为 user 角色消息，包含 functionResponse parts。

        Args:
            tool_call_id: 工具调用 ID（在 Gemini 中用作函数名）.
            tool_result: 工具执行结果.

        Returns:
            包含 user 角色 functionResponse 消息的列表.
        """
        # Parse the output as JSON if possible, otherwise wrap as string
        try:
            response_content = json.loads(tool_result.output)
        except (json.JSONDecodeError, ValueError):
            response_content = {"result": tool_result.output}

        tool_message = {
            "role": "user",
            "parts": [
                {
                    "functionResponse": {
                        "name": tool_call_id,
                        "response": response_content,
                    }
                }
            ],
        }

        return [tool_message]

    async def _process_chunk(self, chunk: dict) -> AsyncIterator[StreamEvent]:
        """处理单个 Gemini 流式响应 chunk.

        Args:
            chunk: 解析后的 JSON chunk.

        Yields:
            StreamEvent 实例.
        """
        # Skip chunks with no candidates or empty candidates array
        # Requirement 5.12
        candidates = chunk.get("candidates")
        if not candidates or not isinstance(candidates, list):
            return

        candidate = candidates[0]
        if not isinstance(candidate, dict):
            return

        # Process content parts
        content = candidate.get("content")
        if content and isinstance(content, dict):
            parts = content.get("parts")
            if parts and isinstance(parts, list):
                for part in parts:
                    if not isinstance(part, dict):
                        continue

                    # Text parts → TextDelta (Requirement 5.4)
                    if "text" in part:
                        text = part["text"]
                        if text:
                            yield TextDelta(text=text)

                    # functionCall parts → ToolCallStart + ToolCallComplete (Requirement 5.5)
                    elif "functionCall" in part:
                        func_call = part["functionCall"]
                        if isinstance(func_call, dict):
                            func_name = func_call.get("name", "")
                            func_args = func_call.get("args", {})

                            # Generate a tool call ID from function name
                            tool_call_id = f"call_{func_name}"

                            # Serialize arguments to JSON string
                            args_str = json.dumps(func_args, ensure_ascii=False)

                            yield ToolCallStart(
                                tool_call_id=tool_call_id,
                                function_name=func_name,
                            )
                            yield ToolCallComplete(
                                tool_call_id=tool_call_id,
                                function_name=func_name,
                                arguments=args_str,
                            )

        # Process finishReason
        finish_reason = candidate.get("finishReason")
        if finish_reason:
            if finish_reason == "STOP":
                # Requirement 5.6
                yield StreamEnd(reason="stop")
            elif finish_reason == "MAX_TOKENS":
                # Requirement 5.7
                yield StreamEnd(reason="max_tokens")
            elif finish_reason in ("SAFETY", "RECITATION"):
                # Requirement 5.8
                yield StreamErrorEvent(
                    status_code=None,
                    message=f"Response blocked due to {finish_reason}",
                )

    def _convert_messages(self, messages: list[Message]) -> list[dict]:
        """将内部 Message 列表转换为 Gemini contents 格式.

        角色映射: "assistant" → "model", "user" → "user", "tool" → "user"

        Args:
            messages: 内部消息列表.

        Returns:
            Gemini contents 格式的消息字典列表.
        """
        result: list[dict] = []

        for msg in messages:
            converted = self._convert_single_message(msg)
            if converted is not None:
                result.append(converted)

        return result

    def _convert_single_message(self, msg: Message) -> dict | None:
        """将单个 Message 转换为 Gemini content 格式.

        Args:
            msg: 内部消息.

        Returns:
            Gemini 格式的 content 字典，或 None.
        """
        if msg.role == "user":
            return {
                "role": "user",
                "parts": [{"text": msg.content}],
            }

        elif msg.role == "assistant":
            parts: list[dict] = []

            # Add text content if present
            if msg.content:
                parts.append({"text": msg.content})

            # Add function calls if present
            if msg.tool_calls:
                for tc in msg.tool_calls:
                    try:
                        args = json.loads(tc.arguments) if tc.arguments else {}
                    except (json.JSONDecodeError, ValueError):
                        args = {}
                    parts.append(
                        {
                            "functionCall": {
                                "name": tc.function_name,
                                "args": args,
                            }
                        }
                    )

            if parts:
                return {
                    "role": "model",
                    "parts": parts,
                }
            return None

        elif msg.role == "tool":
            # Tool results are formatted as user-role functionResponse
            try:
                response_content = json.loads(msg.content)
            except (json.JSONDecodeError, ValueError):
                response_content = {"result": msg.content}

            func_name = msg.tool_name or msg.tool_call_id or "unknown"

            return {
                "role": "user",
                "parts": [
                    {
                        "functionResponse": {
                            "name": func_name,
                            "response": response_content,
                        }
                    }
                ],
            }

        return None

    def _convert_tool_definitions(self, tool_definitions: list[dict]) -> list[dict]:
        """将 OpenAI 格式的工具定义转换为 Gemini functionDeclarations 格式.

        OpenAI 格式:
        {
            "type": "function",
            "function": {
                "name": "...",
                "description": "...",
                "parameters": {...}
            }
        }

        Gemini 格式:
        {
            "name": "...",
            "description": "...",
            "parameters": {...}
        }

        Args:
            tool_definitions: OpenAI 格式的工具定义列表.

        Returns:
            Gemini functionDeclarations 格式的列表.
        """
        declarations: list[dict] = []

        for tool_def in tool_definitions:
            if not isinstance(tool_def, dict):
                continue

            function_info = tool_def.get("function", {})
            if not isinstance(function_info, dict):
                continue

            declaration: dict = {
                "name": function_info.get("name", ""),
                "description": function_info.get("description", ""),
            }

            # Include parameters if present
            parameters = function_info.get("parameters")
            if parameters:
                declaration["parameters"] = parameters

            declarations.append(declaration)

        return declarations
