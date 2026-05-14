# -*- coding: utf-8 -*-
"""Property-based tests for Session module.

Property 12: Message serialization round-trip.
Validates: Requirements 9.5

For any valid Message instance (including those with tool_calls), serializing to JSON
and deserializing the result SHALL produce a Message instance with identical values
for all fields including id, role, content, timestamp, tool_call_id, tool_name, and
tool_calls.
"""

from __future__ import annotations

from datetime import datetime, timezone

# 第三方库导入
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

# 项目内模块导入
from src.core.agent.session import Message, ToolCallInfo


# --- Strategies ---

# Generate random UUIDs for id
message_id = st.uuids()

# role from ("user", "assistant", "tool")
message_role = st.sampled_from(["user", "assistant", "tool"])

# Non-empty content (1-1000 chars)
message_content = st.text(
    alphabet=st.characters(
        whitelist_categories=("L", "N", "P", "Z", "S"),
        blacklist_characters="\x00",
    ),
    min_size=1,
    max_size=1000,
)

# UTC timestamps
message_timestamp = st.datetimes(
    min_value=datetime(2000, 1, 1),
    max_value=datetime(2099, 12, 31),
    timezones=st.just(timezone.utc),
)

# Optional tool_call_id
optional_tool_call_id = st.one_of(
    st.none(),
    st.text(
        alphabet=st.characters(whitelist_categories=("L", "N", "Pd")),
        min_size=1,
        max_size=64,
    ),
)

# Optional tool_name
optional_tool_name = st.one_of(
    st.none(),
    st.from_regex(r"[a-z][a-z0-9_]{0,63}", fullmatch=True),
)

# ToolCallInfo strategy
tool_call_info = st.builds(
    ToolCallInfo,
    id=st.text(
        alphabet=st.characters(whitelist_categories=("L", "N", "Pd")),
        min_size=1,
        max_size=64,
    ),
    function_name=st.from_regex(r"[a-z][a-z0-9_]{0,63}", fullmatch=True),
    arguments=st.sampled_from([
        '{}',
        '{"path": "src/index.ts"}',
        '{"content": "hello", "path": "/tmp/test.txt"}',
        '{"query": "def main", "max_results": 10}',
    ]),
)

# Optional tool_calls list (for assistant messages)
optional_tool_calls = st.one_of(
    st.none(),
    st.lists(tool_call_info, min_size=1, max_size=5),
)


class TestMessageSerializationRoundTrip:
    """Property 12: Message serialization round-trip.

    **Validates: Requirements 9.5**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        msg_id=message_id,
        role=message_role,
        content=message_content,
        timestamp=message_timestamp,
        tool_call_id=optional_tool_call_id,
        tool_name=optional_tool_name,
        tool_calls=optional_tool_calls,
    )
    def test_message_round_trip(
        self,
        msg_id,
        role,
        content,
        timestamp,
        tool_call_id,
        tool_name,
        tool_calls,
    ) -> None:
        """For any valid Message, serialize → deserialize produces identical instance.

        **Validates: Requirements 9.5**
        """
        # Only assistant messages should have tool_calls
        if role != "assistant":
            tool_calls = None

        message = Message(
            id=msg_id,
            role=role,
            content=content,
            timestamp=timestamp,
            tool_call_id=tool_call_id,
            tool_name=tool_name,
            tool_calls=tool_calls,
        )

        # Serialize to JSON
        json_str = message.model_dump_json()

        # Deserialize from JSON
        restored = Message.model_validate_json(json_str)

        # All fields must be identical
        assert restored.id == message.id, (
            f"id mismatch: {restored.id} != {message.id}"
        )
        assert restored.role == message.role, (
            f"role mismatch: {restored.role} != {message.role}"
        )
        assert restored.content == message.content, (
            f"content mismatch: {restored.content!r} != {message.content!r}"
        )
        assert restored.timestamp == message.timestamp, (
            f"timestamp mismatch: {restored.timestamp} != {message.timestamp}"
        )
        assert restored.tool_call_id == message.tool_call_id, (
            f"tool_call_id mismatch: {restored.tool_call_id} != {message.tool_call_id}"
        )
        assert restored.tool_name == message.tool_name, (
            f"tool_name mismatch: {restored.tool_name} != {message.tool_name}"
        )
        assert restored.tool_calls == message.tool_calls, (
            f"tool_calls mismatch: {restored.tool_calls} != {message.tool_calls}"
        )

        # Also verify full model equality
        assert restored == message, (
            f"Full model equality failed:\n  original={message}\n  restored={restored}"
        )
