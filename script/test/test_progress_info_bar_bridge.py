# -*- coding: utf-8 -*-
"""[`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py)
桥接行为单测 (P3 perf, ProgressInfoBar 重构).

验证:

- ``BackgroundTaskCenter.begin`` → 桥在 parent 上 spawn 一个不确定模式 ProgressInfoBar
- ``BackgroundTaskCenter.end(success=True/False, message=...)`` → 桥调用对应
  InfoBar 的 ``setComplete``, 切换 ✅/❌ 配色
- 重复 begin 同一 task_id 视为 label/content 更新, 不重复弹窗
- parent 销毁后桥不再 spawn (避免野指针)
"""
from __future__ import annotations

# 标准库导入
import os
import sys
from types import ModuleType, SimpleNamespace
from typing import Any
from unittest.mock import MagicMock

# 第三方库导入
import pytest

# 项目内模块导入
sys.modules.setdefault("qrcode", ModuleType("qrcode"))

from PySide6.QtWidgets import QApplication, QWidget

import src.ui.components.progress_info_bar_bridge as bridge_module
from src.core.runtime.background_tasks import BackgroundTaskCenter


def _ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture
def reset_center(monkeypatch: pytest.MonkeyPatch) -> BackgroundTaskCenter:
    """每个测试用一个独立的 BackgroundTaskCenter, 不依赖 creart 单例."""
    center = BackgroundTaskCenter()
    monkeypatch.setattr(bridge_module, "it", lambda _cls: center)
    return center


class _FakeProgressInfoBar:
    """``ProgressInfoBar`` 的 in-memory 替身, 记录构造 + setComplete 调用.

    桥的契约只关心: 构造时拿到 (title, content, parent), 完成时调
    ``setComplete(success=, content=, autoCloseAfter=)`` — 这两点足够断言.
    """

    instances: list["_FakeProgressInfoBar"] = []

    def __init__(self, **kwargs: Any) -> None:
        self.title: str = kwargs.get("title", "")
        self.content: str = kwargs.get("content", "")
        self.is_closable: bool = kwargs.get("isClosable", True)
        self.duration: int = kwargs.get("duration", -1)
        self.position = kwargs.get("position")
        self.parent: QWidget | None = kwargs.get("parent")
        self.complete_calls: list[dict[str, Any]] = []
        self.title_updates: list[str] = []
        self.content_updates: list[str] = []
        type(self).instances.append(self)

    @classmethod
    def indeterminate(cls, **kwargs: Any) -> "_FakeProgressInfoBar":
        return cls(**kwargs)

    def setComplete(
        self, *, success: bool, content: str = "", autoCloseAfter: int = 1500
    ) -> None:
        self.complete_calls.append(
            {"success": success, "content": content, "autoCloseAfter": autoCloseAfter}
        )

    def setTitle(self, title: str) -> None:
        self.title = title
        self.title_updates.append(title)

    def setContent(self, content: str) -> None:
        self.content = content
        self.content_updates.append(content)


@pytest.fixture
def fake_info_bar_class(monkeypatch: pytest.MonkeyPatch) -> type[_FakeProgressInfoBar]:
    _FakeProgressInfoBar.instances = []
    monkeypatch.setattr(bridge_module, "ProgressInfoBar", _FakeProgressInfoBar)
    return _FakeProgressInfoBar


# ==================== 实例化 / spawn / 完成 ====================
def test_begin_spawns_progress_info_bar_on_parent(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """``begin(task_id, label, content)`` 在桥的 parent 上 spawn 一个 InfoBar."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841 - 保活

    reset_center.begin("t1", "启动 Bot 12345", content="正在通过 SSH 连接…")
    QApplication.processEvents()  # flush queued connections

    assert len(fake_info_bar_class.instances) == 1
    bar = fake_info_bar_class.instances[0]
    assert bar.title == "启动 Bot 12345"
    assert bar.content == "正在通过 SSH 连接…"
    assert bar.parent is parent
    # 长任务不允许用户提前关掉
    assert bar.is_closable is False
    # duration=-1 表示永驻直到 setComplete
    assert bar.duration == -1


def test_end_with_success_calls_setcomplete_on_corresponding_bar(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """``end(success=True, message=...)`` 把对应 InfoBar 切到完成态 (✅)."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841

    reset_center.begin("t1", "启动 Bot 12345")
    QApplication.processEvents()
    reset_center.end("t1", success=True, message="启动成功")
    QApplication.processEvents()

    bar = fake_info_bar_class.instances[0]
    assert bar.complete_calls == [
        {"success": True, "content": "启动成功", "autoCloseAfter": 1500},
    ]


def test_end_with_failure_propagates_message_to_setcomplete(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """``end(success=False, message=...)`` 让 InfoBar 切换到 ❌ 配色 + 失败文案."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841

    reset_center.begin("t1", "启动 Bot 12345")
    QApplication.processEvents()
    reset_center.fail("t1", "SSH 连接被拒绝")
    QApplication.processEvents()

    bar = fake_info_bar_class.instances[0]
    [call] = bar.complete_calls
    assert call["success"] is False
    assert call["content"] == "SSH 连接被拒绝"


def test_end_without_message_falls_back_to_title(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """``end`` 无 message 时, 桥用 InfoBar 当前 title 作为完成文案, 避免空白."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841

    reset_center.begin("t1", "启动 Bot 12345")
    QApplication.processEvents()
    reset_center.end("t1", success=True)  # message 为空串
    QApplication.processEvents()

    bar = fake_info_bar_class.instances[0]
    [call] = bar.complete_calls
    assert call["content"] == "启动 Bot 12345"


# ==================== 重复 begin / 未知 end ====================
def test_repeat_begin_updates_existing_bar_in_place(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """重复 begin 同一 task_id: 不重复 spawn, 仅更新已有 InfoBar 文案."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841

    reset_center.begin("t1", "启动 Bot", content="第一阶段…")
    QApplication.processEvents()
    reset_center.begin("t1", "启动 Bot (重试)", content="第二阶段…")
    QApplication.processEvents()

    assert len(fake_info_bar_class.instances) == 1
    bar = fake_info_bar_class.instances[0]
    assert bar.title_updates == ["启动 Bot (重试)"]
    assert bar.content_updates == ["第二阶段…"]


def test_end_unknown_task_is_silently_ignored(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """``end(task_id)`` 在桥侧没有对应 InfoBar 时不抛异常."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841

    reset_center.end("never-started", success=True, message="ignore")
    QApplication.processEvents()

    assert fake_info_bar_class.instances == []


# ==================== 启动期前序任务回放 ====================
def test_bridge_replays_existing_active_tasks_on_construction(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """桥构造时如果 Center 已有任务在跑, 应立即把它们补弹出来."""
    _ensure_qapp()
    parent = QWidget()

    reset_center.begin("t-pre", "已经在跑的任务", content="...")

    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841
    QApplication.processEvents()

    assert len(fake_info_bar_class.instances) == 1
    assert fake_info_bar_class.instances[0].title == "已经在跑的任务"


# ==================== parent 销毁后保护 ====================
def test_bridge_skips_spawn_when_parent_destroyed(
    reset_center: BackgroundTaskCenter,
    fake_info_bar_class: type[_FakeProgressInfoBar],
) -> None:
    """parent widget 被释放后, 桥不应再尝试 spawn InfoBar (使用 weakref 实现)."""
    _ensure_qapp()
    parent = QWidget()
    bridge = bridge_module.ProgressInfoBarBridge(parent)
    parent.deleteLater()
    QApplication.processEvents()

    # 替父级释放 weakref 引用
    bridge._parent_ref = type(bridge._parent_ref)(MagicMock())  # 强制让 weakref 返回 None
    # 直接构造一个会返回 None 的 weakref 是麻烦, 用最直接做法: 把 _parent_ref 替换成 lambda
    bridge._parent_ref = lambda: None  # type: ignore[assignment]

    reset_center.begin("t-after-destroy", "label", content="content")
    QApplication.processEvents()

    assert fake_info_bar_class.instances == []


def test_bridge_swallows_progress_info_bar_construction_errors(
    reset_center: BackgroundTaskCenter,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """ProgressInfoBar 构造异常被桥吞掉, 不应让 Runnable 因为 UI 故障挂掉."""
    _ensure_qapp()
    parent = QWidget()

    boom_logger = MagicMock()
    monkeypatch.setattr(bridge_module.logger, "warning", boom_logger)

    class _BoomBar:
        @classmethod
        def indeterminate(cls, **_kwargs: Any) -> Any:
            raise RuntimeError("simulated InfoBar construction failure")

    monkeypatch.setattr(bridge_module, "ProgressInfoBar", _BoomBar)

    bridge = bridge_module.ProgressInfoBarBridge(parent)  # noqa: F841
    reset_center.begin("t1", "label", content="c")
    QApplication.processEvents()

    # logger.warning 应被触发, 信号不应让 begin() 抛
    assert boom_logger.called
