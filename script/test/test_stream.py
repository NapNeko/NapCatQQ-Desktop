# -*- coding: utf-8 -*-
"""单元测试：StreamProcessor 和流式事件类型.

验证 StreamProcessor 正确解析 OpenAI-compatible SSE chunks 并发射类型化事件。
"""

from __future__ import annotations

import json

from src.core.agent.stream import (
    PermissionAskEvent,
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    StreamProcessor,
    TextDelta,
    ToolCallComplete,
    ToolCallDelta,
    ToolCallStart,
)


class TestStreamEventTypes:
    """测试所有 StreamEvent 子类的实例化."""

    def test_text_delta(self) -> None:
        event = TextDelta(text="hello")
        assert event.text == "hello"
        assert isinstance(event, StreamEvent)

    def test_tool_call_start(self) -> None:
        event = ToolCallStart(tool_call_id="call_1", function_name="file_read")
        assert event.tool_call_id == "call_1"
        assert event.function_name == "file_read"
        assert isinstance(event, StreamEvent)

    def test_tool_call_delta(self) -> None:
        event = ToolCallDelta(tool_call_id="call_1", arguments_delta='{"path":')
        assert event.tool_call_id == "call_1"
        assert event.arguments_delta == '{"path":'
        assert isinstance(event, StreamEvent)

    def test_tool_call_complete(self) -> None:
        event = ToolCallComplete(
            tool_call_id="call_1",
            function_name="file_read",
            arguments='{"path": "src/main.py"}',
        )
        assert event.tool_call_id == "call_1"
        assert event.function_name == "file_read"
        assert event.arguments == '{"path": "src/main.py"}'
        assert isinstance(event, StreamEvent)

    def test_stream_end(self) -> None:
        event = StreamEnd(reason="stop")
        assert event.reason == "stop"
        assert isinstance(event, StreamEvent)

    def test_stream_error_event(self) -> None:
        event = StreamErrorEvent(status_code=429, message="Rate limited")
        assert event.status_code == 429
        assert event.message == "Rate limited"
        assert isinstance(event, StreamEvent)

    def test_stream_error_event_no_status(self) -> None:
        event = StreamErrorEvent(status_code=None, message="Timeout")
        assert event.status_code is None
        assert isinstance(event, StreamEvent)

    def test_permission_ask_event(self) -> None:
        event = PermissionAskEvent(
            tool_id="shell_exec",
            pattern="shell_*",
            description="Execute shell command",
        )
        assert event.tool_id == "shell_exec"
        assert event.pattern == "shell_*"
        assert event.description == "Execute shell command"
        assert isinstance(event, StreamEvent)


class TestStreamProcessorTextDelta:
    """测试 StreamProcessor 解析文本增量."""

    def test_simple_text_delta(self) -> None:
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {"content": "Hello"}, "finish_reason": None}]
        }
        line = f"data: {json.dumps(chunk)}"
        events = processor.feed_line(line)
        assert len(events) == 1
        assert isinstance(events[0], TextDelta)
        assert events[0].text == "Hello"

    def test_multiple_text_deltas(self) -> None:
        processor = StreamProcessor()
        texts = ["Hello", " ", "world", "!"]
        for text in texts:
            chunk = {
                "choices": [{"delta": {"content": text}, "finish_reason": None}]
            }
            processor.feed_line(f"data: {json.dumps(chunk)}")

        text_events = [e for e in processor.events if isinstance(e, TextDelta)]
        assert len(text_events) == 4
        assert "".join(e.text for e in text_events) == "Hello world!"

    def test_empty_content_not_emitted(self) -> None:
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {"content": ""}, "finish_reason": None}]
        }
        events = processor.feed_line(f"data: {json.dumps(chunk)}")
        assert len(events) == 0

    def test_null_content_not_emitted(self) -> None:
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {"content": None}, "finish_reason": None}]
        }
        events = processor.feed_line(f"data: {json.dumps(chunk)}")
        assert len(events) == 0


class TestStreamProcessorDone:
    """测试 StreamProcessor 处理 [DONE] 信号."""

    def test_done_signal(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line("data: [DONE]")
        assert len(events) == 1
        assert isinstance(events[0], StreamEnd)
        assert events[0].reason == "stop"
        assert processor.done is True

    def test_lines_after_done_ignored(self) -> None:
        processor = StreamProcessor()
        processor.feed_line("data: [DONE]")
        chunk = {
            "choices": [{"delta": {"content": "ignored"}, "finish_reason": None}]
        }
        events = processor.feed_line(f"data: {json.dumps(chunk)}")
        assert len(events) == 0

    def test_finish_reason_stop(self) -> None:
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {}, "finish_reason": "stop"}]
        }
        events = processor.feed_line(f"data: {json.dumps(chunk)}")
        assert any(isinstance(e, StreamEnd) and e.reason == "stop" for e in events)
        assert processor.done is True


class TestStreamProcessorToolCalls:
    """测试 StreamProcessor 处理 tool_calls 增量拼接."""

    def test_single_tool_call_complete_flow(self) -> None:
        """测试完整的单个 tool_call 流程：start → delta → complete."""
        processor = StreamProcessor()

        # 第一个 chunk: tool_call 开始（id + function name）
        chunk1 = {
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc123",
                        "type": "function",
                        "function": {"name": "file_read", "arguments": ""},
                    }]
                },
                "finish_reason": None,
            }]
        }
        events1 = processor.feed_line(f"data: {json.dumps(chunk1)}")
        assert len(events1) == 1
        assert isinstance(events1[0], ToolCallStart)
        assert events1[0].tool_call_id == "call_abc123"
        assert events1[0].function_name == "file_read"

        # 第二个 chunk: arguments 增量
        chunk2 = {
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {"arguments": '{"path":'},
                    }]
                },
                "finish_reason": None,
            }]
        }
        events2 = processor.feed_line(f"data: {json.dumps(chunk2)}")
        assert len(events2) == 1
        assert isinstance(events2[0], ToolCallDelta)
        assert events2[0].arguments_delta == '{"path":'

        # 第三个 chunk: arguments 继续
        chunk3 = {
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": {"arguments": ' "src/main.py"}'},
                    }]
                },
                "finish_reason": None,
            }]
        }
        events3 = processor.feed_line(f"data: {json.dumps(chunk3)}")
        assert len(events3) == 1
        assert isinstance(events3[0], ToolCallDelta)

        # 第四个 chunk: finish_reason="tool_calls" 触发 complete
        chunk4 = {
            "choices": [{"delta": {}, "finish_reason": "tool_calls"}]
        }
        events4 = processor.feed_line(f"data: {json.dumps(chunk4)}")
        assert len(events4) == 1
        assert isinstance(events4[0], ToolCallComplete)
        assert events4[0].tool_call_id == "call_abc123"
        assert events4[0].function_name == "file_read"
        assert events4[0].arguments == '{"path": "src/main.py"}'

    def test_multiple_tool_calls(self) -> None:
        """测试多个并行 tool_calls."""
        processor = StreamProcessor()

        # 两个 tool_calls 同时开始
        chunk1 = {
            "choices": [{
                "delta": {
                    "tool_calls": [
                        {
                            "index": 0,
                            "id": "call_001",
                            "type": "function",
                            "function": {"name": "file_read", "arguments": ""},
                        },
                        {
                            "index": 1,
                            "id": "call_002",
                            "type": "function",
                            "function": {"name": "grep_search", "arguments": ""},
                        },
                    ]
                },
                "finish_reason": None,
            }]
        }
        events1 = processor.feed_line(f"data: {json.dumps(chunk1)}")
        starts = [e for e in events1 if isinstance(e, ToolCallStart)]
        assert len(starts) == 2

        # 各自的 arguments 增量
        chunk2 = {
            "choices": [{
                "delta": {
                    "tool_calls": [
                        {"index": 0, "function": {"arguments": '{"path": "a.py"}'}},
                    ]
                },
                "finish_reason": None,
            }]
        }
        processor.feed_line(f"data: {json.dumps(chunk2)}")

        chunk3 = {
            "choices": [{
                "delta": {
                    "tool_calls": [
                        {"index": 1, "function": {"arguments": '{"pattern": "TODO"}'}},
                    ]
                },
                "finish_reason": None,
            }]
        }
        processor.feed_line(f"data: {json.dumps(chunk3)}")

        # finish_reason="tool_calls" 完成所有
        chunk4 = {
            "choices": [{"delta": {}, "finish_reason": "tool_calls"}]
        }
        events4 = processor.feed_line(f"data: {json.dumps(chunk4)}")
        completes = [e for e in events4 if isinstance(e, ToolCallComplete)]
        assert len(completes) == 2
        assert completes[0].tool_call_id == "call_001"
        assert completes[0].arguments == '{"path": "a.py"}'
        assert completes[1].tool_call_id == "call_002"
        assert completes[1].arguments == '{"pattern": "TODO"}'

    def test_done_finalizes_pending_tool_calls(self) -> None:
        """测试 [DONE] 信号会完成所有 pending tool calls."""
        processor = StreamProcessor()

        # 开始一个 tool_call
        chunk = {
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_xyz",
                        "type": "function",
                        "function": {"name": "shell_exec", "arguments": '{"cmd": "ls"}'},
                    }]
                },
                "finish_reason": None,
            }]
        }
        processor.feed_line(f"data: {json.dumps(chunk)}")

        # [DONE] 信号
        events = processor.feed_line("data: [DONE]")
        completes = [e for e in events if isinstance(e, ToolCallComplete)]
        ends = [e for e in events if isinstance(e, StreamEnd)]
        assert len(completes) == 1
        assert completes[0].arguments == '{"cmd": "ls"}'
        assert len(ends) == 1


class TestStreamProcessorEdgeCases:
    """测试 StreamProcessor 边界情况."""

    def test_empty_line_ignored(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line("")
        assert len(events) == 0

    def test_comment_line_ignored(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line(": this is a comment")
        assert len(events) == 0

    def test_non_data_line_ignored(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line("event: message")
        assert len(events) == 0

    def test_invalid_json_ignored(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line("data: {invalid json}")
        assert len(events) == 0

    def test_no_choices_ignored(self) -> None:
        processor = StreamProcessor()
        events = processor.feed_line('data: {"id": "chatcmpl-123"}')
        assert len(events) == 0

    def test_callback_invoked(self) -> None:
        received: list[StreamEvent] = []
        processor = StreamProcessor(on_event=received.append)
        chunk = {
            "choices": [{"delta": {"content": "Hi"}, "finish_reason": None}]
        }
        processor.feed_line(f"data: {json.dumps(chunk)}")
        assert len(received) == 1
        assert isinstance(received[0], TextDelta)

    def test_feed_bytes(self) -> None:
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {"content": "Hello"}, "finish_reason": None}]
        }
        raw = f"data: {json.dumps(chunk)}\n\ndata: [DONE]\n".encode("utf-8")
        events = processor.feed_bytes(raw)
        text_events = [e for e in events if isinstance(e, TextDelta)]
        end_events = [e for e in events if isinstance(e, StreamEnd)]
        assert len(text_events) == 1
        assert len(end_events) == 1

    def test_finalize(self) -> None:
        processor = StreamProcessor()
        events = processor.finalize()
        assert len(events) == 1
        assert isinstance(events[0], StreamEnd)
        assert processor.done is True

    def test_emit_error(self) -> None:
        processor = StreamProcessor()
        event = processor.emit_error(500, "Internal Server Error")
        assert isinstance(event, StreamErrorEvent)
        assert event.status_code == 500
        assert event.message == "Internal Server Error"
        assert processor.done is True

    def test_data_with_extra_spaces(self) -> None:
        """测试 data: 后有多余空格的情况."""
        processor = StreamProcessor()
        chunk = {
            "choices": [{"delta": {"content": "test"}, "finish_reason": None}]
        }
        # 多余空格
        events = processor.feed_line(f"data:  {json.dumps(chunk)}")
        assert len(events) == 1
        assert isinstance(events[0], TextDelta)
        assert events[0].text == "test"

    def test_line_with_crlf(self) -> None:
        """测试行尾有 \\r\\n 的情况."""
        processor = StreamProcessor()
        events = processor.feed_line("data: [DONE]\r\n")
        assert len(events) == 1
        assert isinstance(events[0], StreamEnd)
