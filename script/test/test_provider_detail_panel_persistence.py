# -*- coding: utf-8 -*-
"""单元测试: ProviderDetailPanel / EditModelDialog 持久化验证.

验证以下行为：
- ProviderDetailPanel._save_provider_changes() 调用后 ConfigPersistence.save() 被调用
- EditModelDialog.save() 调用后 ConfigPersistence.save() 被调用

由于这些是 PySide6 UI 组件，测试通过 mock 持久化层来验证调用关系，
不需要实际实例化 Qt 窗口组件。

Requirements: 3.1, 3.2
"""

from __future__ import annotations

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

# Ensure project root is on sys.path
PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))


class TestProviderDetailPanelPersistence:
    """验证 ProviderDetailPanel 在 update_provider 后调用 ConfigPersistence.save()。

    Requirements: 3.1
    """

    @patch("src.ui.page.setup_page.sub_page.provider_panel.provider_detail_panel.it")
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.save"
    )
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.load"
    )
    def test_persist_config_called_after_save_provider_changes(
        self, mock_load, mock_save, mock_it
    ):
        """_persist_config() 应调用 ConfigPersistence.save()。

        Validates: Requirement 3.1 - WHEN ProviderDetailPanel 成功调用
        ProviderRegistry.update_provider() 更新供应商配置后,
        THE ProviderDetailPanel SHALL 立即调用 ConfigPersistence.save()
        """
        from src.core.agent.config_persistence import ConfigData, ConfigPersistence
        from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry

        # Setup mock registry and path_func
        mock_registry = MagicMock(spec=ProviderRegistry)
        mock_registry.list_all.return_value = []

        mock_path_func = MagicMock()
        mock_path_func.config_dir_path = Path("/tmp/test_config")

        def it_side_effect(cls):
            if cls == ProviderRegistry:
                return mock_registry
            # PathFunc
            return mock_path_func

        mock_it.side_effect = it_side_effect

        # Setup mock load to return empty ConfigData
        mock_load.return_value = ConfigData()

        # Import and directly call _persist_config on the class
        # We patch the module-level `it` used inside _persist_config
        from src.ui.page.setup_page.sub_page.provider_panel.provider_detail_panel import (
            ProviderDetailPanel,
        )

        # Create a minimal instance by patching __init__ to avoid Qt initialization
        with patch.object(ProviderDetailPanel, "__init__", lambda self, *a, **kw: None):
            panel = ProviderDetailPanel.__new__(ProviderDetailPanel)
            panel._current_provider_id = "test-provider"

            # Call _persist_config directly (this is what _save_provider_changes calls)
            panel._persist_config()

        # Verify ConfigPersistence.save() was called
        mock_save.assert_called_once()

    @patch("src.ui.page.setup_page.sub_page.provider_panel.provider_detail_panel.it")
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.save"
    )
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.load"
    )
    def test_persist_config_syncs_providers_from_registry(
        self, mock_load, mock_save, mock_it
    ):
        """_persist_config() 应将 registry.list_all() 的结果同步到 config_data.providers。

        Validates: Requirement 3.1 - 将完整配置写入 agent_config.json
        """
        from src.core.agent.config_persistence import ConfigData
        from src.core.agent.provider import Provider, ProviderRegistry

        # Setup mock registry with a provider
        test_provider = MagicMock(spec=Provider)
        mock_registry = MagicMock(spec=ProviderRegistry)
        mock_registry.list_all.return_value = [test_provider]

        mock_path_func = MagicMock()
        mock_path_func.config_dir_path = Path("/tmp/test_config")

        def it_side_effect(cls):
            if cls == ProviderRegistry:
                return mock_registry
            return mock_path_func

        mock_it.side_effect = it_side_effect

        config_data = ConfigData()
        mock_load.return_value = config_data

        from src.ui.page.setup_page.sub_page.provider_panel.provider_detail_panel import (
            ProviderDetailPanel,
        )

        with patch.object(ProviderDetailPanel, "__init__", lambda self, *a, **kw: None):
            panel = ProviderDetailPanel.__new__(ProviderDetailPanel)
            panel._current_provider_id = "test-provider"
            panel._persist_config()

        # Verify save was called with config_data that has providers synced
        mock_save.assert_called_once()
        saved_config = mock_save.call_args[0][0]
        assert saved_config.providers == [test_provider]


class TestEditModelDialogPersistence:
    """验证 EditModelDialog.save() 后调用 ConfigPersistence.save()。

    Requirements: 3.2
    """

    @patch("creart.it")
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.save"
    )
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.load"
    )
    def test_persist_config_called_after_edit_model_save(
        self, mock_load, mock_save, mock_it
    ):
        """EditModelDialog._persist_config() 应调用 ConfigPersistence.save()。

        Validates: Requirement 3.2 - WHEN EditModelDialog 保存模型参数变更后,
        THE EditModelDialog SHALL 触发配置持久化流程，将变更写入 agent_config.json
        """
        from src.core.agent.config_persistence import ConfigData
        from src.core.agent.provider import ProviderRegistry

        # Setup mock registry
        mock_registry = MagicMock(spec=ProviderRegistry)
        mock_registry.list_all.return_value = []

        mock_path_func = MagicMock()
        mock_path_func.config_dir_path = Path("/tmp/test_config")

        def it_side_effect(cls):
            if cls == ProviderRegistry:
                return mock_registry
            return mock_path_func

        mock_it.side_effect = it_side_effect
        mock_load.return_value = ConfigData()

        from src.ui.page.agent_page.edit_model_dialog import EditModelDialog

        # Create a minimal instance by patching __init__ to avoid Qt initialization
        with patch.object(EditModelDialog, "__init__", lambda self, *a, **kw: None):
            dialog = EditModelDialog.__new__(EditModelDialog)
            dialog._persist_config()

        # Verify ConfigPersistence.save() was called
        mock_save.assert_called_once()

    @patch("creart.it")
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.save"
    )
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.load"
    )
    def test_edit_model_persist_syncs_providers(
        self, mock_load, mock_save, mock_it
    ):
        """EditModelDialog._persist_config() 应将 providers 列表同步到配置。

        Validates: Requirement 3.2 - 将变更写入 agent_config.json
        """
        from src.core.agent.config_persistence import ConfigData
        from src.core.agent.provider import Provider, ProviderRegistry

        test_provider = MagicMock(spec=Provider)
        mock_registry = MagicMock(spec=ProviderRegistry)
        mock_registry.list_all.return_value = [test_provider]

        mock_path_func = MagicMock()
        mock_path_func.config_dir_path = Path("/tmp/test_config")

        def it_side_effect(cls):
            if cls == ProviderRegistry:
                return mock_registry
            return mock_path_func

        mock_it.side_effect = it_side_effect

        config_data = ConfigData()
        mock_load.return_value = config_data

        from src.ui.page.agent_page.edit_model_dialog import EditModelDialog

        with patch.object(EditModelDialog, "__init__", lambda self, *a, **kw: None):
            dialog = EditModelDialog.__new__(EditModelDialog)
            dialog._persist_config()

        mock_save.assert_called_once()
        saved_config = mock_save.call_args[0][0]
        assert saved_config.providers == [test_provider]


class TestProviderConfigPanelPersistence:
    """验证 ProviderConfigPanel._persist_config() 调用 ConfigPersistence.save()。

    Requirements: 3.1, 3.2
    """

    @patch("src.ui.page.agent_page.provider_config_panel.it")
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.save"
    )
    @patch(
        "src.core.agent.config_persistence.ConfigPersistence.load"
    )
    def test_persist_config_called(self, mock_load, mock_save, mock_it):
        """ProviderConfigPanel._persist_config() 应调用 ConfigPersistence.save()。

        Validates: Requirement 3.1
        """
        from src.core.agent.config_persistence import ConfigData
        from src.core.agent.provider import ProviderRegistry

        mock_registry = MagicMock(spec=ProviderRegistry)
        mock_registry.list_all.return_value = []

        mock_path_func = MagicMock()
        mock_path_func.config_dir_path = Path("/tmp/test_config")

        def it_side_effect(cls):
            if cls == ProviderRegistry:
                return mock_registry
            return mock_path_func

        mock_it.side_effect = it_side_effect
        mock_load.return_value = ConfigData()

        from src.ui.page.agent_page.provider_config_panel import ProviderConfigPanel

        with patch.object(ProviderConfigPanel, "__init__", lambda self, *a, **kw: None):
            panel = ProviderConfigPanel.__new__(ProviderConfigPanel)
            panel._persist_config()

        mock_save.assert_called_once()
