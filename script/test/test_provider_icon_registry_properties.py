# -*- coding: utf-8 -*-
"""属性测试: Provider UX Polish - Property 2: Icon registry alias resolution.

使用 hypothesis 验证 ProviderIconRegistry 的别名解析行为:
- 通过 canonical provider_id 或别名查询返回相同路径
- 未注册 id 返回 None

# Feature: provider-ux-polish, Property 2: Icon registry alias resolution

测试文件: tests/core/agent/test_provider_icon_registry_properties.py
框架: pytest + hypothesis
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

import pytest
from hypothesis import given, settings, assume
from hypothesis import strategies as st

# Ensure project root is on sys.path
PROJECT_ROOT = Path(__file__).resolve().parents[3]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from src.core.agent.config_persistence import ConfigData, ConfigPersistence
from src.core.agent.provider_icon_registry import (
    ProviderIconRegistry,
    _PROVIDER_ALIASES,
)


# --- Hypothesis Strategies ---

# Strategy for picking a registered alias from the actual _PROVIDER_ALIASES dict
registered_alias_st = st.sampled_from(list(_PROVIDER_ALIASES.keys()))

# Strategy for generating unregistered provider_ids that won't match any alias or direct file
unregistered_id_st = st.from_regex(
    r"zzz_unregistered_[a-z0-9]{1,20}", fullmatch=True
)


# --- Helper to build a registry with all canonical icon files ---


def _make_registry(tmp_dir: Path) -> tuple[ProviderIconRegistry, Path, set[str]]:
    """Create a ProviderIconRegistry with SVG files for all canonical icon_ids.

    Returns:
        Tuple of (registry, icons_dir, canonical_ids_set).
    """
    icons_dir = tmp_dir / "provider_icons"
    icons_dir.mkdir(exist_ok=True)

    # Collect all unique canonical icon_ids from the alias map
    canonical_ids = set(_PROVIDER_ALIASES.values())

    # Create SVG files for each canonical icon_id
    for icon_id in canonical_ids:
        svg_file = icons_dir / f"{icon_id}-color.svg"
        if not svg_file.exists():
            svg_file.write_text(f"<svg>{icon_id}</svg>", encoding="utf-8")

    # Create a config persistence with empty config
    config_path = tmp_dir / "agent_config.json"
    persistence = ConfigPersistence(config_path)
    persistence.save(ConfigData())

    registry = ProviderIconRegistry(icons_dir, persistence)
    return registry, icons_dir, canonical_ids


# =============================================================================
# Property 2: Icon registry alias resolution
# =============================================================================


class TestProperty2IconRegistryAliasResolution:
    """Property 2: Icon registry alias resolution.

    # Feature: provider-ux-polish, Property 2: Icon registry alias resolution

    **Validates: Requirements 2.2, 2.4**
    """

    @given(alias=registered_alias_st)
    @settings(max_examples=100)
    def test_alias_resolves_to_same_path_as_canonical(self, alias: str) -> None:
        """# Feature: provider-ux-polish, Property 2: Icon registry alias resolution

        **Validates: Requirements 2.2, 2.4**

        For any registered alias, querying with the alias returns the same path
        as querying with the canonical icon_id.
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            registry, icons_dir, canonical_ids = _make_registry(Path(tmp_dir))
            canonical_id = _PROVIDER_ALIASES[alias]

            # Query via alias
            alias_result = registry.resolve_icon_path(alias)

            # Query via canonical id
            canonical_result = registry.resolve_icon_path(canonical_id)

            # Both should return a non-None path
            assert alias_result is not None, (
                f"Alias '{alias}' should resolve to a path but got None"
            )
            assert canonical_result is not None, (
                f"Canonical id '{canonical_id}' should resolve to a path but got None"
            )

            # Both should resolve to the same file path
            assert alias_result == canonical_result, (
                f"Alias '{alias}' resolved to {alias_result}, "
                f"but canonical '{canonical_id}' resolved to {canonical_result}"
            )

    @given(unregistered_id=unregistered_id_st)
    @settings(max_examples=100)
    def test_unregistered_id_returns_none(self, unregistered_id: str) -> None:
        """# Feature: provider-ux-polish, Property 2: Icon registry alias resolution

        **Validates: Requirements 2.2, 2.4**

        For any provider_id that has no alias, no direct SVG match, and no custom
        binding, resolve_icon_path returns None.
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            registry, icons_dir, canonical_ids = _make_registry(Path(tmp_dir))

            # Ensure the generated id is truly unregistered
            assume(unregistered_id.lower().strip() not in _PROVIDER_ALIASES)
            assume(unregistered_id.lower().strip() not in canonical_ids)
            assume(
                not (icons_dir / f"{unregistered_id.lower().strip()}-color.svg").exists()
            )

            result = registry.resolve_icon_path(unregistered_id)
            assert result is None, (
                f"Unregistered id '{unregistered_id}' should return None but got {result}"
            )

    @given(alias=registered_alias_st)
    @settings(max_examples=100)
    def test_alias_resolution_returns_existing_file(self, alias: str) -> None:
        """# Feature: provider-ux-polish, Property 2: Icon registry alias resolution

        **Validates: Requirements 2.2, 2.4**

        For any registered alias, the resolved path points to an existing file
        with the expected naming convention {icon_id}-color.svg.
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            registry, icons_dir, canonical_ids = _make_registry(Path(tmp_dir))
            canonical_id = _PROVIDER_ALIASES[alias]

            result = registry.resolve_icon_path(alias)

            assert result is not None
            assert result.is_file(), (
                f"Resolved path {result} should be an existing file"
            )
            assert result.name == f"{canonical_id}-color.svg", (
                f"Expected filename '{canonical_id}-color.svg' but got '{result.name}'"
            )


# =============================================================================
# Property 3: Custom icon binding persistence round-trip
# =============================================================================


# Strategy for generating valid provider_id strings (non-empty, alphanumeric + hyphens)
provider_id_st = st.from_regex(r"[a-z][a-z0-9\-]{0,30}", fullmatch=True)

# Strategy for generating valid icon_filename strings (matching {name}-color.svg pattern)
icon_filename_st = st.from_regex(r"[a-z][a-z0-9\-]{0,20}-color\.svg", fullmatch=True)

# Strategy for generating custom_bindings dictionaries
custom_bindings_st = st.dictionaries(
    keys=provider_id_st,
    values=icon_filename_st,
    min_size=0,
    max_size=20,
)


class TestProperty3CustomIconBindingPersistenceRoundTrip:
    """Property 3: Custom icon binding persistence round-trip.

    # Feature: provider-ux-polish, Property 3: Custom icon binding persistence round-trip

    **Validates: Requirements 2.7**
    """

    @given(bindings=custom_bindings_st)
    @settings(max_examples=100)
    def test_custom_bindings_persist_and_reload_consistently(
        self, bindings: dict[str, str]
    ) -> None:
        """# Feature: provider-ux-polish, Property 3: Custom icon binding persistence round-trip

        **Validates: Requirements 2.7**

        For any set of custom icon bindings (provider_id → icon_filename pairs),
        persisting them to the config file and reloading SHALL produce an identical mapping.
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            config_path = tmp_path / "agent_config.json"
            icons_dir = tmp_path / "provider_icons"
            icons_dir.mkdir(exist_ok=True)

            # Create a fresh ConfigPersistence and save initial config with bindings
            persistence = ConfigPersistence(config_path)
            config = ConfigData(custom_icon_bindings=bindings)
            persistence.save(config)

            # Create a new ConfigPersistence instance (simulating app restart)
            persistence_reloaded = ConfigPersistence(config_path)

            # Create ProviderIconRegistry which loads bindings from config
            registry = ProviderIconRegistry(icons_dir, persistence_reloaded)

            # The internal _custom_bindings should match the original
            assert registry._custom_bindings == bindings, (
                f"Round-trip failed: saved {bindings}, "
                f"but reloaded {registry._custom_bindings}"
            )

    @given(bindings=custom_bindings_st)
    @settings(max_examples=100)
    def test_set_custom_binding_persists_each_entry(
        self, bindings: dict[str, str]
    ) -> None:
        """# Feature: provider-ux-polish, Property 3: Custom icon binding persistence round-trip

        **Validates: Requirements 2.7**

        For any set of custom icon bindings, calling set_custom_binding() for each
        entry and then creating a new registry from the same config file SHALL
        produce a registry with an identical mapping.
        """
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_path = Path(tmp_dir)
            config_path = tmp_path / "agent_config.json"
            icons_dir = tmp_path / "provider_icons"
            icons_dir.mkdir(exist_ok=True)

            # Start with empty config
            persistence = ConfigPersistence(config_path)
            persistence.save(ConfigData())

            # Create registry and set each binding individually
            registry = ProviderIconRegistry(icons_dir, persistence)
            for provider_id, icon_filename in bindings.items():
                registry.set_custom_binding(provider_id, icon_filename)

            # Simulate app restart: create new persistence and registry
            persistence_reloaded = ConfigPersistence(config_path)
            registry_reloaded = ProviderIconRegistry(icons_dir, persistence_reloaded)

            # All bindings should be present after reload
            assert registry_reloaded._custom_bindings == bindings, (
                f"set_custom_binding round-trip failed: expected {bindings}, "
                f"but got {registry_reloaded._custom_bindings}"
            )
