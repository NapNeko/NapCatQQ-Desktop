# -*- coding: utf-8 -*-
"""验证 RunnableE 接入 [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 的契约 (P3 perf W2).

覆盖:

- :class:`src.core.runtime.napcat.RemoteBotOperationRunnable`:
    - ``start`` / ``stop`` 调用前后产生 begin/end 事件
    - ``poll`` 不上报 (避免 5s 一次的 poll 让状态条频闪)
    - 异常路径仍 emit ``end`` (保证状态条不会卡死)
- :class:`src.core.operation.migration.BotMigrationRunnable`: 同样 try/finally 兜底
"""
from __future__ import annotations

# 标准库导入
from dataclasses import dataclass, field
from types import SimpleNamespace

# 第三方库导入
import pytest
from creart import it

# 项目内模块导入
import src.core.runtime.napcat as run_napcat
from src.core.operation.backend import ProcessStatus
from src.core.runtime.background_tasks import BackgroundTaskCenter


@dataclass
class _RecordingBackend:
    """Backend 替身: 记录调用顺序, 让我们可以观察 task 是否在 SSH 调用前后开关."""

    events: list[str] = field(default_factory=list)
    raise_on_start: bool = False

    def connect(self) -> None:
        self.events.append("connect")

    def start_napcat(self, qq_id: str, config) -> ProcessStatus:  # noqa: ANN001
        self.events.append(f"start:{qq_id}")
        if self.raise_on_start:
            raise RuntimeError("ssh boom")
        return ProcessStatus(qq_id=qq_id, running=True, pid=1, memory_rss_bytes=0)

    def stop_napcat(self, qq_id: str) -> None:
        self.events.append(f"stop:{qq_id}")

    def close_webui_tunnel(self, qq_id: str) -> None:
        self.events.append(f"close-tunnel:{qq_id}")

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        self.events.append(f"poll:{qq_id}")
        return ProcessStatus(qq_id=qq_id, running=False, pid=None, memory_rss_bytes=0)

    def get_webui_endpoint(self, qq_id: str):  # noqa: ANN201
        return None


@pytest.fixture
def reset_center() -> BackgroundTaskCenter:
    """每个 case 拿到干净的 Center (creart 单例不重建, 内部状态用 reset_for_test 清空)."""
    center = it(BackgroundTaskCenter)
    center.reset_for_test()
    return center


@pytest.fixture
def fake_backend(monkeypatch: pytest.MonkeyPatch) -> _RecordingBackend:
    backend = _RecordingBackend()
    from src.core.operation import resolver as resolver_module

    monkeypatch.setattr(resolver_module, "resolve_backend_for_bot", lambda config, **_: backend)
    return backend


@pytest.fixture
def mute_logger(monkeypatch: pytest.MonkeyPatch) -> None:
    for level in ("trace", "info", "warning", "error", "exception"):
        monkeypatch.setattr(run_napcat.logger, level, lambda *a, **k: None)


def _make_remote_config(config_factory, qqid: int = 9527):
    config = config_factory(qqid=qqid)
    config.bot.runtime_target = "srv-1"
    return config


# ==================== RemoteBotOperationRunnable ====================
def test_remote_runnable_start_tracks_through_center(
    reset_center: BackgroundTaskCenter,
    fake_backend: _RecordingBackend,
    config_factory,
    mute_logger,
) -> None:
    started_events: list[tuple[str, str]] = []
    finished_events: list[str] = []
    reset_center.task_started_signal.connect(lambda task_id, label: started_events.append((task_id, label)))
    reset_center.task_finished_signal.connect(lambda task_id: finished_events.append(task_id))

    config = _make_remote_config(config_factory, qqid=9527)
    runnable = run_napcat.RemoteBotOperationRunnable("9527", config, "start")
    runnable.run()

    expected_task_id = "remote-bot-start-9527"
    assert any(task_id == expected_task_id for task_id, _ in started_events)
    assert finished_events == [expected_task_id]
    # SSH 调用确实发生在 begin 之后, end 之前
    assert "start:9527" in fake_backend.events
    assert reset_center.active_count() == 0


def test_remote_runnable_stop_tracks_through_center(
    reset_center: BackgroundTaskCenter,
    fake_backend: _RecordingBackend,
    config_factory,
    mute_logger,
) -> None:
    started_events: list[str] = []
    finished_events: list[str] = []
    reset_center.task_started_signal.connect(lambda task_id, _label: started_events.append(task_id))
    reset_center.task_finished_signal.connect(lambda task_id: finished_events.append(task_id))

    runnable = run_napcat.RemoteBotOperationRunnable(
        "9527", _make_remote_config(config_factory, qqid=9527), "stop"
    )
    runnable.run()

    assert started_events == ["remote-bot-stop-9527"]
    assert finished_events == ["remote-bot-stop-9527"]


def test_remote_runnable_poll_does_not_track(
    reset_center: BackgroundTaskCenter,
    fake_backend: _RecordingBackend,
    config_factory,
    mute_logger,
) -> None:
    """``poll`` 是 5s 一次的静默轮询, 不应推到 BackgroundTaskCenter."""
    started_events: list[str] = []
    reset_center.task_started_signal.connect(lambda task_id, _label: started_events.append(task_id))

    runnable = run_napcat.RemoteBotOperationRunnable(
        "9527", _make_remote_config(config_factory, qqid=9527), "poll"
    )
    runnable.run()

    assert started_events == []
    assert reset_center.active_count() == 0


def test_remote_runnable_end_emits_even_when_ssh_raises(
    reset_center: BackgroundTaskCenter,
    fake_backend: _RecordingBackend,
    config_factory,
    mute_logger,
) -> None:
    """SSH 调用抛异常时, 必须依然 ``end`` 该 task, 否则状态条会永远停留."""
    fake_backend.raise_on_start = True
    finished_events: list[str] = []
    reset_center.task_finished_signal.connect(lambda task_id: finished_events.append(task_id))

    runnable = run_napcat.RemoteBotOperationRunnable(
        "9527", _make_remote_config(config_factory, qqid=9527), "start"
    )
    runnable.run()

    assert finished_events == ["remote-bot-start-9527"]
    assert reset_center.active_count() == 0


# ==================== BotMigrationRunnable ====================
def test_bot_migration_runnable_tracks_through_center(
    reset_center: BackgroundTaskCenter, monkeypatch: pytest.MonkeyPatch
) -> None:
    """:class:`BotMigrationRunnable` 上报到 Center, 任何路径都 end."""
    from src.core.operation import migration as migration_module

    finished_events: list[str] = []
    reset_center.task_finished_signal.connect(lambda task_id: finished_events.append(task_id))

    # 用 SimpleNamespace 假冒 BotMigrationService, execute() 立即返回不走真正迁移流程
    fake_service = SimpleNamespace(
        progress_signal=SimpleNamespace(connect=lambda *_a, **_k: None),
        finished_signal=SimpleNamespace(connect=lambda *_a, **_k: None),
        execute=lambda plan: None,
    )
    monkeypatch.setattr(migration_module, "BotMigrationService", lambda: fake_service)
    # 屏蔽 logger
    for level in ("trace", "info", "warning", "error", "exception"):
        monkeypatch.setattr(migration_module.logger, level, lambda *a, **k: None)

    plan = SimpleNamespace(qq_id="42", source_target="local", dest_target="srv-1")
    runnable = migration_module.BotMigrationRunnable(plan)
    runnable.run()

    assert finished_events == ["bot-migration-42"]
    assert reset_center.active_count() == 0
