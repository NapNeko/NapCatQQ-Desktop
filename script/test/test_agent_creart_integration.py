# -*- coding: utf-8 -*-
"""creart 集成冒烟测试.

验证 Agent 模块的 creart Creator 注册正确工作: 
- ProviderRegistryCreator
- ToolRegistryCreator
- SessionManagerCreator
- AgentEngineCreator
- exists_module() 返回 False 时 available() 返回 False

Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6, 8.7
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest

import creart

from src.core.agent import (
    AgentEngineCreator,
    ProviderRegistryCreator,
    SessionManagerCreator,
    ToolRegistryCreator,
)
from src.core.agent.engine import AgentEngine
from src.core.agent.provider import ProviderRegistry
from src.core.agent.session import SessionManager
from src.core.agent.tool import ToolRegistry


# ---------------------------------------------------------------------------
# Tests: Creator available() returns True
# ---------------------------------------------------------------------------


class TestCreatorAvailability:
    """测试各 Creator 的 available() 方法在模块存在时返回 True."""

    def test_provider_registry_creator_available(self):
        """ProviderRegistryCreator.available() 应返回 True.

        Validates: Requirements 8.1, 8.5
        """
        assert ProviderRegistryCreator.available() is True

    def test_tool_registry_creator_available(self):
        """ToolRegistryCreator.available() 应返回 True.

        Validates: Requirements 8.2, 8.5
        """
        assert ToolRegistryCreator.available() is True

    def test_session_manager_creator_available(self):
        """SessionManagerCreator.available() 应返回 True.

        Validates: Requirements 8.3, 8.5
        """
        assert SessionManagerCreator.available() is True

    def test_agent_engine_creator_available(self):
        """AgentEngineCreator.available() 应返回 True.

        Validates: Requirements 8.4, 8.5
        """
        assert AgentEngineCreator.available() is True


# ---------------------------------------------------------------------------
# Tests: creart.it() returns correct instances
# ---------------------------------------------------------------------------


class TestCreartInstantiation:
    """测试 creart.it() 返回正确的实例类型."""

    def test_creart_it_provider_registry(self):
        """creart.it(ProviderRegistry) 应返回 ProviderRegistry 实例.

        Validates: Requirements 8.1
        """
        instance = creart.it(ProviderRegistry)
        assert isinstance(instance, ProviderRegistry)

    def test_creart_it_tool_registry(self):
        """creart.it(ToolRegistry) 应返回 ToolRegistry 实例.

        Validates: Requirements 8.2
        """
        instance = creart.it(ToolRegistry)
        assert isinstance(instance, ToolRegistry)

    def test_creart_it_session_manager(self, tmp_path: Path):
        """creart.it(SessionManager) 应返回 SessionManager 实例.

        注意: SessionManagerCreator 依赖 PathFunc 提供 storage_dir. 
        通过 mock PathFunc 来避免对真实文件系统的依赖. 

        Validates: Requirements 8.3
        """
        # SessionManagerCreator.create() 内部调用 creart.it(PathFunc)
        # 需要 mock PathFunc 以避免依赖真实环境
        mock_path_func = type("MockPathFunc", (), {
            "config_dir_path": tmp_path / "config"
        })()
        (tmp_path / "config").mkdir(parents=True, exist_ok=True)

        # 直接测试 Creator.create() 方法, mock 内部的 creart.it 调用
        with patch("creart.it", return_value=mock_path_func):
            instance = SessionManagerCreator.create(SessionManager)
            assert isinstance(instance, SessionManager)

    def test_creart_it_agent_engine_injects_dependencies(self, tmp_path: Path):
        """creart.it(AgentEngine) 应正确注入 ProviderRegistry, ToolRegistry, SessionManager 依赖.

        Validates: Requirements 8.4, 8.7
        """
        # 准备依赖实例
        provider_registry = ProviderRegistry()
        tool_registry = ToolRegistry()
        session_manager = SessionManager(storage_dir=tmp_path / "sessions")

        # Mock creart.it() 在 AgentEngineCreator.create() 内部的调用
        def mock_it(cls):
            if cls is ProviderRegistry:
                return provider_registry
            elif cls is ToolRegistry:
                return tool_registry
            elif cls is SessionManager:
                return session_manager
            raise ValueError(f"Unexpected class: {cls}")

        with patch("creart.it", side_effect=mock_it):
            engine = AgentEngineCreator.create(AgentEngine)

        assert isinstance(engine, AgentEngine)
        # 验证依赖注入正确
        assert engine._provider_registry is provider_registry
        assert engine._tool_registry is tool_registry
        assert engine._session_manager is session_manager


# ---------------------------------------------------------------------------
# Tests: exists_module() 返回 False 时 available() 返回 False
# ---------------------------------------------------------------------------


class TestCreatorUnavailability:
    """测试 exists_module() 返回 False 时 Creator 的 available() 返回 False.

    Validates: Requirements 8.5, 8.6
    """

    def test_provider_registry_unavailable_when_module_missing(self):
        """exists_module 返回 False 时 ProviderRegistryCreator.available() 应返回 False."""
        with patch("src.core.agent.exists_module", return_value=False):
            assert ProviderRegistryCreator.available() is False

    def test_tool_registry_unavailable_when_module_missing(self):
        """exists_module 返回 False 时 ToolRegistryCreator.available() 应返回 False."""
        with patch("src.core.agent.exists_module", return_value=False):
            assert ToolRegistryCreator.available() is False

    def test_session_manager_unavailable_when_module_missing(self):
        """exists_module 返回 False 时 SessionManagerCreator.available() 应返回 False."""
        with patch("src.core.agent.exists_module", return_value=False):
            assert SessionManagerCreator.available() is False

    def test_agent_engine_unavailable_when_module_missing(self):
        """exists_module 返回 False 时 AgentEngineCreator.available() 应返回 False."""
        with patch("src.core.agent.exists_module", return_value=False):
            assert AgentEngineCreator.available() is False
