# -*- coding: utf-8 -*-
"""Agent bootstrap 模块单元测试.

测试 bootstrap_agent_engine 函数的集成逻辑: 
- 内置工具注册
- 默认 Agent 初始化
- 配置持久化恢复
- ToolProvider 接口注册
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any
from unittest.mock import AsyncMock

import pytest

from src.core.agent.bootstrap import (
    bootstrap_agent_engine,
    register_builtin_tools,
    restore_config,
)
from src.core.agent.config_persistence import ConfigData, ConfigPersistence
from src.core.agent.engine import AgentEngine
from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry
from src.core.agent.tool import ToolDefinition, ToolProvider, ToolRegistry, ToolResult

from pydantic import BaseModel


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def tmp_workspace(tmp_path: Path) -> Path:
    """创建临时工作区目录."""
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    return workspace


@pytest.fixture
def tmp_config_dir(tmp_path: Path) -> Path:
    """创建临时配置目录."""
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    return config_dir


class FakeBotManager:
    """Fake BotManagerInterface for testing."""

    def get_status(self) -> dict:
        return {"state": "stopped", "uptime": None, "error": None}

    async def start(self) -> None:
        pass

    async def stop(self) -> None:
        pass

    async def restart(self) -> None:
        pass

    def get_logs(self, lines: int = 50) -> list[str]:
        return []


class FakeWebUIClient:
    """Fake WebUIClientInterface for testing."""

    async def list_plugins(self) -> list[dict[str, Any]]:
        return []

    async def reload_plugin(self, plugin_id: str) -> dict[str, Any]:
        return {"message": "ok"}

    async def get_plugin_config(self, plugin_id: str) -> dict[str, Any]:
        return {}

    async def set_plugin_config(self, plugin_id: str, config: dict[str, Any]) -> dict[str, Any]:
        return {"message": "ok"}

    async def get_bot_info(self) -> dict[str, Any]:
        return {"online": False, "qq": "12345", "nickname": "test", "groups_count": 0}

    async def send_message(self, target_type: str, target_id: int, message: str) -> dict[str, Any]:
        return {"message_id": "msg_001"}


# ---------------------------------------------------------------------------
# Tests: register_builtin_tools
# ---------------------------------------------------------------------------


class TestRegisterBuiltinTools:
    """测试内置工具注册."""

    def test_registers_file_tools(self, tmp_workspace: Path, tmp_config_dir: Path):
        """文件操作工具应被正确注册."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "file_read" in all_tools
        assert "file_write" in all_tools
        assert "file_edit" in all_tools

    def test_registers_search_tools(self, tmp_workspace: Path, tmp_config_dir: Path):
        """搜索工具应被正确注册."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "grep_search" in all_tools
        assert "list_directory" in all_tools

    def test_registers_shell_tool(self, tmp_workspace: Path, tmp_config_dir: Path):
        """Shell 执行工具应被正确注册."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "shell_exec" in all_tools

    def test_registers_context_tool(self, tmp_workspace: Path, tmp_config_dir: Path):
        """上下文文件工具应被正确注册."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "open_context_file" in all_tools

    def test_registers_bot_tools_when_manager_provided(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """提供 BotManager 时应注册 Bot 管理工具."""
        registry = ToolRegistry()
        bot_manager = FakeBotManager()
        register_builtin_tools(
            registry, tmp_workspace, tmp_config_dir, bot_manager=bot_manager
        )

        all_tools = registry.list_all()
        assert "bot_status" in all_tools
        assert "bot_start" in all_tools
        assert "bot_stop" in all_tools
        assert "bot_restart" in all_tools
        assert "bot_logs" in all_tools

    def test_skips_bot_tools_when_no_manager(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """未提供 BotManager 时应跳过 Bot 管理工具."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "bot_status" not in all_tools
        assert "bot_start" not in all_tools

    def test_registers_webui_tools_when_client_provided(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """提供 WebUI 客户端时应注册 WebUI 工具."""
        registry = ToolRegistry()
        webui_client = FakeWebUIClient()
        register_builtin_tools(
            registry, tmp_workspace, tmp_config_dir, webui_client=webui_client
        )

        all_tools = registry.list_all()
        assert "webui_list_plugins" in all_tools
        assert "webui_reload_plugin" in all_tools
        assert "webui_plugin_config" in all_tools
        assert "webui_bot_info" in all_tools
        assert "webui_send_test_message" in all_tools

    def test_skips_webui_tools_when_no_client(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """未提供 WebUI 客户端时应跳过 WebUI 工具."""
        registry = ToolRegistry()
        register_builtin_tools(registry, tmp_workspace, tmp_config_dir)

        all_tools = registry.list_all()
        assert "webui_list_plugins" not in all_tools
        assert "webui_bot_info" not in all_tools

    def test_all_tools_registered_with_full_dependencies(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """提供所有依赖时应注册全部 16 个内置工具."""
        registry = ToolRegistry()
        bot_manager = FakeBotManager()
        webui_client = FakeWebUIClient()
        register_builtin_tools(
            registry,
            tmp_workspace,
            tmp_config_dir,
            bot_manager=bot_manager,
            webui_client=webui_client,
        )

        all_tools = registry.list_all()
        expected_tools = [
            "file_read", "file_write", "file_edit",
            "grep_search", "list_directory",
            "shell_exec",
            "bot_status", "bot_start", "bot_stop", "bot_restart", "bot_logs",
            "webui_list_plugins", "webui_reload_plugin", "webui_plugin_config",
            "webui_bot_info", "webui_send_test_message",
            "open_context_file",
        ]
        for tool_id in expected_tools:
            assert tool_id in all_tools, f"Tool '{tool_id}' not registered"
        assert len(all_tools) == len(expected_tools)


# ---------------------------------------------------------------------------
# Tests: restore_config
# ---------------------------------------------------------------------------


class TestRestoreConfig:
    """测试配置恢复逻辑."""

    def test_restores_providers(self, tmp_config_dir: Path, tmp_workspace: Path):
        """应从配置文件恢复 Provider."""
        # 准备配置文件
        config_data = {
            "providers": [
                {
                    "provider_id": "test-provider",
                    "name": "Test Provider",
                    "api_base_url": "https://api.test.com/v1",
                    "api_key_ref": "TEST_KEY",
                    "models": [
                        {
                            "model_id": "test-model",
                            "display_name": "Test Model",
                            "max_tokens": 4096,
                            "supports_streaming": True,
                            "supports_tools": True,
                        }
                    ],
                }
            ],
            "active_provider_id": "test-provider",
            "active_model_id": "test-model",
            "agents": [],
        }
        config_file = tmp_config_dir / "agent_config.json"
        config_file.write_text(json.dumps(config_data), encoding="utf-8")

        # 创建组件
        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_config_dir / "sessions")
        engine = AgentEngine(provider_registry, tool_registry, session_manager)
        config_persistence = ConfigPersistence(config_file)

        # 恢复配置
        restore_config(config_persistence, provider_registry, engine)

        # 验证
        assert len(provider_registry.list_all()) == 1
        provider, model_config = provider_registry.get_active()
        assert provider.provider_id == "test-provider"
        assert model_config.model_id == "test-model"

    def test_handles_missing_config_file(self, tmp_config_dir: Path):
        """配置文件不存在时应注册默认供应商."""
        config_file = tmp_config_dir / "agent_config.json"
        # 不创建文件

        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_config_dir / "sessions")
        engine = AgentEngine(provider_registry, tool_registry, session_manager)
        config_persistence = ConfigPersistence(config_file)

        # 不应抛出异常
        restore_config(config_persistence, provider_registry, engine)

        # 首次启动时应注册默认供应商列表
        from src.core.agent.default_providers import get_default_providers
        assert len(provider_registry.list_all()) == len(get_default_providers())

    def test_handles_corrupted_config_file(self, tmp_config_dir: Path):
        """配置文件损坏时应注册默认供应商."""
        config_file = tmp_config_dir / "agent_config.json"
        config_file.write_text("not valid json {{{", encoding="utf-8")

        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_config_dir / "sessions")
        engine = AgentEngine(provider_registry, tool_registry, session_manager)
        config_persistence = ConfigPersistence(config_file)

        # 不应抛出异常
        restore_config(config_persistence, provider_registry, engine)

        # 配置损坏时应回退到默认供应商列表
        from src.core.agent.default_providers import get_default_providers
        assert len(provider_registry.list_all()) == len(get_default_providers())

    def test_restores_custom_agents(self, tmp_config_dir: Path):
        """应从配置文件恢复自定义 Agent 定义."""
        config_data = {
            "providers": [],
            "active_provider_id": None,
            "active_model_id": None,
            "agents": [
                {
                    "name": "custom-agent",
                    "description": "A custom agent",
                    "mode": "subagent",
                    "system_prompt": "You are a custom agent.",
                    "tool_ids": ["file_read"],
                    "permission_rules": [
                        {"pattern": "*", "target": "*", "action": "allow"}
                    ],
                }
            ],
        }
        config_file = tmp_config_dir / "agent_config.json"
        config_file.write_text(json.dumps(config_data), encoding="utf-8")

        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_config_dir / "sessions")
        engine = AgentEngine(provider_registry, tool_registry, session_manager)
        config_persistence = ConfigPersistence(config_file)

        restore_config(config_persistence, provider_registry, engine)

        # 验证自定义 Agent 已注册
        agents = engine.list_agents()
        agent_names = [a.name for a in agents]
        assert "custom-agent" in agent_names
        assert "napcat-plugin-dev" in agent_names  # 默认 Agent 仍存在

    def test_skips_default_agent_from_config(self, tmp_config_dir: Path):
        """配置中的 napcat-plugin-dev Agent 应被跳过 (使用内置版本) ."""
        config_data = {
            "providers": [],
            "active_provider_id": None,
            "active_model_id": None,
            "agents": [
                {
                    "name": "napcat-plugin-dev",
                    "description": "Overridden",
                    "mode": "primary",
                    "system_prompt": "overridden prompt",
                    "tool_ids": ["file_read"],
                    "permission_rules": [],
                }
            ],
        }
        config_file = tmp_config_dir / "agent_config.json"
        config_file.write_text(json.dumps(config_data), encoding="utf-8")

        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_config_dir / "sessions")
        engine = AgentEngine(provider_registry, tool_registry, session_manager)
        config_persistence = ConfigPersistence(config_file)

        restore_config(config_persistence, provider_registry, engine)

        # 默认 Agent 应保持原始配置 (不被覆盖) 
        default_agent = engine.get_agent("napcat-plugin-dev")
        assert default_agent.description == "NapCat 插件开发助手"
        assert len(default_agent.tool_ids) > 1  # 完整的 tool_ids 列表


# ---------------------------------------------------------------------------
# Tests: bootstrap_agent_engine
# ---------------------------------------------------------------------------


class TestBootstrapAgentEngine:
    """测试完整的引导流程."""

    def test_returns_agent_engine(self, tmp_workspace: Path, tmp_config_dir: Path):
        """应返回 AgentEngine 实例."""
        engine = bootstrap_agent_engine(tmp_workspace, tmp_config_dir)
        assert isinstance(engine, AgentEngine)

    def test_registers_core_tools(self, tmp_workspace: Path, tmp_config_dir: Path):
        """应注册核心文件/搜索/Shell 工具."""
        engine = bootstrap_agent_engine(tmp_workspace, tmp_config_dir)

        # 通过 engine 的 tool_registry 验证
        # 使用默认 Agent 的 tool_definitions 来验证
        default_agent = engine.get_agent("napcat-plugin-dev")
        tool_defs = engine._get_tool_definitions_for_agent(default_agent)

        tool_names = [td["function"]["name"] for td in tool_defs]
        assert "file_read" in tool_names
        assert "file_write" in tool_names
        assert "file_edit" in tool_names
        assert "grep_search" in tool_names
        assert "list_directory" in tool_names
        assert "shell_exec" in tool_names
        assert "open_context_file" in tool_names

    def test_default_agent_has_complete_tool_ids(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """默认 napcat-plugin-dev Agent 应包含完整的 tool_ids 列表."""
        engine = bootstrap_agent_engine(tmp_workspace, tmp_config_dir)
        default_agent = engine.get_agent("napcat-plugin-dev")

        expected_tool_ids = [
            "file_read", "file_write", "file_edit",
            "grep_search", "list_directory",
            "shell_exec",
            "bot_status", "bot_start", "bot_stop", "bot_restart", "bot_logs",
            "webui_list_plugins", "webui_reload_plugin", "webui_plugin_config",
            "webui_bot_info", "webui_send_test_message",
            "open_context_file",
        ]
        for tool_id in expected_tool_ids:
            assert tool_id in default_agent.tool_ids, (
                f"Tool '{tool_id}' missing from default Agent's tool_ids"
            )

    def test_with_bot_manager(self, tmp_workspace: Path, tmp_config_dir: Path):
        """提供 BotManager 时应注册 Bot 工具."""
        bot_manager = FakeBotManager()
        engine = bootstrap_agent_engine(
            tmp_workspace, tmp_config_dir, bot_manager=bot_manager
        )

        default_agent = engine.get_agent("napcat-plugin-dev")
        tool_defs = engine._get_tool_definitions_for_agent(default_agent)
        tool_names = [td["function"]["name"] for td in tool_defs]
        assert "bot_status" in tool_names

    def test_with_webui_client(self, tmp_workspace: Path, tmp_config_dir: Path):
        """提供 WebUI 客户端时应注册 WebUI 工具."""
        webui_client = FakeWebUIClient()
        engine = bootstrap_agent_engine(
            tmp_workspace, tmp_config_dir, webui_client=webui_client
        )

        default_agent = engine.get_agent("napcat-plugin-dev")
        tool_defs = engine._get_tool_definitions_for_agent(default_agent)
        tool_names = [td["function"]["name"] for td in tool_defs]
        assert "webui_list_plugins" in tool_names

    def test_with_tool_provider(self, tmp_workspace: Path, tmp_config_dir: Path):
        """应支持注册外部 ToolProvider (MCP 扩展点) ."""

        class CustomParams(BaseModel):
            query: str

        class CustomTool(ToolDefinition):
            tool_id = "custom_mcp_tool"
            description = "A custom MCP tool"
            parameters_schema = CustomParams

            async def execute(self, params: BaseModel) -> ToolResult:
                return ToolResult(output="custom result")

        class CustomProvider(ToolProvider):
            def get_tools(self) -> list[ToolDefinition]:
                return [CustomTool()]

        engine = bootstrap_agent_engine(
            tmp_workspace,
            tmp_config_dir,
            tool_providers=[CustomProvider()],
        )

        # 验证自定义工具已注册
        tool = engine._tool_registry.get("custom_mcp_tool")
        assert tool.description == "A custom MCP tool"

    def test_loads_persisted_config(self, tmp_workspace: Path, tmp_config_dir: Path):
        """应加载持久化配置文件."""
        # 预先写入配置文件
        config_data = {
            "providers": [
                {
                    "provider_id": "deepseek",
                    "name": "DeepSeek",
                    "api_base_url": "https://api.deepseek.com/v1",
                    "api_key_ref": "DEEPSEEK_KEY",
                    "models": [
                        {
                            "model_id": "deepseek-chat",
                            "display_name": "DeepSeek Chat",
                            "max_tokens": 8192,
                            "supports_streaming": True,
                            "supports_tools": True,
                        }
                    ],
                }
            ],
            "active_provider_id": "deepseek",
            "active_model_id": "deepseek-chat",
            "agents": [],
        }
        config_file = tmp_config_dir / "agent_config.json"
        config_file.write_text(json.dumps(config_data), encoding="utf-8")

        engine = bootstrap_agent_engine(tmp_workspace, tmp_config_dir)

        # 验证 Provider 已恢复
        provider, model_config = engine._provider_registry.get_active()
        assert provider.provider_id == "deepseek"
        assert model_config.model_id == "deepseek-chat"

    def test_creates_config_dir_if_not_exists(self, tmp_path: Path):
        """配置目录不存在时应自动创建."""
        workspace = tmp_path / "workspace"
        workspace.mkdir()
        config_dir = tmp_path / "nonexistent" / "config"

        engine = bootstrap_agent_engine(workspace, config_dir)

        assert config_dir.exists()
        assert isinstance(engine, AgentEngine)

    def test_creates_session_storage_dir(
        self, tmp_workspace: Path, tmp_config_dir: Path
    ):
        """应创建 session 存储目录."""
        engine = bootstrap_agent_engine(tmp_workspace, tmp_config_dir)

        session_dir = tmp_config_dir / "agent_sessions"
        assert session_dir.exists()


# ---------------------------------------------------------------------------
# Import for SessionManager
# ---------------------------------------------------------------------------
from src.core.agent.session import SessionManager
