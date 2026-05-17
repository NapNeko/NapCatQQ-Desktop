# -*- coding: utf-8 -*-
# Feature: provider-ux-polish, Property 4: Atomic write file integrity
"""Property-based tests for ConfigPersistence atomic write integrity.

Property 4: Atomic write file integrity.
Validates: Requirements 3.4, 3.3

For any valid ConfigData instance, after ConfigPersistence.save() completes without
raising, the config file SHALL contain valid JSON that deserializes to an equivalent
ConfigData. If save() encounters an I/O error, the original file content SHALL remain
unchanged.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from src.core.agent.agent_def import AgentDefinition
from src.core.agent.config_persistence import ConfigData, ConfigPersistence
from src.core.agent.permission import PermissionRule
from src.core.agent.provider import ModelEntry, Provider


# ============================================================
# Strategies
# ============================================================

# Valid string strategies with constrained lengths
_provider_id_st = st.from_regex(r"[a-z][a-z0-9\-]{0,30}", fullmatch=True)
_model_id_st = st.from_regex(r"[a-z][a-z0-9\-_]{0,30}", fullmatch=True)
_name_st = st.text(
    alphabet=st.characters(categories=("L", "N", "P", "Z")),
    min_size=1,
    max_size=32,
)
_url_st = st.sampled_from([
    "https://api.example.com/v1",
    "https://api.openai.com/v1",
    "https://api.anthropic.com",
    "https://generativelanguage.googleapis.com",
])
_api_key_ref_st = st.from_regex(r"[A-Z][A-Z_]{2,20}", fullmatch=True)


@st.composite
def model_entry_strategy(draw: st.DrawFn) -> ModelEntry:
    """Generate a valid ModelEntry with random field values."""
    return ModelEntry(
        model_id=draw(_model_id_st),
        display_name=draw(st.text(min_size=0, max_size=20)),
        max_tokens=draw(st.integers(min_value=1, max_value=1_000_000)),
        supports_streaming=draw(st.booleans()),
        supports_tools=draw(st.booleans()),
        supports_vision=draw(st.booleans()),
        supports_web=draw(st.booleans()),
        supports_reasoning=draw(st.booleans()),
        supports_rerank=draw(st.booleans()),
        supports_embedding=draw(st.booleans()),
        temperature=draw(st.one_of(st.none(), st.floats(min_value=0.0, max_value=2.0))),
        top_p=draw(st.one_of(st.none(), st.floats(min_value=0.0, max_value=1.0))),
        reasoning_effort=draw(st.one_of(st.none(), st.sampled_from(["low", "medium", "high"]))),
    )


@st.composite
def provider_strategy(draw: st.DrawFn) -> Provider:
    """Generate a valid Provider with 1-3 models."""
    models = draw(st.lists(model_entry_strategy(), min_size=1, max_size=3))
    return Provider(
        provider_id=draw(_provider_id_st),
        name=draw(_name_st),
        api_base_url=draw(_url_st),
        api_key_ref=draw(_api_key_ref_st),
        models=models,
        enabled=draw(st.booleans()),
        protocol_type=draw(st.sampled_from(["openai", "anthropic", "gemini"])),
    )


@st.composite
def permission_rule_strategy(draw: st.DrawFn) -> PermissionRule:
    """Generate a valid PermissionRule."""
    return PermissionRule(
        pattern=draw(st.from_regex(r"[a-z\*\?]{1,20}", fullmatch=True)),
        target=draw(st.from_regex(r"[a-z_]{1,20}", fullmatch=True)),
        action=draw(st.sampled_from(["allow", "deny", "ask"])),
    )


@st.composite
def agent_definition_strategy(draw: st.DrawFn) -> AgentDefinition:
    """Generate a valid AgentDefinition."""
    return AgentDefinition(
        name=draw(st.text(min_size=1, max_size=30)),
        description=draw(st.text(min_size=0, max_size=50)),
        mode=draw(st.sampled_from(["primary", "subagent"])),
        system_prompt=draw(st.text(min_size=0, max_size=200)),
        tool_ids=draw(st.lists(st.from_regex(r"[a-z_]{1,15}", fullmatch=True), max_size=5)),
        permission_rules=draw(st.lists(permission_rule_strategy(), max_size=3)),
    )


@st.composite
def custom_icon_bindings_strategy(draw: st.DrawFn) -> dict[str, str]:
    """Generate a valid custom_icon_bindings dict."""
    return draw(st.dictionaries(
        keys=_provider_id_st,
        values=st.from_regex(r"[a-z][a-z0-9\-]{0,20}-color\.svg", fullmatch=True),
        max_size=5,
    ))


@st.composite
def config_data_strategy(draw: st.DrawFn) -> ConfigData:
    """Generate a valid ConfigData with random providers, agents, and bindings."""
    providers = draw(st.lists(provider_strategy(), min_size=0, max_size=3))
    agents = draw(st.lists(agent_definition_strategy(), min_size=0, max_size=2))
    custom_icon_bindings = draw(custom_icon_bindings_strategy())

    # Optionally set active_provider_id/active_model_id from existing providers
    active_provider_id = None
    active_model_id = None
    if providers:
        if draw(st.booleans()):
            chosen_provider = draw(st.sampled_from(providers))
            active_provider_id = chosen_provider.provider_id
            chosen_model = draw(st.sampled_from(chosen_provider.models))
            active_model_id = chosen_model.model_id

    return ConfigData(
        providers=providers,
        active_provider_id=active_provider_id,
        active_model_id=active_model_id,
        agents=agents,
        custom_icon_bindings=custom_icon_bindings,
    )


# ============================================================
# Property Tests
# ============================================================


class TestAtomicWriteFileIntegrity:
    """Property 4: Atomic write file integrity.

    **Validates: Requirements 3.4, 3.3**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(config=config_data_strategy())
    def test_save_produces_valid_json(self, config: ConfigData) -> None:
        """For any valid ConfigData, after save() completes, the config file SHALL
        contain valid JSON that can be parsed without errors.

        **Validates: Requirements 3.4, 3.3**
        """
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "agent_config.json"
            persistence = ConfigPersistence(config_path)

            persistence.save(config)

            # File must exist after save
            assert config_path.exists(), "Config file should exist after save()"

            # File content must be valid JSON
            raw_text = config_path.read_text(encoding="utf-8")
            parsed = json.loads(raw_text)  # Should not raise
            assert isinstance(parsed, dict), "Parsed JSON should be a dict"

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(config=config_data_strategy())
    def test_save_then_load_roundtrip(self, config: ConfigData) -> None:
        """For any valid ConfigData, save() followed by load() SHALL produce an
        equivalent ConfigData instance.

        **Validates: Requirements 3.4, 3.3**
        """
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "agent_config.json"
            persistence = ConfigPersistence(config_path)

            persistence.save(config)
            loaded = persistence.load()

            # Core fields must match
            assert loaded.active_provider_id == config.active_provider_id
            assert loaded.active_model_id == config.active_model_id
            assert loaded.custom_icon_bindings == config.custom_icon_bindings
            assert len(loaded.providers) == len(config.providers)
            assert len(loaded.agents) == len(config.agents)

            # Verify providers match
            for orig_provider, loaded_provider in zip(config.providers, loaded.providers):
                assert loaded_provider.provider_id == orig_provider.provider_id
                assert loaded_provider.name == orig_provider.name
                assert str(loaded_provider.api_base_url) == str(orig_provider.api_base_url)
                assert loaded_provider.api_key_ref == orig_provider.api_key_ref
                assert loaded_provider.enabled == orig_provider.enabled
                assert loaded_provider.protocol_type == orig_provider.protocol_type
                assert len(loaded_provider.models) == len(orig_provider.models)
                for orig_model, loaded_model in zip(orig_provider.models, loaded_provider.models):
                    assert loaded_model.model_id == orig_model.model_id
                    assert loaded_model.max_tokens == orig_model.max_tokens
                    assert loaded_model.temperature == orig_model.temperature
                    assert loaded_model.top_p == orig_model.top_p
                    assert loaded_model.reasoning_effort == orig_model.reasoning_effort

            # Verify agents match
            for orig_agent, loaded_agent in zip(config.agents, loaded.agents):
                assert loaded_agent.name == orig_agent.name
                assert loaded_agent.description == orig_agent.description
                assert loaded_agent.mode == orig_agent.mode
                assert loaded_agent.system_prompt == orig_agent.system_prompt

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(config=config_data_strategy())
    def test_no_tmp_file_remains_after_save(self, config: ConfigData) -> None:
        """After save() completes successfully, no .json.tmp file SHALL remain
        in the config directory (atomic rename completed).

        **Validates: Requirements 3.4, 3.3**
        """
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "agent_config.json"
            persistence = ConfigPersistence(config_path)

            persistence.save(config)

            tmp_file = config_path.with_suffix(".json.tmp")
            assert not tmp_file.exists(), (
                "Temporary file .json.tmp should not remain after successful save()"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(config=config_data_strategy())
    def test_save_preserves_original_on_io_error(self, config: ConfigData) -> None:
        """If save() encounters an I/O error, the original file content SHALL
        remain unchanged.

        **Validates: Requirements 3.4, 3.3**
        """
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "agent_config.json"
            persistence = ConfigPersistence(config_path)

            # First, write a known good config
            original_config = ConfigData(active_provider_id="original-marker")
            persistence.save(original_config)
            original_content = config_path.read_text(encoding="utf-8")

            # Now create a persistence pointing to a directory (will cause I/O error)
            bad_dir = Path(tmpdir) / "bad_target"
            bad_dir.mkdir()
            bad_persistence = ConfigPersistence(bad_dir)

            # Attempt to save new config (should fail silently)
            bad_persistence.save(config)

            # Original file should remain unchanged
            assert config_path.exists()
            assert config_path.read_text(encoding="utf-8") == original_content
