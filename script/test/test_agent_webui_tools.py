# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/tools/webui_tools.py.

验证 WebuiListPluginsTool、WebuiReloadPluginTool、WebuiPluginConfigTool、
WebuiBotInfoTool、WebuiSendTestMessageTool 的核心功能，
包括正常调用、连接失败和认证失败的错误处理。
"""

from __future__ import annotations

import asyncio
from typing import Any

import pytest
from pydantic import BaseModel

from src.core.agent.tools.webui_tools import (
    AuthenticationError,
    WebuiBotInfoParams,
    WebuiBotInfoTool,
    WebuiListPluginsParams,
    WebuiListPluginsTool,
    WebuiPluginConfigParams,
    WebuiPluginConfigTool,
    WebuiReloadPluginParams,
    WebuiReloadPluginTool,
    WebuiSendTestMessageParams,
    WebuiSendTestMessageTool,
    WebUIClientInterface,
    _AUTH_ERROR_MSG,
    _CONNECTION_ERROR_MSG,
)


def _run(coro):
    """运行异步协程的辅助函数."""
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# Mock WebUI Client
# ---------------------------------------------------------------------------


class MockWebUIClient:
    """Mock WebUI 客户端，用于测试."""

    def __init__(self) -> None:
        self.plugins: list[dict[str, Any]] = []
        self.bot_info: dict[str, Any] = {}
        self.plugin_configs: dict[str, dict[str, Any]] = {}
        self.sent_messages: list[dict[str, Any]] = []
        self.reload_results: dict[str, dict[str, Any]] = {}
        self.error_to_raise: Exception | None = None

    async def list_plugins(self) -> list[dict[str, Any]]:
        if self.error_to_raise:
            raise self.error_to_raise
        return self.plugins

    async def reload_plugin(self, plugin_id: str) -> dict[str, Any]:
        if self.error_to_raise:
            raise self.error_to_raise
        return self.reload_results.get(plugin_id, {"message": "success"})

    async def get_plugin_config(self, plugin_id: str) -> dict[str, Any]:
        if self.error_to_raise:
            raise self.error_to_raise
        return self.plugin_configs.get(plugin_id, {})

    async def set_plugin_config(self, plugin_id: str, config: dict[str, Any]) -> dict[str, Any]:
        if self.error_to_raise:
            raise self.error_to_raise
        self.plugin_configs[plugin_id] = config
        return {"message": "success"}

    async def get_bot_info(self) -> dict[str, Any]:
        if self.error_to_raise:
            raise self.error_to_raise
        return self.bot_info

    async def send_message(self, target_type: str, target_id: int, message: str) -> dict[str, Any]:
        if self.error_to_raise:
            raise self.error_to_raise
        msg = {"target_type": target_type, "target_id": target_id, "message": message, "message_id": "12345"}
        self.sent_messages.append(msg)
        return {"message_id": "12345"}


@pytest.fixture
def mock_client() -> MockWebUIClient:
    """创建 Mock WebUI 客户端."""
    return MockWebUIClient()


# ---------------------------------------------------------------------------
# WebuiListPluginsTool tests
# ---------------------------------------------------------------------------


class TestWebuiListPluginsTool:
    """WebuiListPluginsTool 单元测试."""

    def test_list_plugins_empty(self, mock_client: MockWebUIClient) -> None:
        """没有插件时应返回提示信息."""
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "没有已安装的插件" in result.output

    def test_list_plugins_with_data(self, mock_client: MockWebUIClient) -> None:
        """有插件时应返回格式化的插件列表."""
        mock_client.plugins = [
            {"id": "test-plugin", "name": "Test Plugin", "version": "1.0.0", "loaded": True, "enabled": True},
            {"id": "another", "name": "Another", "version": "2.1.0", "loaded": False, "enabled": False},
        ]
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "2 个" in result.output
        assert "Test Plugin" in result.output
        assert "test-plugin" in result.output
        assert "已加载" in result.output
        assert "未加载" in result.output
        assert result.metadata is not None
        assert result.metadata["plugin_count"] == 2

    def test_list_plugins_connection_error(self, mock_client: MockWebUIClient) -> None:
        """连接失败应返回 is_error=True 并提示检查 NapCat 运行状态."""
        mock_client.error_to_raise = ConnectionError("Connection refused")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert "NapCat" in result.output
        assert "运行" in result.output

    def test_list_plugins_timeout_error(self, mock_client: MockWebUIClient) -> None:
        """超时应返回 is_error=True 并提示检查 NapCat 运行状态."""
        mock_client.error_to_raise = TimeoutError("Connection timed out")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert "NapCat" in result.output

    def test_list_plugins_auth_error(self, mock_client: MockWebUIClient) -> None:
        """认证失败应返回 is_error=True 并提示检查 token 配置."""
        mock_client.error_to_raise = AuthenticationError("Unauthorized")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert "token" in result.output

    def test_tool_id_and_description(self, mock_client: MockWebUIClient) -> None:
        """验证 tool_id 和 description 属性."""
        tool = WebuiListPluginsTool(webui_client=mock_client)
        assert tool.tool_id == "webui_list_plugins"
        assert len(tool.description) > 0

    def test_parameters_schema_is_pydantic_model(self, mock_client: MockWebUIClient) -> None:
        """验证 parameters_schema 是 pydantic BaseModel 子类."""
        tool = WebuiListPluginsTool(webui_client=mock_client)
        assert issubclass(tool.parameters_schema, BaseModel)


# ---------------------------------------------------------------------------
# WebuiReloadPluginTool tests
# ---------------------------------------------------------------------------


class TestWebuiReloadPluginTool:
    """WebuiReloadPluginTool 单元测试."""

    def test_reload_plugin_success(self, mock_client: MockWebUIClient) -> None:
        """成功热重载插件."""
        mock_client.reload_results["my-plugin"] = {"message": "reloaded"}
        tool = WebuiReloadPluginTool(webui_client=mock_client)
        params = WebuiReloadPluginParams(plugin_id="my-plugin")
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "my-plugin" in result.output
        assert "热重载成功" in result.output

    def test_reload_plugin_connection_error(self, mock_client: MockWebUIClient) -> None:
        """连接失败应返回连接错误提示."""
        mock_client.error_to_raise = ConnectionError("refused")
        tool = WebuiReloadPluginTool(webui_client=mock_client)
        params = WebuiReloadPluginParams(plugin_id="test")
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG

    def test_reload_plugin_auth_error(self, mock_client: MockWebUIClient) -> None:
        """认证失败应返回认证错误提示."""
        mock_client.error_to_raise = AuthenticationError("401")
        tool = WebuiReloadPluginTool(webui_client=mock_client)
        params = WebuiReloadPluginParams(plugin_id="test")
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _AUTH_ERROR_MSG

    def test_tool_id(self, mock_client: MockWebUIClient) -> None:
        """验证 tool_id."""
        tool = WebuiReloadPluginTool(webui_client=mock_client)
        assert tool.tool_id == "webui_reload_plugin"


# ---------------------------------------------------------------------------
# WebuiPluginConfigTool tests
# ---------------------------------------------------------------------------


class TestWebuiPluginConfigTool:
    """WebuiPluginConfigTool 单元测试."""

    def test_read_config(self, mock_client: MockWebUIClient) -> None:
        """读取插件配置."""
        mock_client.plugin_configs["my-plugin"] = {"key": "value", "count": 42}
        tool = WebuiPluginConfigTool(webui_client=mock_client)
        params = WebuiPluginConfigParams(plugin_id="my-plugin", config_data=None)
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "my-plugin" in result.output
        assert "key" in result.output
        assert "value" in result.output
        assert result.metadata is not None
        assert result.metadata["action"] == "read"

    def test_write_config(self, mock_client: MockWebUIClient) -> None:
        """写入插件配置."""
        tool = WebuiPluginConfigTool(webui_client=mock_client)
        new_config = {"setting": "new_value"}
        params = WebuiPluginConfigParams(plugin_id="my-plugin", config_data=new_config)
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "已更新" in result.output
        assert result.metadata is not None
        assert result.metadata["action"] == "write"
        assert mock_client.plugin_configs["my-plugin"] == new_config

    def test_read_config_connection_error(self, mock_client: MockWebUIClient) -> None:
        """读取配置时连接失败."""
        mock_client.error_to_raise = ConnectionError("refused")
        tool = WebuiPluginConfigTool(webui_client=mock_client)
        params = WebuiPluginConfigParams(plugin_id="test", config_data=None)
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG

    def test_write_config_auth_error(self, mock_client: MockWebUIClient) -> None:
        """写入配置时认证失败."""
        mock_client.error_to_raise = AuthenticationError("Unauthorized")
        tool = WebuiPluginConfigTool(webui_client=mock_client)
        params = WebuiPluginConfigParams(plugin_id="test", config_data={"a": 1})
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _AUTH_ERROR_MSG

    def test_tool_id(self, mock_client: MockWebUIClient) -> None:
        """验证 tool_id."""
        tool = WebuiPluginConfigTool(webui_client=mock_client)
        assert tool.tool_id == "webui_plugin_config"


# ---------------------------------------------------------------------------
# WebuiBotInfoTool tests
# ---------------------------------------------------------------------------


class TestWebuiBotInfoTool:
    """WebuiBotInfoTool 单元测试."""

    def test_bot_info_online(self, mock_client: MockWebUIClient) -> None:
        """查询在线 Bot 信息."""
        mock_client.bot_info = {
            "online": True,
            "qq": 123456789,
            "nickname": "TestBot",
            "groups_count": 10,
        }
        tool = WebuiBotInfoTool(webui_client=mock_client)
        params = WebuiBotInfoParams()
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "在线" in result.output
        assert "123456789" in result.output
        assert "TestBot" in result.output
        assert "10" in result.output
        assert result.metadata is not None
        assert result.metadata["online"] is True

    def test_bot_info_offline(self, mock_client: MockWebUIClient) -> None:
        """查询离线 Bot 信息."""
        mock_client.bot_info = {
            "online": False,
            "qq": 987654321,
            "nickname": "OfflineBot",
            "groups_count": 0,
        }
        tool = WebuiBotInfoTool(webui_client=mock_client)
        params = WebuiBotInfoParams()
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "离线" in result.output
        assert result.metadata is not None
        assert result.metadata["online"] is False

    def test_bot_info_connection_error(self, mock_client: MockWebUIClient) -> None:
        """连接失败应返回连接错误提示."""
        mock_client.error_to_raise = TimeoutError("timed out")
        tool = WebuiBotInfoTool(webui_client=mock_client)
        params = WebuiBotInfoParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG

    def test_tool_id(self, mock_client: MockWebUIClient) -> None:
        """验证 tool_id."""
        tool = WebuiBotInfoTool(webui_client=mock_client)
        assert tool.tool_id == "webui_bot_info"


# ---------------------------------------------------------------------------
# WebuiSendTestMessageTool tests
# ---------------------------------------------------------------------------


class TestWebuiSendTestMessageTool:
    """WebuiSendTestMessageTool 单元测试."""

    def test_send_to_group(self, mock_client: MockWebUIClient) -> None:
        """发送消息到群."""
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="group", target_id=12345, message="Hello group!"
        )
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "群" in result.output
        assert "12345" in result.output
        assert len(mock_client.sent_messages) == 1
        assert mock_client.sent_messages[0]["target_type"] == "group"

    def test_send_to_user(self, mock_client: MockWebUIClient) -> None:
        """发送消息到用户."""
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="user", target_id=67890, message="Hello user!"
        )
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert "用户" in result.output
        assert "67890" in result.output

    def test_send_invalid_target_type(self, mock_client: MockWebUIClient) -> None:
        """无效的目标类型应返回错误."""
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="invalid", target_id=123, message="test"
        )
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert "无效" in result.output

    def test_send_connection_error(self, mock_client: MockWebUIClient) -> None:
        """连接失败应返回连接错误提示."""
        mock_client.error_to_raise = ConnectionError("refused")
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="group", target_id=123, message="test"
        )
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG

    def test_send_auth_error(self, mock_client: MockWebUIClient) -> None:
        """认证失败应返回认证错误提示."""
        mock_client.error_to_raise = AuthenticationError("Unauthorized")
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="user", target_id=456, message="test"
        )
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _AUTH_ERROR_MSG

    def test_send_returns_metadata(self, mock_client: MockWebUIClient) -> None:
        """发送成功应返回 metadata."""
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        params = WebuiSendTestMessageParams(
            target_type="group", target_id=111, message="hi"
        )
        result = _run(tool.execute(params))
        assert result.is_error is False
        assert result.metadata is not None
        assert result.metadata["target_type"] == "group"
        assert result.metadata["target_id"] == 111
        assert result.metadata["message_id"] == "12345"

    def test_tool_id(self, mock_client: MockWebUIClient) -> None:
        """验证 tool_id."""
        tool = WebuiSendTestMessageTool(webui_client=mock_client)
        assert tool.tool_id == "webui_send_test_message"


# ---------------------------------------------------------------------------
# Error handling edge cases
# ---------------------------------------------------------------------------


class TestErrorHandling:
    """错误处理边界情况测试."""

    def test_unauthorized_in_exception_message(self, mock_client: MockWebUIClient) -> None:
        """异常消息中包含 Unauthorized 应被识别为认证错误."""
        mock_client.error_to_raise = Exception("NapCat returned Unauthorized")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _AUTH_ERROR_MSG

    def test_connection_refused_in_exception_message(self, mock_client: MockWebUIClient) -> None:
        """异常消息中包含 connection refused 应被识别为连接错误."""
        mock_client.error_to_raise = Exception("Connection refused by host")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG

    def test_unknown_error(self, mock_client: MockWebUIClient) -> None:
        """未知错误应返回通用错误信息."""
        mock_client.error_to_raise = ValueError("something unexpected")
        tool = WebuiListPluginsTool(webui_client=mock_client)
        params = WebuiListPluginsParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert "WebUI API 调用失败" in result.output
        assert "ValueError" in result.output

    def test_os_error_treated_as_connection_error(self, mock_client: MockWebUIClient) -> None:
        """OSError 应被视为连接错误."""
        mock_client.error_to_raise = OSError("Network is unreachable")
        tool = WebuiBotInfoTool(webui_client=mock_client)
        params = WebuiBotInfoParams()
        result = _run(tool.execute(params))
        assert result.is_error is True
        assert result.output == _CONNECTION_ERROR_MSG


# ---------------------------------------------------------------------------
# Protocol compliance test
# ---------------------------------------------------------------------------


class TestWebUIClientProtocol:
    """WebUIClientInterface 协议合规性测试."""

    def test_mock_client_satisfies_protocol(self, mock_client: MockWebUIClient) -> None:
        """MockWebUIClient 应满足 WebUIClientInterface 协议."""
        assert isinstance(mock_client, WebUIClientInterface)
