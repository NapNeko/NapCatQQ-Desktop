# -*- coding: utf-8 -*-
"""StreamProcessor 与流式事件类型.

实现 OpenAI-compatible SSE (Server-Sent Events) 流式响应的解析与事件分发。
StreamProcessor 接收 SSE 行数据，解析 JSON chunks，并发射类型化的 StreamEvent 事件，
支持文本增量、工具调用增量拼接、流结束和错误处理。

Requirements: 5.1, 5.2, 5.3
"""

from __future__ import annotations

import json
from abc import ABC
from dataclasses import dataclass, field
from typing import Callable


# --- StreamEvent 类型层次 ---


class StreamEvent(ABC):
    """流式事件基类."""

    pass


@dataclass(frozen=True)
class TextDelta(StreamEvent):
    """文本增量事件.

    当 LLM 返回文本内容时发射。

    Attributes:
        text: 增量文本片段.
    """

    text: str


@dataclass(frozen=True)
class ToolCallStart(StreamEvent):
    """工具调用开始事件.

    当检测到新的 tool_call 时发射（首次出现某个 index 的 tool_call）。

    Attributes:
        tool_call_id: 工具调用的唯一标识.
        function_name: 被调用的函数名.
    """

    tool_call_id: str
    function_name: str


@dataclass(frozen=True)
class ToolCallDelta(StreamEvent):
    """工具调用参数增量事件.

    当 tool_call 的 arguments 有新的增量数据时发射。

    Attributes:
        tool_call_id: 工具调用的唯一标识.
        arguments_delta: 参数 JSON 的增量片段.
    """

    tool_call_id: str
    arguments_delta: str


@dataclass(frozen=True)
class ToolCallComplete(StreamEvent):
    """工具调用完成事件.

    当一个 tool_call 的所有参数已完整接收时发射。

    Attributes:
        tool_call_id: 工具调用的唯一标识.
        function_name: 被调用的函数名.
        arguments: 完整的参数 JSON 字符串.
    """

    tool_call_id: str
    function_name: str
    arguments: str


@dataclass(frozen=True)
class StreamEnd(StreamEvent):
    """流结束事件.

    Attributes:
        reason: 结束原因，可选值: "stop" | "max_iterations" | "error".
    """

    reason: str


@dataclass(frozen=True)
class StreamErrorEvent(StreamEvent):
    """流错误事件.

    当 LLM API 返回错误或连接异常时发射。

    Attributes:
        status_code: HTTP 状态码，连接超时等场景可能为 None.
        message: 错误描述信息.
    """

    status_code: int | None
    message: str


@dataclass(frozen=True)
class PermissionAskEvent(StreamEvent):
    """权限询问事件.

    当工具调用需要用户确认权限时发射。

    Attributes:
        tool_id: 需要权限确认的工具 ID.
        pattern: 匹配到的权限规则 pattern.
        description: 请求操作的描述.
    """

    tool_id: str
    pattern: str
    description: str


# --- 内部状态跟踪 ---


@dataclass
class _PendingToolCall:
    """跟踪正在增量拼接中的 tool_call.

    Attributes:
        tool_call_id: 工具调用 ID.
        function_name: 函数名（可能在首个 chunk 中完整给出，也可能增量拼接）.
        arguments: 已累积的参数 JSON 字符串.
        started: 是否已发射 ToolCallStart 事件.
    """

    tool_call_id: str
    function_name: str
    arguments: str = ""
    started: bool = False


# --- StreamProcessor ---


class StreamProcessor:
    """OpenAI-compatible SSE 流式响应处理器.

    解析 SSE 格式的行数据（如 "data: {...}"），将 OpenAI Chat Completion
    streaming 格式的 chunks 转换为类型化的 StreamEvent 事件序列。

    支持的功能：
    - 解析 delta.content → TextDelta 事件
    - 跟踪 tool_calls 增量拼接 → ToolCallStart / ToolCallDelta / ToolCallComplete
    - 处理 "data: [DONE]" 终止信号 → StreamEnd(reason="stop")
    - 处理 finish_reason="tool_calls" → 完成所有 pending tool calls
    - 处理 finish_reason="stop" → StreamEnd(reason="stop")

    Usage:
        processor = StreamProcessor(on_event=callback)
        for line in sse_lines:
            processor.feed_line(line)
    """

    def __init__(self, on_event: Callable[[StreamEvent], None] | None = None) -> None:
        """初始化 StreamProcessor.

        Args:
            on_event: 事件回调函数，每当产生新事件时调用.
                      如果为 None，事件仅存储在 events 列表中.
        """
        self._on_event = on_event
        self._pending_tool_calls: dict[int, _PendingToolCall] = {}
        self._events: list[StreamEvent] = []
        self._done: bool = False

    @property
    def events(self) -> list[StreamEvent]:
        """获取已发射的所有事件列表."""
        return list(self._events)

    @property
    def done(self) -> bool:
        """流是否已结束."""
        return self._done

    def feed_line(self, line: str) -> list[StreamEvent]:
        """处理一行 SSE 数据.

        接受原始 SSE 行（如 "data: {...}" 或 "data: [DONE]"），
        解析并发射相应的事件。

        Args:
            line: 原始 SSE 行字符串.

        Returns:
            本次调用产生的事件列表.
        """
        if self._done:
            return []

        # 去除行尾换行
        line = line.rstrip("\r\n")

        # 跳过空行和注释行
        if not line or line.startswith(":"):
            return []

        # 解析 SSE "data:" 前缀
        if not line.startswith("data:"):
            return []

        # 提取 data 内容（去除 "data:" 前缀和前导空格）
        data = line[5:].lstrip(" ")

        # 处理 [DONE] 终止信号
        if data == "[DONE]":
            return self._handle_done()

        # 尝试解析 JSON
        try:
            chunk = json.loads(data)
        except (json.JSONDecodeError, ValueError):
            # 无法解析的数据行，静默跳过
            return []

        return self._process_chunk(chunk)

    def feed_bytes(self, raw: bytes, encoding: str = "utf-8") -> list[StreamEvent]:
        """处理原始字节数据（可能包含多行）.

        Args:
            raw: 原始字节数据.
            encoding: 字符编码，默认 utf-8.

        Returns:
            本次调用产生的所有事件列表.
        """
        text = raw.decode(encoding)
        all_events: list[StreamEvent] = []
        for line in text.split("\n"):
            events = self.feed_line(line)
            all_events.extend(events)
        return all_events

    def finalize(self) -> list[StreamEvent]:
        """强制结束流处理.

        完成所有 pending tool calls 并发射 StreamEnd 事件。
        用于连接中断等异常场景。

        Returns:
            本次调用产生的事件列表.
        """
        if self._done:
            return []
        return self._handle_done()

    def emit_error(self, status_code: int | None, message: str) -> StreamErrorEvent:
        """发射错误事件.

        Args:
            status_code: HTTP 状态码.
            message: 错误描述.

        Returns:
            发射的 StreamErrorEvent.
        """
        event = StreamErrorEvent(status_code=status_code, message=message)
        self._emit(event)
        self._done = True
        return event

    def _handle_done(self) -> list[StreamEvent]:
        """处理流结束（[DONE] 信号或 finalize）."""
        emitted: list[StreamEvent] = []

        # 完成所有 pending tool calls
        emitted.extend(self._finalize_all_pending_tool_calls())

        # 发射 StreamEnd
        end_event = StreamEnd(reason="stop")
        self._emit(end_event)
        emitted.append(end_event)

        self._done = True
        return emitted

    def _process_chunk(self, chunk: dict) -> list[StreamEvent]:
        """处理一个解析后的 JSON chunk.

        OpenAI Chat Completion streaming 格式：
        {
            "choices": [{
                "delta": {
                    "content": "...",
                    "tool_calls": [...]
                },
                "finish_reason": "stop" | "tool_calls" | null
            }]
        }
        """
        emitted: list[StreamEvent] = []

        choices = chunk.get("choices")
        if not choices or not isinstance(choices, list):
            return emitted

        choice = choices[0]
        if not isinstance(choice, dict):
            return emitted

        delta = choice.get("delta", {})
        finish_reason = choice.get("finish_reason")

        # 处理文本增量
        if isinstance(delta, dict):
            content = delta.get("content")
            if content is not None and content != "":
                event = TextDelta(text=content)
                self._emit(event)
                emitted.append(event)

            # 处理 tool_calls 增量
            tool_calls = delta.get("tool_calls")
            if tool_calls and isinstance(tool_calls, list):
                for tc in tool_calls:
                    if not isinstance(tc, dict):
                        continue
                    tc_events = self._process_tool_call_delta(tc)
                    emitted.extend(tc_events)

        # 处理 finish_reason
        if finish_reason == "tool_calls":
            # 完成所有 pending tool calls
            emitted.extend(self._finalize_all_pending_tool_calls())
        elif finish_reason == "stop":
            # 完成所有 pending tool calls 并发射 StreamEnd
            emitted.extend(self._finalize_all_pending_tool_calls())
            end_event = StreamEnd(reason="stop")
            self._emit(end_event)
            emitted.append(end_event)
            self._done = True

        return emitted

    def _process_tool_call_delta(self, tc: dict) -> list[StreamEvent]:
        """处理单个 tool_call 增量.

        OpenAI tool_call delta 格式：
        {
            "index": 0,
            "id": "call_xxx",          # 仅首次出现
            "type": "function",        # 仅首次出现
            "function": {
                "name": "func_name",   # 仅首次出现或增量
                "arguments": "..."     # 增量拼接
            }
        }
        """
        emitted: list[StreamEvent] = []

        index = tc.get("index", 0)
        tc_id = tc.get("id")
        function_info = tc.get("function", {})

        if not isinstance(function_info, dict):
            function_info = {}

        func_name = function_info.get("name", "")
        arguments_delta = function_info.get("arguments", "")

        # 检查是否是新的 tool_call（首次出现该 index）
        if index not in self._pending_tool_calls:
            # 新的 tool_call
            pending = _PendingToolCall(
                tool_call_id=tc_id or "",
                function_name=func_name or "",
                arguments="",
            )
            self._pending_tool_calls[index] = pending

            # 如果有 id 和 function_name，发射 ToolCallStart
            if pending.tool_call_id and pending.function_name:
                start_event = ToolCallStart(
                    tool_call_id=pending.tool_call_id,
                    function_name=pending.function_name,
                )
                self._emit(start_event)
                emitted.append(start_event)
                pending.started = True
        else:
            pending = self._pending_tool_calls[index]

            # 更新 id（如果之前为空）
            if tc_id and not pending.tool_call_id:
                pending.tool_call_id = tc_id

            # 更新 function_name（如果之前为空）
            if func_name and not pending.function_name:
                pending.function_name = func_name

            # 如果还没发射 start 且现在有了完整信息
            if not pending.started and pending.tool_call_id and pending.function_name:
                start_event = ToolCallStart(
                    tool_call_id=pending.tool_call_id,
                    function_name=pending.function_name,
                )
                self._emit(start_event)
                emitted.append(start_event)
                pending.started = True

        # 处理 arguments 增量
        if arguments_delta:
            pending.arguments += arguments_delta

            # 发射 ToolCallDelta（仅在已 started 后）
            if pending.started:
                delta_event = ToolCallDelta(
                    tool_call_id=pending.tool_call_id,
                    arguments_delta=arguments_delta,
                )
                self._emit(delta_event)
                emitted.append(delta_event)

        return emitted

    def _finalize_all_pending_tool_calls(self) -> list[StreamEvent]:
        """完成所有 pending tool calls，发射 ToolCallComplete 事件."""
        emitted: list[StreamEvent] = []

        for _index, pending in sorted(self._pending_tool_calls.items()):
            # 如果还没 started，先发射 start
            if not pending.started and pending.tool_call_id and pending.function_name:
                start_event = ToolCallStart(
                    tool_call_id=pending.tool_call_id,
                    function_name=pending.function_name,
                )
                self._emit(start_event)
                emitted.append(start_event)
                pending.started = True

            # 发射 ToolCallComplete
            if pending.started:
                complete_event = ToolCallComplete(
                    tool_call_id=pending.tool_call_id,
                    function_name=pending.function_name,
                    arguments=pending.arguments,
                )
                self._emit(complete_event)
                emitted.append(complete_event)

        # 清空 pending
        self._pending_tool_calls.clear()
        return emitted

    def _emit(self, event: StreamEvent) -> None:
        """发射事件：存储到列表并调用回调."""
        self._events.append(event)
        if self._on_event is not None:
            self._on_event(event)
