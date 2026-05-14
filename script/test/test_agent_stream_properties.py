# -*- coding: utf-8 -*-
"""Property-based tests for StreamProcessor module.

Property 13: SSE chunk parsing produces correct typed events.
Validates: Requirements 5.2

For any valid sequence of OpenAI-compatible SSE chunks representing text deltas
and tool calls, the StreamProcessor SHALL emit the corresponding typed events
(TextDelta, ToolCallStart, ToolCallDelta, ToolCallComplete) in the correct order
matching the chunk sequence.
"""

from __future__ import annotations

import json

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from src.core.agent.stream import (
    StreamEnd,
    StreamProcessor,
    TextDelta,
    ToolCallComplete,
    ToolCallDelta,
    ToolCallStart,
)


# --- Strategies ---


def _make_text_delta_line(content: str) -> str:
    """Build an SSE line for a text delta chunk."""
    chunk = {
        "choices": [
            {
                "delta": {"content": content},
                "finish_reason": None,
            }
        ]
    }
    return f"data: {json.dumps(chunk)}"


def _make_tool_call_start_line(index: int, tool_call_id: str, function_name: str) -> str:
    """Build an SSE line for the first tool_call chunk (start)."""
    chunk = {
        "choices": [
            {
                "delta": {
                    "tool_calls": [
                        {
                            "index": index,
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
    return f"data: {json.dumps(chunk)}"


def _make_tool_call_delta_line(index: int, arguments_delta: str) -> str:
    """Build an SSE line for a tool_call arguments delta chunk."""
    chunk = {
        "choices": [
            {
                "delta": {
                    "tool_calls": [
                        {
                            "index": index,
                            "function": {
                                "arguments": arguments_delta,
                            },
                        }
                    ]
                },
                "finish_reason": None,
            }
        ]
    }
    return f"data: {json.dumps(chunk)}"


def _make_finish_tool_calls_line() -> str:
    """Build an SSE line with finish_reason='tool_calls'."""
    chunk = {
        "choices": [
            {
                "delta": {},
                "finish_reason": "tool_calls",
            }
        ]
    }
    return f"data: {json.dumps(chunk)}"


# Strategy for non-empty text content (no newlines to keep SSE lines valid)
text_content_st = st.text(
    alphabet=st.characters(
        whitelist_categories=("L", "N", "P", "S", "Z"),
        blacklist_characters="\r\n",
    ),
    min_size=1,
    max_size=50,
)

# Strategy for tool call IDs
tool_call_id_st = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N")),
    min_size=4,
    max_size=20,
).map(lambda s: f"call_{s}")

# Strategy for function names
function_name_st = st.from_regex(r"[a-z][a-z0-9_]{1,20}", fullmatch=True)

# Strategy for argument delta fragments (valid partial JSON strings)
arg_delta_st = st.text(
    alphabet=st.characters(
        whitelist_categories=("L", "N", "P", "S"),
        blacklist_characters="\r\n",
    ),
    min_size=1,
    max_size=30,
)

# Strategy for a list of text delta contents
text_deltas_st = st.lists(text_content_st, min_size=1, max_size=10)

# Strategy for argument delta fragments for a single tool call
arg_deltas_st = st.lists(arg_delta_st, min_size=1, max_size=5)


# --- Property Tests ---


class TestSSEChunkParsingProducesCorrectTypedEvents:
    """Property 13: SSE chunk parsing produces correct typed events.

    **Validates: Requirements 5.2**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(text_contents=text_deltas_st)
    def test_text_deltas_emitted_for_content_chunks(
        self,
        text_contents: list[str],
    ) -> None:
        """TextDelta events are emitted for each content chunk, and their
        concatenation matches the original content."""
        processor = StreamProcessor()

        # Feed all text delta lines
        for content in text_contents:
            line = _make_text_delta_line(content)
            processor.feed_line(line)

        # Feed DONE
        processor.feed_line("data: [DONE]")

        events = processor.events

        # Filter TextDelta events
        text_events = [e for e in events if isinstance(e, TextDelta)]

        # 1. TextDelta events are emitted for content chunks
        assert len(text_events) == len(text_contents)

        # 2. Each TextDelta.text matches the corresponding content
        for event, expected_content in zip(text_events, text_contents):
            assert event.text == expected_content

        # 3. The concatenation of all TextDelta.text values matches the original content
        concatenated = "".join(e.text for e in text_events)
        expected_full = "".join(text_contents)
        assert concatenated == expected_full

        # Stream should end with StreamEnd
        assert isinstance(events[-1], StreamEnd)
        assert events[-1].reason == "stop"

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        tool_call_id=tool_call_id_st,
        function_name=function_name_st,
        arg_fragments=arg_deltas_st,
    )
    def test_tool_call_events_emitted_in_correct_order(
        self,
        tool_call_id: str,
        function_name: str,
        arg_fragments: list[str],
    ) -> None:
        """ToolCallStart/ToolCallDelta/ToolCallComplete events are emitted in
        correct order for tool call chunks."""
        processor = StreamProcessor()

        # Feed tool call start
        start_line = _make_tool_call_start_line(0, tool_call_id, function_name)
        processor.feed_line(start_line)

        # Feed argument deltas
        for frag in arg_fragments:
            delta_line = _make_tool_call_delta_line(0, frag)
            processor.feed_line(delta_line)

        # Feed finish_reason="tool_calls" to trigger ToolCallComplete
        finish_line = _make_finish_tool_calls_line()
        processor.feed_line(finish_line)

        events = processor.events

        # Filter tool-related events
        tool_events = [
            e
            for e in events
            if isinstance(e, (ToolCallStart, ToolCallDelta, ToolCallComplete))
        ]

        # Must have at least: Start + N deltas + Complete
        assert len(tool_events) >= 2  # at minimum Start + Complete

        # 1. First tool event is ToolCallStart
        assert isinstance(tool_events[0], ToolCallStart)
        assert tool_events[0].tool_call_id == tool_call_id
        assert tool_events[0].function_name == function_name

        # 2. Middle events are ToolCallDelta (in order)
        delta_events = [e for e in tool_events if isinstance(e, ToolCallDelta)]
        assert len(delta_events) == len(arg_fragments)
        for event, expected_frag in zip(delta_events, arg_fragments):
            assert event.tool_call_id == tool_call_id
            assert event.arguments_delta == expected_frag

        # 3. Last tool event is ToolCallComplete
        assert isinstance(tool_events[-1], ToolCallComplete)
        assert tool_events[-1].tool_call_id == tool_call_id
        assert tool_events[-1].function_name == function_name

        # 4. ToolCallComplete.arguments matches the concatenation of all argument deltas
        expected_args = "".join(arg_fragments)
        assert tool_events[-1].arguments == expected_args

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        text_contents=text_deltas_st,
        tool_call_id=tool_call_id_st,
        function_name=function_name_st,
        arg_fragments=arg_deltas_st,
    )
    def test_mixed_text_and_tool_calls_preserve_order(
        self,
        text_contents: list[str],
        tool_call_id: str,
        function_name: str,
        arg_fragments: list[str],
    ) -> None:
        """For a sequence of text deltas followed by tool calls, events are
        emitted in the correct order: TextDeltas first, then tool call events."""
        processor = StreamProcessor()

        # Phase 1: Feed text deltas
        for content in text_contents:
            line = _make_text_delta_line(content)
            processor.feed_line(line)

        # Phase 2: Feed tool call start + deltas
        start_line = _make_tool_call_start_line(0, tool_call_id, function_name)
        processor.feed_line(start_line)

        for frag in arg_fragments:
            delta_line = _make_tool_call_delta_line(0, frag)
            processor.feed_line(delta_line)

        # Finish with tool_calls reason
        finish_line = _make_finish_tool_calls_line()
        processor.feed_line(finish_line)

        events = processor.events

        # Verify text deltas come first
        text_events = [e for e in events if isinstance(e, TextDelta)]
        tool_start_events = [e for e in events if isinstance(e, ToolCallStart)]
        tool_delta_events = [e for e in events if isinstance(e, ToolCallDelta)]
        tool_complete_events = [e for e in events if isinstance(e, ToolCallComplete)]

        # All text deltas present
        assert len(text_events) == len(text_contents)
        concatenated_text = "".join(e.text for e in text_events)
        assert concatenated_text == "".join(text_contents)

        # Tool call events present
        assert len(tool_start_events) == 1
        assert len(tool_delta_events) == len(arg_fragments)
        assert len(tool_complete_events) == 1

        # Verify ordering: all TextDelta indices < ToolCallStart index
        text_indices = [i for i, e in enumerate(events) if isinstance(e, TextDelta)]
        start_index = next(i for i, e in enumerate(events) if isinstance(e, ToolCallStart))
        delta_indices = [i for i, e in enumerate(events) if isinstance(e, ToolCallDelta)]
        complete_index = next(
            i for i, e in enumerate(events) if isinstance(e, ToolCallComplete)
        )

        # Text deltas before tool call start
        assert all(ti < start_index for ti in text_indices)

        # Tool call deltas between start and complete
        assert all(start_index < di < complete_index for di in delta_indices)

        # ToolCallComplete.arguments matches concatenation
        assert tool_complete_events[0].arguments == "".join(arg_fragments)
