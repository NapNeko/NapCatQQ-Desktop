# -*- coding: utf-8 -*-
"""属性测试: Provider UX Polish - Property 5: ModelEntry field validation.

# Feature: provider-ux-polish, Property 5: ModelEntry field validation

使用 Hypothesis 验证 ModelEntry 字段验证边界：
- temperature 范围 [0.0, 2.0]
- top_p 范围 [0.0, 1.0]
- reasoning_effort 仅接受 "low"/"medium"/"high"/None

测试文件: tests/core/agent/test_provider_ux_properties.py
框架: pytest + hypothesis
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st
from pydantic import ValidationError

# Ensure project root is on sys.path
PROJECT_ROOT = Path(__file__).resolve().parents[3]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from src.core.agent.provider import ModelEntry


# --- Hypothesis Strategies ---

# Valid temperature values: floats in [0.0, 2.0]
valid_temperature_st = st.floats(min_value=0.0, max_value=2.0, allow_nan=False, allow_infinity=False)

# Invalid temperature values: floats outside [0.0, 2.0]
invalid_temperature_below_st = st.floats(max_value=-0.001, allow_nan=False, allow_infinity=False).filter(
    lambda x: x < 0.0
)
invalid_temperature_above_st = st.floats(min_value=2.001, allow_nan=False, allow_infinity=False).filter(
    lambda x: x > 2.0
)

# Valid top_p values: floats in [0.0, 1.0]
valid_top_p_st = st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False)

# Invalid top_p values: floats outside [0.0, 1.0]
invalid_top_p_below_st = st.floats(max_value=-0.001, allow_nan=False, allow_infinity=False).filter(
    lambda x: x < 0.0
)
invalid_top_p_above_st = st.floats(min_value=1.001, allow_nan=False, allow_infinity=False).filter(
    lambda x: x > 1.0
)

# Valid reasoning_effort values
valid_reasoning_effort_st = st.sampled_from(["low", "medium", "high", None])

# Invalid reasoning_effort values: arbitrary strings that are not valid
invalid_reasoning_effort_st = st.text(min_size=1, max_size=32).filter(
    lambda x: x not in ("low", "medium", "high")
)


# =============================================================================
# Property 5: ModelEntry field validation
# =============================================================================


class TestProperty5ModelEntryFieldValidation:
    """Property 5: ModelEntry field validation.

    # Feature: provider-ux-polish, Property 5: ModelEntry field validation

    **Validates: Requirements 4.1, 5.1, 5.2**
    """

    @given(temperature=valid_temperature_st)
    @settings(max_examples=100)
    def test_temperature_accepts_valid_range(self, temperature: float) -> None:
        """Valid temperature values in [0.0, 2.0] are accepted by ModelEntry.

        **Validates: Requirements 5.1**
        """
        entry = ModelEntry(
            model_id="test-model",
            max_tokens=4096,
            temperature=temperature,
        )
        assert entry.temperature == temperature

    @given(top_p=valid_top_p_st)
    @settings(max_examples=100)
    def test_top_p_accepts_valid_range(self, top_p: float) -> None:
        """Valid top_p values in [0.0, 1.0] are accepted by ModelEntry.

        **Validates: Requirements 5.2**
        """
        entry = ModelEntry(
            model_id="test-model",
            max_tokens=4096,
            top_p=top_p,
        )
        assert entry.top_p == top_p

    @given(temperature=invalid_temperature_below_st)
    @settings(max_examples=100)
    def test_temperature_rejects_below_range(self, temperature: float) -> None:
        """Temperature values below 0.0 are rejected by ModelEntry.

        **Validates: Requirements 5.1**
        """
        with pytest.raises(ValidationError):
            ModelEntry(
                model_id="test-model",
                max_tokens=4096,
                temperature=temperature,
            )

    @given(temperature=invalid_temperature_above_st)
    @settings(max_examples=100)
    def test_temperature_rejects_above_range(self, temperature: float) -> None:
        """Temperature values above 2.0 are rejected by ModelEntry.

        **Validates: Requirements 5.1**
        """
        with pytest.raises(ValidationError):
            ModelEntry(
                model_id="test-model",
                max_tokens=4096,
                temperature=temperature,
            )

    @given(top_p=invalid_top_p_below_st)
    @settings(max_examples=100)
    def test_top_p_rejects_below_range(self, top_p: float) -> None:
        """top_p values below 0.0 are rejected by ModelEntry.

        **Validates: Requirements 5.2**
        """
        with pytest.raises(ValidationError):
            ModelEntry(
                model_id="test-model",
                max_tokens=4096,
                top_p=top_p,
            )

    @given(top_p=invalid_top_p_above_st)
    @settings(max_examples=100)
    def test_top_p_rejects_above_range(self, top_p: float) -> None:
        """top_p values above 1.0 are rejected by ModelEntry.

        **Validates: Requirements 5.2**
        """
        with pytest.raises(ValidationError):
            ModelEntry(
                model_id="test-model",
                max_tokens=4096,
                top_p=top_p,
            )

    @given(effort=valid_reasoning_effort_st)
    @settings(max_examples=100)
    def test_reasoning_effort_accepts_valid_values(self, effort: str | None) -> None:
        """reasoning_effort accepts only "low", "medium", "high", or None.

        **Validates: Requirements 4.1**
        """
        entry = ModelEntry(
            model_id="test-model",
            max_tokens=4096,
            reasoning_effort=effort,
        )
        assert entry.reasoning_effort == effort

    @given(effort=invalid_reasoning_effort_st)
    @settings(max_examples=100)
    def test_reasoning_effort_rejects_invalid_values(self, effort: str) -> None:
        """reasoning_effort rejects arbitrary strings not in {"low", "medium", "high"}.

        **Validates: Requirements 4.1**
        """
        with pytest.raises(ValidationError):
            ModelEntry(
                model_id="test-model",
                max_tokens=4096,
                reasoning_effort=effort,
            )

    @given(
        temperature=st.one_of(valid_temperature_st, st.none()),
        top_p=st.one_of(valid_top_p_st, st.none()),
        effort=valid_reasoning_effort_st,
    )
    @settings(max_examples=100)
    def test_combined_valid_fields(
        self, temperature: float | None, top_p: float | None, effort: str | None
    ) -> None:
        """Any combination of valid temperature, top_p, and reasoning_effort is accepted.

        **Validates: Requirements 4.1, 5.1, 5.2**
        """
        entry = ModelEntry(
            model_id="test-model",
            max_tokens=4096,
            temperature=temperature,
            top_p=top_p,
            reasoning_effort=effort,
        )
        assert entry.temperature == temperature
        assert entry.top_p == top_p
        assert entry.reasoning_effort == effort


# --- Additional Strategies for Property 8 ---

model_id_st = st.from_regex(r"[a-z][a-z0-9\-\.]{0,127}", fullmatch=True)

model_entry_full_st = st.builds(
    ModelEntry,
    model_id=model_id_st,
    display_name=st.text(max_size=64),
    max_tokens=st.integers(min_value=1, max_value=1_000_000),
    temperature=st.one_of(st.none(), st.floats(min_value=0.0, max_value=2.0, allow_nan=False, allow_infinity=False)),
    top_p=st.one_of(st.none(), st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False)),
    reasoning_effort=st.sampled_from([None, "low", "medium", "high"]),
    supports_reasoning=st.booleans(),
    supports_streaming=st.booleans(),
    supports_tools=st.booleans(),
    supports_vision=st.booleans(),
)


# =============================================================================
# Feature: provider-ux-polish, Property 8: Backward-compatible deserialization
# =============================================================================


class TestProperty8BackwardCompatibleDeserialization:
    """Property 8: Backward-compatible deserialization.

    # Feature: provider-ux-polish, Property 8: Backward-compatible deserialization

    For any valid ModelEntry JSON representation with temperature and/or top_p
    fields randomly omitted, deserialization SHALL succeed and the missing fields
    SHALL be None, while all other fields retain their original values.

    **Validates: Requirements 5.4**
    """

    @given(
        entry=model_entry_full_st,
        remove_temperature=st.booleans(),
        remove_top_p=st.booleans(),
    )
    @settings(max_examples=100)
    def test_backward_compatible_deserialization(
        self,
        entry: ModelEntry,
        remove_temperature: bool,
        remove_top_p: bool,
    ) -> None:
        """# Feature: provider-ux-polish, Property 8: Backward-compatible deserialization

        **Validates: Requirements 5.4**

        Generate random ModelEntry JSON and randomly remove temperature/top_p fields,
        verify deserialization succeeds and missing fields are None.
        """
        import json

        # Serialize the ModelEntry to a JSON dict
        json_dict = json.loads(entry.model_dump_json())

        # Randomly remove temperature and/or top_p fields
        if remove_temperature and "temperature" in json_dict:
            del json_dict["temperature"]
        if remove_top_p and "top_p" in json_dict:
            del json_dict["top_p"]

        # Deserialization must succeed
        restored = ModelEntry.model_validate(json_dict)

        # Missing fields should be None
        if remove_temperature:
            assert restored.temperature is None
        else:
            assert restored.temperature == entry.temperature

        if remove_top_p:
            assert restored.top_p is None
        else:
            assert restored.top_p == entry.top_p

        # All other fields retain their original values
        assert restored.model_id == entry.model_id
        assert restored.display_name == entry.display_name
        assert restored.max_tokens == entry.max_tokens
        assert restored.reasoning_effort == entry.reasoning_effort
        assert restored.supports_reasoning == entry.supports_reasoning
        assert restored.supports_streaming == entry.supports_streaming
        assert restored.supports_tools == entry.supports_tools
        assert restored.supports_vision == entry.supports_vision


# =============================================================================
# Property 7: ModelConfig parameter resolution priority
# =============================================================================
# Feature: provider-ux-polish, Property 7: ModelConfig parameter resolution priority

from src.core.agent.provider import ModelConfig


# Strategy for temperature: either None or a valid float in [0.0, 2.0]
temperature_or_none_st = st.one_of(
    st.none(),
    st.floats(min_value=0.0, max_value=2.0, allow_nan=False, allow_infinity=False),
)

# Strategy for top_p: either None or a valid float in [0.0, 1.0]
top_p_or_none_st = st.one_of(
    st.none(),
    st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False),
)

# Strategy for model_id
model_id_st = st.from_regex(r"[a-z][a-z0-9\-\.]{0,63}", fullmatch=True)

# Strategy for max_tokens
max_tokens_st = st.integers(min_value=1, max_value=1_000_000)

# Strategy for provider_id
provider_id_st = st.from_regex(r"[a-z][a-z0-9\-]{0,63}", fullmatch=True)


class TestProperty7ModelConfigResolutionPriority:
    """Property 7: ModelConfig parameter resolution priority.

    # Feature: provider-ux-polish, Property 7: ModelConfig parameter resolution priority

    For any ModelEntry with an arbitrary combination of None and non-None
    temperature/top_p values, ModelConfig.from_model_entry() SHALL use the
    ModelEntry value when non-None, and the global default (temperature=0.7,
    top_p=1.0) when None.

    **Validates: Requirements 5.3**
    """

    @given(
        model_id=model_id_st,
        max_tokens=max_tokens_st,
        temperature=temperature_or_none_st,
        top_p=top_p_or_none_st,
        provider_id=provider_id_st,
    )
    @settings(max_examples=100)
    def test_from_model_entry_uses_entry_value_when_not_none(
        self,
        model_id: str,
        max_tokens: int,
        temperature: float | None,
        top_p: float | None,
        provider_id: str,
    ) -> None:
        """Feature: provider-ux-polish, Property 7: ModelConfig parameter resolution priority

        **Validates: Requirements 5.3**

        For any ModelEntry with arbitrary None/non-None temperature and top_p,
        from_model_entry() uses entry value when non-None, else global default.
        """
        # Arrange: build a ModelEntry with the generated parameters
        entry = ModelEntry(
            model_id=model_id,
            max_tokens=max_tokens,
            temperature=temperature,
            top_p=top_p,
        )

        # Act: construct ModelConfig from the entry
        config = ModelConfig.from_model_entry(entry, provider_id)

        # Assert: model_id and max_tokens always come from entry
        assert config.model_id == model_id
        assert config.max_tokens == max_tokens
        assert config.provider_id == provider_id

        # Assert: temperature resolution priority
        if temperature is not None:
            assert config.temperature == temperature
        else:
            assert config.temperature == 0.7

        # Assert: top_p resolution priority
        if top_p is not None:
            assert config.top_p == top_p
        else:
            assert config.top_p == 1.0


# =============================================================================
# Feature: provider-ux-polish, Property 6: Reasoning effort protocol mapping
# =============================================================================

from uuid import uuid4
from datetime import datetime, timezone

from src.core.agent.adapters.openai_adapter import OpenAIAdapter
from src.core.agent.adapters.anthropic_adapter import AnthropicAdapter
from src.core.agent.adapters.gemini_adapter import GeminiAdapter
from src.core.agent.provider import Provider, ModelConfig, ModelEntry
from src.core.agent.session import Message

# --- Strategies for Property 6 ---

# reasoning_effort values including None
reasoning_effort_all_st = st.sampled_from([None, "low", "medium", "high"])

# Adapter choice strategy
adapter_choice_st = st.sampled_from(["openai", "anthropic", "gemini"])

# Budget mapping for Anthropic and Gemini
_BUDGET_MAP = {
    "low": 1024,
    "medium": 8192,
    "high": 32768,
}


def _make_provider_and_config(
    model_id: str,
    reasoning_effort: str | None,
    protocol_type: str = "openai",
) -> tuple[Provider, ModelConfig]:
    """Helper to create a Provider and ModelConfig for testing adapters."""
    model_entry = ModelEntry(
        model_id=model_id,
        max_tokens=4096,
        supports_reasoning=True,
        reasoning_effort=reasoning_effort,
    )
    provider = Provider(
        provider_id="test-provider",
        name="Test Provider",
        api_base_url="https://api.example.com",
        api_key_ref="sk-test-key",
        models=[model_entry],
        protocol_type=protocol_type,
    )
    model_config = ModelConfig(
        model_id=model_id,
        provider_id="test-provider",
        temperature=0.7,
        top_p=1.0,
        max_tokens=4096,
    )
    return provider, model_config


def _make_messages() -> list[Message]:
    """Create a minimal message list for build_request calls."""
    return [
        Message(
            id=uuid4(),
            role="user",
            content="Hello",
            timestamp=datetime.now(tz=timezone.utc),
        )
    ]


class TestProperty6ReasoningEffortProtocolMapping:
    """Property 6: Reasoning effort protocol mapping.

    # Feature: provider-ux-polish, Property 6: Reasoning effort protocol mapping

    For any protocol adapter (OpenAI, Anthropic, Gemini) and for any ModelEntry
    with reasoning_effort set to a non-None value, the built request body SHALL
    contain the protocol-specific reasoning parameter with the correct mapped value.
    When reasoning_effort is None, the request body SHALL NOT contain any
    reasoning-related fields.

    **Validates: Requirements 4.5, 4.6, 4.7, 4.8**
    """

    @given(
        reasoning_effort=reasoning_effort_all_st,
        adapter_type=adapter_choice_st,
    )
    @settings(max_examples=100)
    def test_reasoning_effort_protocol_mapping(
        self,
        reasoning_effort: str | None,
        adapter_type: str,
    ) -> None:
        """# Feature: provider-ux-polish, Property 6: Reasoning effort protocol mapping

        **Validates: Requirements 4.5, 4.6, 4.7, 4.8**

        For any adapter × reasoning_effort combination, verify the request body
        contains the correct protocol-specific reasoning parameter mapping.
        """
        messages = _make_messages()
        provider, model_config = _make_provider_and_config(
            model_id="test-model",
            reasoning_effort=reasoning_effort,
            protocol_type=adapter_type,
        )

        if adapter_type == "openai":
            adapter = OpenAIAdapter()
        elif adapter_type == "anthropic":
            adapter = AnthropicAdapter()
        else:
            adapter = GeminiAdapter()

        request = adapter.build_request(messages, [], model_config, provider)
        body = request.body

        if reasoning_effort is None:
            # When None, no reasoning-related fields should be present
            assert "reasoning_effort" not in body, (
                f"OpenAI body should not contain 'reasoning_effort' when None"
            )
            assert "thinking" not in body, (
                f"Anthropic body should not contain 'thinking' when None"
            )
            assert "thinkingConfig" not in body, (
                f"Gemini body should not contain 'thinkingConfig' when None"
            )
        else:
            # When non-None, verify protocol-specific mapping
            if adapter_type == "openai":
                assert "reasoning_effort" in body, (
                    f"OpenAI body must contain 'reasoning_effort' for {reasoning_effort}"
                )
                assert body["reasoning_effort"] == reasoning_effort
            elif adapter_type == "anthropic":
                assert "thinking" in body, (
                    f"Anthropic body must contain 'thinking' for {reasoning_effort}"
                )
                expected_budget = _BUDGET_MAP[reasoning_effort]
                assert body["thinking"]["budget_tokens"] == expected_budget
                assert body["thinking"]["type"] == "enabled"
            else:  # gemini
                assert "thinkingConfig" in body, (
                    f"Gemini body must contain 'thinkingConfig' for {reasoning_effort}"
                )
                expected_budget = _BUDGET_MAP[reasoning_effort]
                assert body["thinkingConfig"]["thinkingBudget"] == expected_budget

    @given(adapter_type=adapter_choice_st)
    @settings(max_examples=100)
    def test_none_reasoning_effort_excludes_all_reasoning_fields(
        self,
        adapter_type: str,
    ) -> None:
        """# Feature: provider-ux-polish, Property 6: Reasoning effort protocol mapping

        **Validates: Requirements 4.8**

        When reasoning_effort is None, no adapter should include reasoning fields.
        """
        messages = _make_messages()
        provider, model_config = _make_provider_and_config(
            model_id="test-model",
            reasoning_effort=None,
            protocol_type=adapter_type,
        )

        if adapter_type == "openai":
            adapter = OpenAIAdapter()
        elif adapter_type == "anthropic":
            adapter = AnthropicAdapter()
        else:
            adapter = GeminiAdapter()

        request = adapter.build_request(messages, [], model_config, provider)
        body = request.body

        # None means no reasoning-related fields at all
        assert "reasoning_effort" not in body
        assert "thinking" not in body
        assert "thinkingConfig" not in body


# =============================================================================
# Feature: provider-ux-polish, Property 1: Model parameter save/load round-trip
# =============================================================================

import tempfile

from src.core.agent.config_persistence import ConfigPersistence, ConfigData


# --- Strategies for Property 1 ---

# Strategy for valid temperature (0.0–2.0) or None
property1_temperature_st = st.one_of(
    st.none(),
    st.floats(min_value=0.0, max_value=2.0, allow_nan=False, allow_infinity=False),
)

# Strategy for valid top_p (0.0–1.0) or None
property1_top_p_st = st.one_of(
    st.none(),
    st.floats(min_value=0.0, max_value=1.0, allow_nan=False, allow_infinity=False),
)

# Strategy for max_tokens (≥1)
property1_max_tokens_st = st.integers(min_value=1, max_value=1_000_000)

# Strategy for reasoning_effort (None|"low"|"medium"|"high")
property1_reasoning_effort_st = st.sampled_from([None, "low", "medium", "high"])

# Strategy for model_id (valid non-empty string up to 128 chars)
property1_model_id_st = st.from_regex(r"[a-z][a-z0-9\-\.]{0,63}", fullmatch=True)

# Strategy for provider_id
property1_provider_id_st = st.from_regex(r"[a-z][a-z0-9\-]{0,63}", fullmatch=True)


class TestProperty1ModelParameterSaveLoadRoundTrip:
    """Property 1: Model parameter save/load round-trip.

    # Feature: provider-ux-polish, Property 1: Model parameter save/load round-trip

    For any valid ModelEntry with arbitrary temperature (0.0–2.0), top_p (0.0–1.0),
    max_tokens (≥1), and reasoning_effort (None|"low"|"medium"|"high") values,
    saving the entry via ConfigPersistence and then loading it back produces
    identical parameter values.

    **Validates: Requirements 1.4, 1.5**
    """

    @given(
        model_id=property1_model_id_st,
        provider_id=property1_provider_id_st,
        temperature=property1_temperature_st,
        top_p=property1_top_p_st,
        max_tokens=property1_max_tokens_st,
        reasoning_effort=property1_reasoning_effort_st,
    )
    @settings(max_examples=100)
    def test_save_load_round_trip_preserves_model_parameters(
        self,
        model_id: str,
        provider_id: str,
        temperature: float | None,
        top_p: float | None,
        max_tokens: int,
        reasoning_effort: str | None,
    ) -> None:
        """# Feature: provider-ux-polish, Property 1: Model parameter save/load round-trip

        **Validates: Requirements 1.4, 1.5**

        Generate random ModelEntry parameters, save via ConfigPersistence,
        load back, and verify all parameter fields match exactly.
        """
        # Arrange: create a ModelEntry with generated parameters
        model_entry = ModelEntry(
            model_id=model_id,
            max_tokens=max_tokens,
            temperature=temperature,
            top_p=top_p,
            reasoning_effort=reasoning_effort,
        )

        # Create a Provider containing the ModelEntry
        provider = Provider(
            provider_id=provider_id,
            name="Test Provider",
            api_base_url="https://api.example.com",
            api_key_ref="sk-test-key",
            models=[model_entry],
        )

        # Build ConfigData with the provider
        config_data = ConfigData(
            providers=[provider],
            active_provider_id=provider_id,
            active_model_id=model_id,
        )

        # Act: save and load using a temporary file
        with tempfile.TemporaryDirectory() as tmp_dir:
            config_path = Path(tmp_dir) / "agent_config.json"
            persistence = ConfigPersistence(config_path)

            # Save
            persistence.save(config_data)

            # Load
            loaded_config = persistence.load()

        # Assert: loaded config has exactly one provider with one model
        assert len(loaded_config.providers) == 1
        loaded_provider = loaded_config.providers[0]
        assert loaded_provider.provider_id == provider_id
        assert len(loaded_provider.models) == 1

        loaded_entry = loaded_provider.models[0]

        # Assert: all model parameters match the original
        assert loaded_entry.model_id == model_id
        assert loaded_entry.max_tokens == max_tokens
        assert loaded_entry.reasoning_effort == reasoning_effort

        # For float fields, compare with exact equality (JSON round-trip preserves values)
        if temperature is None:
            assert loaded_entry.temperature is None
        else:
            assert loaded_entry.temperature == temperature

        if top_p is None:
            assert loaded_entry.top_p is None
        else:
            assert loaded_entry.top_p == top_p
