# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/errors.py.

验证所有 Agent 异常类的实例化, 属性携带, 继承关系和 str() 表示. 
"""

# 标准库导入
from uuid import UUID, uuid4

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.agent.errors import (
    AgentError,
    DuplicateProviderError,
    DuplicateToolError,
    ModelNotFoundError,
    NoActiveProviderError,
    PermissionDeniedError,
    SessionCorruptedError,
    SessionNotFoundError,
    StreamError,
    ValidationError,
)


class TestAgentErrorBase:
    """AgentError 基类测试."""

    def test_is_exception(self) -> None:
        assert issubclass(AgentError, Exception)

    def test_instantiate_with_message(self) -> None:
        err = AgentError("something went wrong")
        assert str(err) == "something went wrong"


class TestValidationError:
    """ValidationError 测试."""

    def test_instantiate_with_attributes(self) -> None:
        err = ValidationError(field="temperature", reason="must be between 0.0 and 2.0")
        assert err.field == "temperature"
        assert err.reason == "must be between 0.0 and 2.0"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(ValidationError, AgentError)

    def test_str_contains_field_and_reason(self) -> None:
        err = ValidationError(field="api_key", reason="cannot be empty")
        msg = str(err)
        assert "api_key" in msg
        assert "cannot be empty" in msg

    def test_can_be_caught_as_agent_error(self) -> None:
        with pytest.raises(AgentError):
            raise ValidationError(field="name", reason="too long")


class TestNoActiveProviderError:
    """NoActiveProviderError 测试."""

    def test_instantiate_no_args(self) -> None:
        err = NoActiveProviderError()
        assert isinstance(err, NoActiveProviderError)

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(NoActiveProviderError, AgentError)

    def test_str_is_descriptive(self) -> None:
        err = NoActiveProviderError()
        msg = str(err)
        assert "provider" in msg.lower()


class TestDuplicateProviderError:
    """DuplicateProviderError 测试."""

    def test_instantiate_with_provider_id(self) -> None:
        err = DuplicateProviderError(provider_id="openai-main")
        assert err.provider_id == "openai-main"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(DuplicateProviderError, AgentError)

    def test_str_contains_provider_id(self) -> None:
        err = DuplicateProviderError(provider_id="deepseek-v2")
        assert "deepseek-v2" in str(err)


class TestModelNotFoundError:
    """ModelNotFoundError 测试."""

    def test_instantiate_with_attributes(self) -> None:
        err = ModelNotFoundError(model_id="gpt-5", provider_id="openai")
        assert err.model_id == "gpt-5"
        assert err.provider_id == "openai"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(ModelNotFoundError, AgentError)

    def test_str_contains_model_and_provider(self) -> None:
        err = ModelNotFoundError(model_id="claude-4", provider_id="anthropic")
        msg = str(err)
        assert "claude-4" in msg
        assert "anthropic" in msg


class TestDuplicateToolError:
    """DuplicateToolError 测试."""

    def test_instantiate_with_tool_id(self) -> None:
        err = DuplicateToolError(tool_id="file_read")
        assert err.tool_id == "file_read"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(DuplicateToolError, AgentError)

    def test_str_contains_tool_id(self) -> None:
        err = DuplicateToolError(tool_id="shell_exec")
        assert "shell_exec" in str(err)


class TestSessionNotFoundError:
    """SessionNotFoundError 测试."""

    def test_instantiate_with_uuid(self) -> None:
        sid = uuid4()
        err = SessionNotFoundError(session_id=sid)
        assert err.session_id == sid

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(SessionNotFoundError, AgentError)

    def test_str_contains_session_id(self) -> None:
        sid = UUID("12345678-1234-5678-1234-567812345678")
        err = SessionNotFoundError(session_id=sid)
        assert "12345678-1234-5678-1234-567812345678" in str(err)


class TestSessionCorruptedError:
    """SessionCorruptedError 测试."""

    def test_instantiate_with_uuid(self) -> None:
        sid = uuid4()
        err = SessionCorruptedError(session_id=sid)
        assert err.session_id == sid

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(SessionCorruptedError, AgentError)

    def test_str_contains_session_id(self) -> None:
        sid = UUID("abcdefab-cdef-abcd-efab-cdefabcdefab")
        err = SessionCorruptedError(session_id=sid)
        msg = str(err)
        assert "abcdefab-cdef-abcd-efab-cdefabcdefab" in msg
        assert "corrupt" in msg.lower()


class TestStreamError:
    """StreamError 测试."""

    def test_instantiate_with_status_code(self) -> None:
        err = StreamError(status_code=429, message="rate limited")
        assert err.status_code == 429
        assert err.message == "rate limited"

    def test_instantiate_with_none_status_code(self) -> None:
        err = StreamError(status_code=None, message="connection timeout")
        assert err.status_code is None
        assert err.message == "connection timeout"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(StreamError, AgentError)

    def test_str_contains_status_and_message(self) -> None:
        err = StreamError(status_code=500, message="internal server error")
        msg = str(err)
        assert "500" in msg
        assert "internal server error" in msg

    def test_str_with_none_status(self) -> None:
        err = StreamError(status_code=None, message="timeout")
        msg = str(err)
        assert "timeout" in msg
        # Should indicate no status code
        assert "No status" in msg or "None" in msg.lower() or "no" in msg.lower()


class TestPermissionDeniedError:
    """PermissionDeniedError 测试."""

    def test_instantiate_with_attributes(self) -> None:
        err = PermissionDeniedError(tool_id="shell_exec", reason="user denied")
        assert err.tool_id == "shell_exec"
        assert err.reason == "user denied"

    def test_is_agent_error_subclass(self) -> None:
        assert issubclass(PermissionDeniedError, AgentError)

    def test_str_contains_tool_id_and_reason(self) -> None:
        err = PermissionDeniedError(tool_id="file_write", reason="no matching rule")
        msg = str(err)
        assert "file_write" in msg
        assert "no matching rule" in msg


class TestInheritanceHierarchy:
    """验证所有异常类的继承关系."""

    @pytest.mark.parametrize(
        "exc_class",
        [
            ValidationError,
            NoActiveProviderError,
            DuplicateProviderError,
            ModelNotFoundError,
            DuplicateToolError,
            SessionNotFoundError,
            SessionCorruptedError,
            StreamError,
            PermissionDeniedError,
        ],
    )
    def test_all_are_agent_error_subclasses(self, exc_class: type) -> None:
        assert issubclass(exc_class, AgentError)

    @pytest.mark.parametrize(
        "exc_class",
        [
            ValidationError,
            NoActiveProviderError,
            DuplicateProviderError,
            ModelNotFoundError,
            DuplicateToolError,
            SessionNotFoundError,
            SessionCorruptedError,
            StreamError,
            PermissionDeniedError,
        ],
    )
    def test_all_are_exception_subclasses(self, exc_class: type) -> None:
        assert issubclass(exc_class, Exception)
