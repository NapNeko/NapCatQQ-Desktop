# -*- coding: utf-8 -*-
"""[`resolve_backend_for_bot`](src/core/operation/resolver.py) 单元测试 (P2.2)."""

from __future__ import annotations

from unittest.mock import MagicMock

import pytest

from src.core.config.config_model import RUNTIME_TARGET_LOCAL, BotConfig
from src.core.operation import (
    BackendResolutionError,
    LocalBackend,
    reset_local_backend_singleton,
    resolve_backend_for_bot,
)


@pytest.fixture(autouse=True)
def _reset_singleton() -> None:
    """每个用例前后清空 LocalBackend 单例缓存, 避免跨用例污染."""
    reset_local_backend_singleton()
    yield
    reset_local_backend_singleton()


# ==================== 本地路由 ====================
class TestLocalRouting:
    def test_local_target_returns_local_backend(self, config_factory) -> None:
        config = config_factory()
        assert config.bot.runtime_target == RUNTIME_TARGET_LOCAL

        backend = resolve_backend_for_bot(config)
        assert isinstance(backend, LocalBackend)

    def test_local_backend_is_singleton(self, config_factory) -> None:
        config_a = config_factory(qqid=111)
        config_b = config_factory(qqid=222)

        backend_a = resolve_backend_for_bot(config_a)
        backend_b = resolve_backend_for_bot(config_b)
        assert backend_a is backend_b

    def test_accepts_bot_config_directly(self) -> None:
        bot = BotConfig(name="x", QQID=1)
        backend = resolve_backend_for_bot(bot)
        assert isinstance(backend, LocalBackend)


# ==================== 远端路由 ====================
class TestRemoteRouting:
    def test_remote_target_uses_server_manager(self) -> None:
        bot = BotConfig(name="remote-bot", QQID=1, runtime_target="srv-uuid-1")
        fake_remote_backend = object()
        manager = MagicMock()
        manager.get_backend.return_value = fake_remote_backend

        result = resolve_backend_for_bot(bot, server_manager=manager)
        assert result is fake_remote_backend
        manager.get_backend.assert_called_once_with("srv-uuid-1")

    def test_missing_server_manager_raises(self) -> None:
        # 显式不注入 server_manager, 同时通过 monkeypatch 让 creart 单例返回 None
        bot = BotConfig(name="x", QQID=1, runtime_target="srv-uuid-1")
        # 直接传 None: 走 creart 路径; 此时 ServerManager 创建器在测试环境会创建出实例,
        # 因此这里改用 monkeypatch 模拟 creart 不可用场景.
        from src.core.operation import resolver as resolver_module

        original = resolver_module._get_server_manager_singleton
        resolver_module._get_server_manager_singleton = lambda: None
        try:
            with pytest.raises(BackendResolutionError) as exc_info:
                resolve_backend_for_bot(bot)
            assert exc_info.value.stage == "server_manager_missing"
            assert exc_info.value.target == "srv-uuid-1"
        finally:
            resolver_module._get_server_manager_singleton = original

    def test_unknown_server_id_raises_with_stage(self) -> None:
        bot = BotConfig(name="x", QQID=1, runtime_target="missing-id")
        manager = MagicMock()
        manager.get_backend.side_effect = KeyError("missing-id")

        with pytest.raises(BackendResolutionError) as exc_info:
            resolve_backend_for_bot(bot, server_manager=manager)
        assert exc_info.value.stage == "server_not_found"
        assert exc_info.value.target == "missing-id"
        assert "missing-id" in str(exc_info.value)
