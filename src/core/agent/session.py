# -*- coding: utf-8 -*-
"""Session 会话管理模块.

定义会话消息 (Message) , 工具调用信息 (ToolCallInfo) , Token 用量 (TokenUsage) , 
会话 (Session) , 会话摘要 (SessionSummary) 数据模型, 以及 SessionManager 会话管理器, 
负责会话的创建, 持久化, 加载, 列表和删除. 
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, Field

from src.core.agent.errors import (
    SessionCorruptedError,
    SessionNotFoundError,
    ValidationError,
)


class ToolCallInfo(BaseModel):
    """assistant 消息中的工具调用信息."""

    id: str
    function_name: str
    arguments: str  # JSON-encoded


class TokenUsage(BaseModel):
    """Token 用量统计."""

    prompt_tokens: int = 0
    completion_tokens: int = 0


class Message(BaseModel):
    """会话消息."""

    id: UUID
    role: Literal["user", "assistant", "tool"]
    content: str = Field(max_length=1_000_000)
    timestamp: datetime  # UTC, ISO 8601
    tool_call_id: str | None = None
    tool_name: str | None = None
    tool_calls: list[ToolCallInfo] | None = None


class Session(BaseModel):
    """对话会话."""

    session_id: UUID
    created_at: datetime
    last_updated: datetime
    messages: list[Message] = []
    agent_name: str
    token_usage: TokenUsage = TokenUsage()


class SessionSummary(BaseModel):
    """会话摘要, 用于列表展示."""

    session_id: UUID
    created_at: datetime
    last_updated: datetime
    agent_name: str


class SessionManager:
    """会话管理器, 负责会话的创建, 持久化, 加载, 列表和删除.

    Args:
        storage_dir: 会话 JSON 文件的存储目录路径.
    """

    def __init__(self, storage_dir: Path) -> None:
        self._storage_dir = storage_dir
        self._storage_dir.mkdir(parents=True, exist_ok=True)

    def _session_file_path(self, session_id: UUID) -> Path:
        """获取指定 session 的 JSON 文件路径."""
        return self._storage_dir / f"{session_id}.json"

    def _persist(self, session: Session) -> None:
        """将 Session 持久化到 JSON 文件."""
        file_path = self._session_file_path(session.session_id)
        data = session.model_dump(mode="json")
        file_path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")

    def create(self, agent_name: str) -> Session:
        """创建新会话.

        Args:
            agent_name: 关联的 Agent 名称.

        Returns:
            新创建的 Session 实例.
        """
        now = datetime.now(timezone.utc)
        session = Session(
            session_id=uuid4(),
            created_at=now,
            last_updated=now,
            messages=[],
            agent_name=agent_name,
            token_usage=TokenUsage(),
        )
        self._persist(session)
        return session

    def get(self, session_id: UUID) -> Session:
        """按 session_id 加载会话.

        Args:
            session_id: 要加载的会话 ID.

        Returns:
            加载的 Session 实例.

        Raises:
            SessionNotFoundError: 如果 session_id 对应的文件不存在.
            SessionCorruptedError: 如果 JSON 文件损坏或无法解析.
        """
        file_path = self._session_file_path(session_id)
        if not file_path.exists():
            raise SessionNotFoundError(session_id)

        try:
            raw = file_path.read_text(encoding="utf-8")
            data = json.loads(raw)
            return Session.model_validate(data)
        except (json.JSONDecodeError, Exception) as exc:
            # pydantic ValidationError 或 JSON 解析错误都视为损坏
            if isinstance(exc, SessionNotFoundError):
                raise
            raise SessionCorruptedError(session_id) from exc

    def append_message(self, session_id: UUID, message: Message) -> None:
        """向会话追加消息.

        验证 role 和 content 非空后追加消息, 更新 last_updated 并持久化. 

        Args:
            session_id: 目标会话 ID.
            message: 要追加的消息.

        Raises:
            ValidationError: 如果 role 无效或 content 为空.
            SessionNotFoundError: 如果 session_id 不存在.
        """
        # 验证 role (pydantic Literal 已约束, 但额外检查空值场景) 
        valid_roles = ("user", "assistant", "tool")
        if message.role not in valid_roles:
            raise ValidationError(
                field="role",
                reason=f"role must be one of {valid_roles}, got '{message.role}'",
            )

        # 验证 content 非空
        if not message.content:
            raise ValidationError(
                field="content",
                reason="content must not be empty",
            )

        session = self.get(session_id)
        session.messages.append(message)
        session.last_updated = datetime.now(timezone.utc)
        self._persist(session)

    def list_sessions(self) -> list[SessionSummary]:
        """列出所有会话摘要, 按 last_updated 降序排列.

        Returns:
            SessionSummary 列表, 按 last_updated 从新到旧排序.
        """
        summaries: list[SessionSummary] = []
        for file_path in self._storage_dir.glob("*.json"):
            try:
                raw = file_path.read_text(encoding="utf-8")
                data = json.loads(raw)
                summary = SessionSummary(
                    session_id=data["session_id"],
                    created_at=data["created_at"],
                    last_updated=data["last_updated"],
                    agent_name=data["agent_name"],
                )
                summaries.append(summary)
            except (json.JSONDecodeError, KeyError, Exception):
                # 跳过损坏的文件
                continue

        # 按 last_updated 降序排列
        summaries.sort(key=lambda s: s.last_updated, reverse=True)
        return summaries

    def delete(self, session_id: UUID) -> None:
        """删除指定会话.

        Args:
            session_id: 要删除的会话 ID.

        Raises:
            SessionNotFoundError: 如果 session_id 对应的文件不存在.
        """
        file_path = self._session_file_path(session_id)
        if not file_path.exists():
            raise SessionNotFoundError(session_id)
        file_path.unlink()
