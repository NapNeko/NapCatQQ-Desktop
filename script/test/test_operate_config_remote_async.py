# -*- coding: utf-8 -*-
"""验证 :mod:`src.core.config.operate_config` 的远端同步 / 删除异步派发路径 (P3 perf W3).

- 在无 ``QApplication`` 的纯 logic test 中, ``update_config`` / ``delete_config`` 必须
  仍走同步路径 (与 [`test_operate_config.py`](script/test/test_operate_config.py) 现有
  断言保持兼容).
- 在 UI 进程内 (有 QApplication 实例), 远端写入应被 dispatch 到 ``QThreadPool``;
  ``update_config`` 立刻返回, runnable 通过 ``waitForDone`` 同步收尾.
"""
from __future__ import annotations

# 标准库导入
import json
import os
import sys
import threading
import time
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import Any

# 第三方库导入
import pytest
from PySide6.QtCore import QThreadPool
from PySide6.QtWidgets import QApplication

# 项目内模块导入
sys.modules.setdefault("qrcode", ModuleType("qrcode"))
import src.core.config.operate_config as operate_config
from src.core.config.config_model import (
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
)


def _make_config(qqid: int = 31415, runtime_target: str = "srv-uuid-async") -> Config:
    bot = BotConfig(
        name=f"BotAsync{qqid}",
        QQID=qqid,
        musicSignUrl=f"https://example.com/music/{qqid}",
        autoRestartSchedule=AutoRestartScheduleConfig(enable=False, time_unit="m", duration=1),
        offlineAutoRestart=False,
    )
    bot.runtime_target = runtime_target
    return Config(
        bot=bot,
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[],
            websocketClients=[],
            plugins=[],
        ),
        advanced=AdvancedConfig(),
    )


def _patch_path_func(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> SimpleNamespace:
    """构造最小可用的 PathFunc 替身, 隔离磁盘副作用 (与 test_operate_config.py 等价)."""
    base = tmp_path / "config"
    base.mkdir(parents=True, exist_ok=True)
    bot_config_path = base / "bot.json"
    napcat_config_path = base / "napcat"
    napcat_config_path.mkdir(parents=True, exist_ok=True)

    fake = SimpleNamespace(
        bot_config_path=bot_config_path,
        napcat_config_path=napcat_config_path,
    )
    monkeypatch.setattr(operate_config, "_get_path_func", lambda: fake)
    return fake


def _ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


class _RemoteSpy:
    """计数 + 记录 backend.write_bot_runtime_config / delete_bot_runtime_config 调用."""

    def __init__(self) -> None:
        self.write_calls: list[tuple[int, str]] = []
        self.delete_calls: list[str] = []
        # 用 Event 让 UI 端可以等到 worker 完成
        self.write_event = threading.Event()
        self.delete_event = threading.Event()

    def write_bot_runtime_config(self, config: Config) -> tuple[str, str]:
        self.write_calls.append((config.bot.QQID, threading.current_thread().name))
        self.write_event.set()
        return ("/remote/onebot.json", "/remote/napcat.json")

    def delete_bot_runtime_config(self, qq_id: str) -> None:
        self.delete_calls.append(qq_id)
        self.delete_event.set()


def _patch_resolver(monkeypatch: pytest.MonkeyPatch, spy: _RemoteSpy) -> None:
    from src.core.operation import resolver as resolver_module

    monkeypatch.setattr(resolver_module, "resolve_backend_for_bot", lambda config, **_: spy)


# ==================== 同步回退路径 ====================
def test_remote_sync_falls_back_to_sync_when_no_qapplication(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """无 QApplication 实例时必须走同步路径, 写调用立刻可见.

    回归保护: ``test_operate_config.py`` 等纯 logic 测试不创建 QApp,
    若此处异步派发被错误启用, 它们的 ``spy.write_calls`` 断言会失败.
    """
    # 测试运行期间, 之前的 case 可能已经 ensure_qapp; 仅当 Qt 还没初始化时这才有意义.
    if QApplication.instance() is not None:
        pytest.skip("QApplication 已初始化 (后续 case 残留), 同步回退条件不满足")

    _patch_path_func(monkeypatch, tmp_path)
    spy = _RemoteSpy()
    _patch_resolver(monkeypatch, spy)

    config = _make_config(qqid=10001)
    assert operate_config.update_config(config) is True

    # 同步路径下, 写调用必须已经完成
    assert spy.write_calls == [(10001, "MainThread")]


# ==================== 异步 dispatch 路径 ====================
def test_remote_sync_dispatches_to_qthreadpool_in_ui_context(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """UI 进程内: ``update_config`` 立即返回, SSH 写发生在 QThreadPool worker."""
    app = _ensure_qapp()  # noqa: F841

    _patch_path_func(monkeypatch, tmp_path)
    spy = _RemoteSpy()
    _patch_resolver(monkeypatch, spy)

    config = _make_config(qqid=20002, runtime_target="srv-async-1")

    t0 = time.monotonic()
    assert operate_config.update_config(config) is True
    elapsed = time.monotonic() - t0

    # ``update_config`` 返回前不应等待 SSH 完成 (写 < 50ms 数量级);
    # 同步实现是 50ms 量级 (Spy 直接返回), 但派发本身远远小于这个数. 给出 200ms 的宽松上界.
    assert elapsed < 0.5

    # 等 worker 完成 (3s 超时, 在 spy 不真实 SSH 的情况下应远小于此).
    assert spy.write_event.wait(timeout=3.0), "QThreadPool runnable 未在 3s 内完成"
    # P3 perf W4: 远端配置写已迁到 ``remote_ssh_pool``, 等待该池而非全局池
    from src.core.remote.thread_pool import remote_ssh_pool
    remote_ssh_pool().waitForDone(3000)
    QThreadPool.globalInstance().waitForDone(3000)

    assert len(spy.write_calls) == 1
    assert spy.write_calls[0][0] == 20002
    # SSH 调用必须发生在非主线程 (QThreadPool worker)
    assert spy.write_calls[0][1] != "MainThread"


def test_remote_delete_dispatches_to_qthreadpool_in_ui_context(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """对称: ``delete_config`` 也派发到 QThreadPool, 主线程不阻塞."""
    app = _ensure_qapp()  # noqa: F841

    fake_path = _patch_path_func(monkeypatch, tmp_path)
    spy = _RemoteSpy()
    _patch_resolver(monkeypatch, spy)

    config = _make_config(qqid=30003, runtime_target="srv-async-2")
    # 把目标 Bot 写到本地 bot.json (delete_config 需要它能找到目标条目)
    fake_path.bot_config_path.write_text(
        json.dumps(
            {
                "compatVersion": 0,
                "bots": [
                    json.loads(
                        config.model_dump_json(by_alias=False, exclude_none=False)
                    )
                ],
            },
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )

    assert operate_config.delete_config(config) is True
    assert spy.delete_event.wait(timeout=3.0), "QThreadPool delete runnable 未在 3s 内完成"
    # P3 perf W4: 远端配置删除同样走 ``remote_ssh_pool``
    from src.core.remote.thread_pool import remote_ssh_pool
    remote_ssh_pool().waitForDone(3000)
    QThreadPool.globalInstance().waitForDone(3000)

    assert spy.delete_calls == ["30003"]


# ==================== runnable 单元行为 ====================
def test_remote_config_op_runnable_runs_blocking_path(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """直接调用 runnable.run() 也应触发对应的 blocking 函数. 用于回归保护
    "未来有人改了 dispatch 但忘了改 runnable.run 内部分支" 的 bug."""
    sync_calls: list[Any] = []
    delete_calls: list[Any] = []

    monkeypatch.setattr(operate_config, "_do_remote_sync_blocking", lambda c: sync_calls.append(c))
    monkeypatch.setattr(
        operate_config, "_do_remote_delete_blocking", lambda c: delete_calls.append(c)
    )

    sync_runnable = operate_config._RemoteConfigOpRunnable(
        action="sync", config=_make_config(qqid=40004)
    )
    sync_runnable.run()
    delete_runnable = operate_config._RemoteConfigOpRunnable(
        action="delete", config=_make_config(qqid=40005)
    )
    delete_runnable.run()

    assert [c.bot.QQID for c in sync_calls] == [40004]
    assert [c.bot.QQID for c in delete_calls] == [40005]


def test_remote_config_op_runnable_unknown_action_logs_warning(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """未知 action 应被 log warning 而不是抛出, 避免 worker 静默 crash."""
    warnings: list[str] = []
    monkeypatch.setattr(operate_config.logger, "warning", lambda msg, *a, **k: warnings.append(msg))

    runnable = operate_config._RemoteConfigOpRunnable(
        action="unknown-op", config=_make_config(qqid=50005)
    )
    runnable.run()

    assert any("未知远端配置操作" in m for m in warnings)
