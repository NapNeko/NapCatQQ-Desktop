# -*- coding: utf-8 -*-
"""SessionManager 单元测试.

测试 Message, Session, SessionManager 的核心行为: 
- 会话创建, 加载, 删除
- 消息追加与验证
- 列表排序
- 持久化 round-trip
- 错误处理
"""

from __future__ import annotations

import json
import time
from datetime import datetime, timezone
from pathlib import Path
from uuid import UUID, uuid4

import pytest

from src.core.agent.errors import (
    SessionCorruptedError,
    SessionNotFoundError,
    ValidationError,
)
from src.core.agent.session import (
    Message,
    Session,
    SessionManager,
    SessionSummary,
    TokenUsage,
    ToolCallInfo,
)


@pytest.fixture
def storage_dir(tmp_path: Path) -> Path:
    """提供临时存储目录."""
    return tmp_path / "sessions"


@pytest.fixture
def manager(storage_dir: Path) -> SessionManager:
    """提供 SessionManager 实例."""
    return SessionManager(storage_dir)


def _make_message(
    role: str = "user",
    content: str = "hello",
    tool_call_id: str | None = None,
    tool_name: str | None = None,
    tool_calls: list[ToolCallInfo] | None = None,
) -> Message:
    """构造测试用 Message."""
    return Message(
        id=uuid4(),
        role=role,
        content=content,
        timestamp=datetime.now(timezone.utc),
        tool_call_id=tool_call_id,
        tool_name=tool_name,
        tool_calls=tool_calls,
    )


class TestMessage:
    """Message 模型测试."""

    def test_create_user_message(self):
        msg = _make_message(role="user", content="test content")
        assert msg.role == "user"
        assert msg.content == "test content"
        assert isinstance(msg.id, UUID)

    def test_create_assistant_message_with_tool_calls(self):
        tool_calls = [
            ToolCallInfo(id="call_1", function_name="file_read", arguments='{"path": "src/index.ts"}')
        ]
        msg = _make_message(role="assistant", content="Let me read that file", tool_calls=tool_calls)
        assert msg.tool_calls is not None
        assert len(msg.tool_calls) == 1
        assert msg.tool_calls[0].function_name == "file_read"

    def test_create_tool_message(self):
        msg = _make_message(role="tool", content="file content here", tool_call_id="call_1", tool_name="file_read")
        assert msg.role == "tool"
        assert msg.tool_call_id == "call_1"
        assert msg.tool_name == "file_read"

    def test_message_serialization_roundtrip(self):
        tool_calls = [
            ToolCallInfo(id="call_1", function_name="grep_search", arguments='{"pattern": "TODO"}')
        ]
        msg = _make_message(role="assistant", content="searching...", tool_calls=tool_calls)
        data = msg.model_dump(mode="json")
        restored = Message.model_validate(data)
        assert restored.id == msg.id
        assert restored.role == msg.role
        assert restored.content == msg.content
        assert restored.timestamp == msg.timestamp
        assert restored.tool_calls is not None
        assert restored.tool_calls[0].id == "call_1"
        assert restored.tool_calls[0].function_name == "grep_search"


class TestTokenUsage:
    """TokenUsage 模型测试."""

    def test_default_values(self):
        usage = TokenUsage()
        assert usage.prompt_tokens == 0
        assert usage.completion_tokens == 0

    def test_custom_values(self):
        usage = TokenUsage(prompt_tokens=100, completion_tokens=50)
        assert usage.prompt_tokens == 100
        assert usage.completion_tokens == 50


class TestSessionManager:
    """SessionManager 核心功能测试."""

    def test_create_session(self, manager: SessionManager):
        session = manager.create("napcat-plugin-dev")
        assert isinstance(session.session_id, UUID)
        assert session.agent_name == "napcat-plugin-dev"
        assert session.messages == []
        assert session.created_at is not None
        assert session.last_updated is not None
        assert session.created_at == session.last_updated

    def test_create_session_persists_to_file(self, manager: SessionManager, storage_dir: Path):
        session = manager.create("test-agent")
        file_path = storage_dir / f"{session.session_id}.json"
        assert file_path.exists()

    def test_get_session(self, manager: SessionManager):
        session = manager.create("test-agent")
        loaded = manager.get(session.session_id)
        assert loaded.session_id == session.session_id
        assert loaded.agent_name == session.agent_name
        assert loaded.created_at == session.created_at

    def test_get_nonexistent_session_raises(self, manager: SessionManager):
        fake_id = uuid4()
        with pytest.raises(SessionNotFoundError) as exc_info:
            manager.get(fake_id)
        assert exc_info.value.session_id == fake_id

    def test_get_corrupted_session_raises(self, manager: SessionManager, storage_dir: Path):
        session = manager.create("test-agent")
        # 损坏 JSON 文件
        file_path = storage_dir / f"{session.session_id}.json"
        file_path.write_text("not valid json {{{", encoding="utf-8")
        with pytest.raises(SessionCorruptedError) as exc_info:
            manager.get(session.session_id)
        assert exc_info.value.session_id == session.session_id

    def test_append_message(self, manager: SessionManager):
        session = manager.create("test-agent")
        msg = _make_message(role="user", content="hello world")
        manager.append_message(session.session_id, msg)

        loaded = manager.get(session.session_id)
        assert len(loaded.messages) == 1
        assert loaded.messages[0].content == "hello world"
        assert loaded.last_updated >= session.last_updated

    def test_append_message_preserves_order(self, manager: SessionManager):
        session = manager.create("test-agent")
        messages = [
            _make_message(role="user", content=f"message {i}")
            for i in range(5)
        ]
        for msg in messages:
            manager.append_message(session.session_id, msg)

        loaded = manager.get(session.session_id)
        assert len(loaded.messages) == 5
        for i, msg in enumerate(loaded.messages):
            assert msg.content == f"message {i}"

    def test_append_message_empty_content_raises(self, manager: SessionManager):
        session = manager.create("test-agent")
        msg = _make_message(role="user", content="placeholder")
        # Manually set content to empty after creation
        msg.content = ""
        with pytest.raises(ValidationError) as exc_info:
            manager.append_message(session.session_id, msg)
        assert exc_info.value.field == "content"

    def test_append_message_to_nonexistent_session_raises(self, manager: SessionManager):
        msg = _make_message(role="user", content="hello")
        with pytest.raises(SessionNotFoundError):
            manager.append_message(uuid4(), msg)

    def test_list_sessions_empty(self, manager: SessionManager):
        result = manager.list_sessions()
        assert result == []

    def test_list_sessions_returns_summaries(self, manager: SessionManager):
        manager.create("agent-a")
        manager.create("agent-b")
        summaries = manager.list_sessions()
        assert len(summaries) == 2
        assert all(isinstance(s, SessionSummary) for s in summaries)

    def test_list_sessions_sorted_by_last_updated_desc(self, manager: SessionManager):
        s1 = manager.create("agent-1")
        time.sleep(0.01)  # 确保时间戳不同
        s2 = manager.create("agent-2")
        time.sleep(0.01)
        # 更新 s1 使其 last_updated 最新
        msg = _make_message(role="user", content="update")
        manager.append_message(s1.session_id, msg)

        summaries = manager.list_sessions()
        assert len(summaries) == 2
        # s1 应该排在前面 (last_updated 更新) 
        assert summaries[0].session_id == s1.session_id

    def test_delete_session(self, manager: SessionManager, storage_dir: Path):
        session = manager.create("test-agent")
        file_path = storage_dir / f"{session.session_id}.json"
        assert file_path.exists()

        manager.delete(session.session_id)
        assert not file_path.exists()

    def test_delete_nonexistent_session_raises(self, manager: SessionManager):
        with pytest.raises(SessionNotFoundError):
            manager.delete(uuid4())

    def test_session_persistence_roundtrip_with_tool_calls(self, manager: SessionManager):
        session = manager.create("test-agent")

        # 追加带 tool_calls 的 assistant 消息
        tool_calls = [
            ToolCallInfo(id="call_abc", function_name="file_write", arguments='{"path": "a.ts", "content": "x"}')
        ]
        assistant_msg = _make_message(role="assistant", content="Writing file", tool_calls=tool_calls)
        manager.append_message(session.session_id, assistant_msg)

        # 追加 tool 消息
        tool_msg = _make_message(role="tool", content="File written", tool_call_id="call_abc", tool_name="file_write")
        manager.append_message(session.session_id, tool_msg)

        # 重新加载验证
        loaded = manager.get(session.session_id)
        assert len(loaded.messages) == 2
        assert loaded.messages[0].tool_calls is not None
        assert loaded.messages[0].tool_calls[0].id == "call_abc"
        assert loaded.messages[1].tool_call_id == "call_abc"
        assert loaded.messages[1].tool_name == "file_write"

    def test_corrupted_file_preserved(self, manager: SessionManager, storage_dir: Path):
        """验证损坏的文件不被修改."""
        session = manager.create("test-agent")
        file_path = storage_dir / f"{session.session_id}.json"
        corrupted_content = "corrupted data!!!"
        file_path.write_text(corrupted_content, encoding="utf-8")

        with pytest.raises(SessionCorruptedError):
            manager.get(session.session_id)

        # 验证文件内容未被修改
        assert file_path.read_text(encoding="utf-8") == corrupted_content
