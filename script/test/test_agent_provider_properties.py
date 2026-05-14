# -*- coding: utf-8 -*-
"""Property-based tests for Provider validation.

Property 2: Provider validation rejects invalid configs with descriptive errors.
Validates: Requirements 1.2, 1.5

For any Provider configuration containing exactly one invalid field (malformed URL,
empty api_key_ref, or empty models list), construction SHALL raise a pydantic
ValidationError whose message contains the name of the invalid field.
"""

# 第三方库导入
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from pydantic import ValidationError as PydanticValidationError

# 项目内模块导入
from src.core.agent.provider import ModelEntry, Provider


# --- Strategies ---

valid_model_entry = st.builds(
    ModelEntry,
    model_id=st.text(
        alphabet=st.characters(whitelist_categories=("L", "N", "Pd")),
        min_size=1,
        max_size=32,
    ),
    display_name=st.text(min_size=0, max_size=64),
    max_tokens=st.integers(min_value=1, max_value=1_000_000),
    supports_streaming=st.booleans(),
    supports_tools=st.booleans(),
)

valid_provider_id = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N", "Pd")),
    min_size=1,
    max_size=64,
)

valid_name = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N", "Z")),
    min_size=1,
    max_size=128,
)

valid_api_key_ref = st.text(
    alphabet=st.characters(whitelist_categories=("L", "N", "Pd")),
    min_size=1,
    max_size=64,
)

valid_models_list = st.lists(valid_model_entry, min_size=1, max_size=5)

valid_url = st.sampled_from([
    "https://api.example.com/v1",
    "https://api.deepseek.com/v1",
    "http://localhost:11434/v1",
    "https://api.openai.com/v1",
    "http://127.0.0.1:8080/api",
])

# Invalid URL strategy: strings that are NOT valid http/https URLs
invalid_url = st.sampled_from([
    "ftp://example.com",
    "not-a-url",
    "://missing-scheme",
    "just-text",
    "ws://websocket.example.com",
    "file:///local/path",
    "",
])


class TestProviderValidationRejectsInvalidConfigs:
    """Property 2: Provider validation rejects invalid configs with descriptive errors.

    **Validates: Requirements 1.2, 1.5**
    """

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        provider_id=valid_provider_id,
        name=valid_name,
        api_base_url=valid_url,
        models=valid_models_list,
    )
    def test_empty_api_key_ref_raises_validation_error(
        self,
        provider_id: str,
        name: str,
        api_base_url: str,
        models: list[ModelEntry],
    ) -> None:
        """Empty api_key_ref → pydantic ValidationError raised containing field name.

        **Validates: Requirements 1.2, 1.5**
        """
        try:
            Provider(
                provider_id=provider_id,
                name=name,
                api_base_url=api_base_url,
                api_key_ref="",
                models=models,
            )
            # Should not reach here
            raise AssertionError("Expected PydanticValidationError for empty api_key_ref")
        except PydanticValidationError as e:
            error_str = str(e)
            assert "api_key_ref" in error_str, (
                f"ValidationError message should contain 'api_key_ref', got: {error_str}"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        provider_id=valid_provider_id,
        name=valid_name,
        api_base_url=valid_url,
        api_key_ref=valid_api_key_ref,
    )
    def test_empty_models_list_raises_validation_error(
        self,
        provider_id: str,
        name: str,
        api_base_url: str,
        api_key_ref: str,
    ) -> None:
        """Empty models list → pydantic ValidationError raised containing field name.

        **Validates: Requirements 1.2, 1.5**
        """
        try:
            Provider(
                provider_id=provider_id,
                name=name,
                api_base_url=api_base_url,
                api_key_ref=api_key_ref,
                models=[],
            )
            raise AssertionError("Expected PydanticValidationError for empty models list")
        except PydanticValidationError as e:
            error_str = str(e)
            assert "models" in error_str, (
                f"ValidationError message should contain 'models', got: {error_str}"
            )

    @settings(max_examples=100, suppress_health_check=[HealthCheck.too_slow])
    @given(
        provider_id=valid_provider_id,
        name=valid_name,
        invalid_url=invalid_url,
        api_key_ref=valid_api_key_ref,
        models=valid_models_list,
    )
    def test_invalid_url_raises_validation_error(
        self,
        provider_id: str,
        name: str,
        invalid_url: str,
        api_key_ref: str,
        models: list[ModelEntry],
    ) -> None:
        """Invalid URL (not http/https) → pydantic ValidationError raised containing field name.

        **Validates: Requirements 1.2, 1.5**
        """
        try:
            Provider(
                provider_id=provider_id,
                name=name,
                api_base_url=invalid_url,
                api_key_ref=api_key_ref,
                models=models,
            )
            raise AssertionError(
                f"Expected PydanticValidationError for invalid URL: {invalid_url}"
            )
        except PydanticValidationError as e:
            error_str = str(e)
            assert "api_base_url" in error_str or "url" in error_str.lower(), (
                f"ValidationError message should reference the URL field, got: {error_str}"
            )
