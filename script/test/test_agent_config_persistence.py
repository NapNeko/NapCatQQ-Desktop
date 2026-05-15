# -*- coding: utf-8 -*-
"""ConfigPersistence 单元测试.

测试配置持久化模块的核心行为: 
- 文件不存在时返回默认配置
- 文件损坏时重命名为 .bak 并返回默认配置
- 正常加载和保存 round-trip
- 写入失败时记录错误但不抛异常
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.core.agent.agent_def import AgentDefinition
from src.core.agent.config_persistence import ConfigData, ConfigPersistence
from src.core.agent.permission import PermissionRule
from src.core.agent.provider import ModelEntry, Provider


@pytest.fixture
def config_path(tmp_path: Path) -> Path:
    """提供临时配置文件路径."""
    return tmp_path / "config" / "agent_config.json"


@pytest.fixture
def persistence(config_path: Path) -> ConfigPersistence:
    """提供 ConfigPersistence 实例."""
    return ConfigPersistence(config_path)


def _make_provider(provider_id: str = "test-provider") -> Provider:
    """创建测试用 Provider."""
    return Provider(
        provider_id=provider_id,
        name="Test Provider",
        api_base_url="https://api.example.com/v1",
        api_key_ref="TEST_API_KEY",
        models=[
            ModelEntry(
                model_id="test-model",
                display_name="Test Model",
                max_tokens=4096,
                supports_streaming=True,
                supports_tools=True,
            )
        ],
    )


def _make_agent(name: str = "test-agent") -> AgentDefinition:
    """创建测试用 AgentDefinition."""
    return AgentDefinition(
        name=name,
        description="A test agent",
        mode="primary",
        system_prompt="You are a test agent.",
        tool_ids=["file_read", "file_write"],
        permission_rules=[
            PermissionRule(pattern="*", target="*", action="allow")
        ],
    )


class TestConfigDataModel:
    """ConfigData 模型测试."""

    def test_default_config_data(self) -> None:
        """默认 ConfigData 应有空列表和 None 字段."""
        config = ConfigData()
        assert config.providers == []
        assert config.active_provider_id is None
        assert config.active_model_id is None
        assert config.agents == []

    def test_config_data_with_values(self) -> None:
        """ConfigData 应正确存储 Provider 和 Agent."""
        provider = _make_provider()
        agent = _make_agent()
        config = ConfigData(
            providers=[provider],
            active_provider_id="test-provider",
            active_model_id="test-model",
            agents=[agent],
        )
        assert len(config.providers) == 1
        assert config.providers[0].provider_id == "test-provider"
        assert config.active_provider_id == "test-provider"
        assert config.active_model_id == "test-model"
        assert len(config.agents) == 1
        assert config.agents[0].name == "test-agent"


class TestConfigPersistenceLoad:
    """ConfigPersistence.load() 测试."""

    def test_load_file_not_exists_returns_default(
        self, persistence: ConfigPersistence, caplog: pytest.LogCaptureFixture
    ) -> None:
        """文件不存在时应返回默认配置并记录 warning."""
        with caplog.at_level("WARNING"):
            config = persistence.load()

        assert config == ConfigData()
        assert "配置文件不存在" in caplog.text

    def test_load_valid_file(
        self, persistence: ConfigPersistence, config_path: Path
    ) -> None:
        """正常文件应正确加载."""
        provider = _make_provider()
        agent = _make_agent()
        data = ConfigData(
            providers=[provider],
            active_provider_id="test-provider",
            active_model_id="test-model",
            agents=[agent],
        )
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(data.model_dump_json(indent=2), encoding="utf-8")

        loaded = persistence.load()

        assert loaded.active_provider_id == "test-provider"
        assert loaded.active_model_id == "test-model"
        assert len(loaded.providers) == 1
        assert loaded.providers[0].provider_id == "test-provider"
        assert len(loaded.agents) == 1
        assert loaded.agents[0].name == "test-agent"

    def test_load_corrupted_json_renames_to_bak(
        self, persistence: ConfigPersistence, config_path: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        """JSON 损坏时应重命名为 .bak 并返回默认配置."""
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("{invalid json content!!!", encoding="utf-8")

        with caplog.at_level("WARNING"):
            config = persistence.load()

        assert config == ConfigData()
        assert "配置文件损坏或验证失败" in caplog.text
        # 原文件应被重命名
        assert not config_path.exists()
        backup_path = config_path.with_suffix(".json.bak")
        assert backup_path.exists()
        assert backup_path.read_text(encoding="utf-8") == "{invalid json content!!!"

    def test_load_invalid_schema_renames_to_bak(
        self, persistence: ConfigPersistence, config_path: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        """JSON 有效但 schema 验证失败时应重命名为 .bak 并返回默认配置."""
        config_path.parent.mkdir(parents=True, exist_ok=True)
        # providers 应该是列表, 这里给一个无效的字符串
        invalid_data = {"providers": "not_a_list", "agents": []}
        config_path.write_text(json.dumps(invalid_data), encoding="utf-8")

        with caplog.at_level("WARNING"):
            config = persistence.load()

        assert config == ConfigData()
        assert "配置文件损坏或验证失败" in caplog.text
        assert not config_path.exists()
        backup_path = config_path.with_suffix(".json.bak")
        assert backup_path.exists()


class TestConfigPersistenceSave:
    """ConfigPersistence.save() 测试."""

    def test_save_creates_file(
        self, persistence: ConfigPersistence, config_path: Path
    ) -> None:
        """save 应创建文件及父目录."""
        provider = _make_provider()
        config = ConfigData(
            providers=[provider],
            active_provider_id="test-provider",
            active_model_id="test-model",
        )

        persistence.save(config)

        assert config_path.exists()
        loaded_data = json.loads(config_path.read_text(encoding="utf-8"))
        assert loaded_data["active_provider_id"] == "test-provider"
        assert loaded_data["active_model_id"] == "test-model"
        assert len(loaded_data["providers"]) == 1

    def test_save_overwrites_existing(
        self, persistence: ConfigPersistence, config_path: Path
    ) -> None:
        """save 应覆盖已有文件."""
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text("{}", encoding="utf-8")

        config = ConfigData(active_provider_id="new-provider")
        persistence.save(config)

        loaded_data = json.loads(config_path.read_text(encoding="utf-8"))
        assert loaded_data["active_provider_id"] == "new-provider"

    def test_save_io_error_does_not_raise(
        self, tmp_path: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        """写入失败时应记录 error 但不抛异常."""
        # 使用一个目录作为文件路径来触发 I/O 错误
        dir_as_file = tmp_path / "a_directory"
        dir_as_file.mkdir()
        # 尝试写入一个目录路径 (这会失败) 
        persistence = ConfigPersistence(dir_as_file)

        with caplog.at_level("ERROR"):
            # 不应抛出异常
            persistence.save(ConfigData())

        assert "配置文件写入失败" in caplog.text


class TestConfigPersistenceRoundTrip:
    """save → load round-trip 测试."""

    def test_round_trip_preserves_data(
        self, persistence: ConfigPersistence
    ) -> None:
        """保存后加载应得到相同数据."""
        provider = _make_provider("deepseek")
        agent = _make_agent("napcat-plugin-dev")
        original = ConfigData(
            providers=[provider],
            active_provider_id="deepseek",
            active_model_id="test-model",
            agents=[agent],
        )

        persistence.save(original)
        loaded = persistence.load()

        assert loaded.active_provider_id == original.active_provider_id
        assert loaded.active_model_id == original.active_model_id
        assert len(loaded.providers) == len(original.providers)
        assert loaded.providers[0].provider_id == original.providers[0].provider_id
        assert loaded.providers[0].name == original.providers[0].name
        assert len(loaded.agents) == len(original.agents)
        assert loaded.agents[0].name == original.agents[0].name
        assert loaded.agents[0].mode == original.agents[0].mode

    def test_round_trip_empty_config(
        self, persistence: ConfigPersistence
    ) -> None:
        """空配置的 round-trip."""
        original = ConfigData()
        persistence.save(original)
        loaded = persistence.load()

        assert loaded == original
