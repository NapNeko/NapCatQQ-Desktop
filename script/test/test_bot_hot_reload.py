# -*- coding: utf-8 -*-
"""[`bot_hot_reload`](src/core/runtime/bot_hot_reload.py) 单元测试.

2026-05-11 问题 2 修复新增配置热推送模块, 把 ``update_config`` 成功后写盘的新配置通过
NapCat / SnowLuma WebUI 接口推送给在跑的 Bot, 触发后端 hot reload, 无需用户重启 Bot.

测试覆盖 ``push_hot_reload`` 的分流逻辑:

- 远端 Bot (``is_remote=True``) → 跳过 (不提交 worker), 返回 ``False``;
- Bot 未在跑 (``BotProcessManager.get_process`` 返回 None) → 跳过, 返回 ``False``;
- 本地 NapCat Bot 在跑 → 提交 worker, 返回 ``True``;
- 本地 SnowLuma Bot 在跑 → 提交 worker, 返回 ``True``.

以及 ``_build_napcat_payload`` / ``_build_snowluma_payload`` 的 payload 格式与上游 schema 一致.
"""
from __future__ import annotations

# 标准库导入
from typing import Any

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication

# 项目内模块导入
import src.core.runtime.bot_hot_reload as hot_reload
from src.core.config.config_model import (
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
    HttpServersConfig,
    WebsocketServersConfig,
)
from src.core.runtime.backend_type import BackendType


def ensure_qapp() -> QApplication:
    """创建或复用 QApplication (HotReloadSignals 需要)."""
    import os
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app  # type: ignore[return-value]


def make_config(
    qqid: int = 114514,
    *,
    backend: BackendType = BackendType.NAPCAT,
    is_remote: bool = False,
) -> Config:
    """构造测试配置."""
    return Config(
        bot=BotConfig(
            name=f"Bot{qqid}",
            QQID=qqid,
            musicSignUrl="https://example.com/music",
            autoRestartSchedule=AutoRestartScheduleConfig(enable=False),
            offlineAutoRestart=False,
            backend_type=backend,
            runtime_target="srv-uuid-1" if is_remote else "local",
        ),
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[],
            websocketClients=[],
            plugins=[],
        ),
        advanced=AdvancedConfig(
            autoStart=False,
            offlineNotice=True,
            parseMultMsg=False,
            packetServer="ws://127.0.0.1:3001",
            enableLocalFile2Url=False,
            fileLog=True,
            consoleLog=False,
            fileLogLevel="debug",
            consoleLogLevel="info",
            o3HookMode=1,
        ),
    )


class FakeProcessManager:
    """``BotProcessManager`` 替身; 仅暴露 ``get_process``."""

    def __init__(self, running_qqids: list[str] | None = None) -> None:
        self._running = set(running_qqids or [])

    def get_process(self, qq_id: str) -> object | None:
        return object() if qq_id in self._running else None


class FakeQThreadPool:
    """``QThreadPool`` 替身; 记录被提交的 runnable, 不真的执行."""

    def __init__(self) -> None:
        self.started: list[Any] = []

    def start(self, runnable: Any) -> None:
        self.started.append(runnable)


@pytest.fixture
def patch_creart_and_threadpool(monkeypatch: pytest.MonkeyPatch):
    """统一 patch creart it() / QThreadPool.globalInstance(), 返回 (pm, pool) 句柄."""
    ensure_qapp()
    process_manager = FakeProcessManager()
    thread_pool = FakeQThreadPool()

    def fake_it(cls: type) -> object:
        # 按 class 名分流, 避免依赖具体 import (避免 BotProcessManager 单例触发其他初始化).
        name = getattr(cls, "__name__", "")
        if name == "BotProcessManager":
            return process_manager
        if name == "PathFunc":
            class _FakePath:
                napcat_path = "/fake/napcat"
            return _FakePath()
        raise RuntimeError(f"unexpected it({cls!r}) in test")

    monkeypatch.setattr(hot_reload, "it", fake_it)
    monkeypatch.setattr(
        "PySide6.QtCore.QThreadPool.globalInstance", lambda: thread_pool
    )
    return process_manager, thread_pool


# ==================== push_hot_reload 分流逻辑 ====================
def test_push_hot_reload_skips_remote_bot(patch_creart_and_threadpool) -> None:
    """远端 Bot (is_remote=True) 应静默跳过, 不提交 worker."""
    _, thread_pool = patch_creart_and_threadpool
    config = make_config(qqid=114514, is_remote=True)
    signals = hot_reload.HotReloadSignals()

    result = hot_reload.push_hot_reload(config, signals)

    assert result is False
    assert thread_pool.started == []


def test_push_hot_reload_skips_when_bot_not_running(
    patch_creart_and_threadpool,
) -> None:
    """Bot 未在跑 (manager.get_process 返回 None) 时, 跳过且不提交 worker."""
    pm, thread_pool = patch_creart_and_threadpool
    # pm._running 默认为空, 即没有任何在跑的 Bot
    config = make_config(qqid=114514)
    signals = hot_reload.HotReloadSignals()

    result = hot_reload.push_hot_reload(config, signals)

    assert result is False
    assert thread_pool.started == []


def test_push_hot_reload_submits_worker_when_napcat_running(
    patch_creart_and_threadpool,
) -> None:
    """NapCat Bot 在跑时, 应提交一个 NapCat backend 的 worker 到 QThreadPool."""
    pm, thread_pool = patch_creart_and_threadpool
    pm._running.add("114514")  # 标记为在跑

    config = make_config(qqid=114514, backend=BackendType.NAPCAT)
    signals = hot_reload.HotReloadSignals()

    result = hot_reload.push_hot_reload(config, signals)

    assert result is True
    assert len(thread_pool.started) == 1
    worker = thread_pool.started[0]
    assert worker._qq_id == "114514"
    assert worker._backend == BackendType.NAPCAT


def test_push_hot_reload_submits_worker_when_snowluma_running(
    patch_creart_and_threadpool,
) -> None:
    """SnowLuma Bot 在跑时, 应提交一个 SnowLuma backend 的 worker."""
    pm, thread_pool = patch_creart_and_threadpool
    pm._running.add("223344")

    config = make_config(qqid=223344, backend=BackendType.SNOWLUMA)
    signals = hot_reload.HotReloadSignals()

    result = hot_reload.push_hot_reload(config, signals)

    assert result is True
    assert len(thread_pool.started) == 1
    worker = thread_pool.started[0]
    assert worker._qq_id == "223344"
    assert worker._backend == BackendType.SNOWLUMA


# ==================== _build_napcat_payload / _build_snowluma_payload ====================
def test_build_napcat_payload_shape() -> None:
    """NapCat payload 应含 ``network`` (与 SL ``networks`` 复数不同) +
    ``musicSignUrl`` + ``enableLocalFile2Url`` + ``parseMultMsg``.

    与 NapCat ``onebot/config.ts:OneBotConfigSchema`` 字段一致.
    """
    config = make_config(qqid=114514)
    config.connect.httpServers.append(
        HttpServersConfig(name="http-main", host="127.0.0.1", port=3000)
    )

    payload = hot_reload._build_napcat_payload(config)

    assert "network" in payload
    assert "musicSignUrl" in payload
    assert "enableLocalFile2Url" in payload
    assert "parseMultMsg" in payload
    # network.httpServers 应保留 HTTP server 配置
    assert len(payload["network"]["httpServers"]) == 1
    assert payload["network"]["httpServers"][0]["name"] == "http-main"


def test_build_snowluma_payload_shape() -> None:
    """SnowLuma payload 应含 ``networks`` (复数, 与 SL ``OneBotConfig`` 一致) +
    ``musicSignUrl``; 4 个网络数组 key 都存在."""
    config = make_config(qqid=114514)
    config.connect.websocketServers.append(
        WebsocketServersConfig(name="ws-main", host="127.0.0.1", port=3001)
    )

    payload = hot_reload._build_snowluma_payload(config)

    assert "networks" in payload
    assert "musicSignUrl" in payload
    # SnowLuma networks 命名 (httpServers, httpClients, wsServers, wsClients)
    assert "wsServers" in payload["networks"]
    assert "httpServers" in payload["networks"]
    assert "httpClients" in payload["networks"]
    assert "wsClients" in payload["networks"]
    # WS server 应保留
    assert len(payload["networks"]["wsServers"]) == 1


def test_build_snowluma_payload_uses_fallback_when_no_servers() -> None:
    """SnowLuma 全空网络时应走 fallback (与 ``makeDefaultOneBotConfig`` 等价)."""
    config = make_config(qqid=114514)
    # connect 全空 (make_config 默认就是)

    payload = hot_reload._build_snowluma_payload(config)

    # fallback 网络 4 个 key 都应存在 (即使是空数组也好)
    assert "httpServers" in payload["networks"]
    assert "httpClients" in payload["networks"]
    assert "wsServers" in payload["networks"]
    assert "wsClients" in payload["networks"]
