# -*- coding: utf-8 -*-
"""[`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 单元测试 (P3 perf W1).

验证:

- begin / end 的计数累计 + 信号 emit 时序
- 重复 begin 视为 label 更新, 不重复增加计数
- 不存在的 task_id end 静默忽略
- 多线程并发 begin / end 后总计数收敛, 不串字典
- ``track`` 上下文管理器异常路径仍 emit ``end``
- ``active_tasks`` 返回值与登记顺序一致 (Python 3.7+ dict 保序)
"""
from __future__ import annotations

# 标准库导入
import threading

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.runtime.background_tasks import BackgroundTask, BackgroundTaskCenter


@pytest.fixture
def center() -> BackgroundTaskCenter:
    """每个测试用一个干净的中心实例 (绕开 creart 单例)."""
    return BackgroundTaskCenter()


def _collect_signals(center: BackgroundTaskCenter):
    """订阅三类信号到 list, 用于断言 emit 顺序."""
    started: list[tuple[str, str]] = []
    finished: list[str] = []
    counts: list[int] = []
    center.task_started_signal.connect(lambda task_id, label: started.append((task_id, label)))
    center.task_finished_signal.connect(lambda task_id: finished.append(task_id))
    center.count_changed_signal.connect(lambda count: counts.append(count))
    return started, finished, counts


def test_begin_end_emits_and_updates_count(center: BackgroundTaskCenter) -> None:
    started, finished, counts = _collect_signals(center)

    center.begin("t1", "启动 Bot 12345")
    center.begin("t2", "停止 Bot 67890")
    center.end("t1")
    center.end("t2")

    assert started == [
        ("t1", "启动 Bot 12345"),
        ("t2", "停止 Bot 67890"),
    ]
    assert finished == ["t1", "t2"]
    assert counts == [1, 2, 1, 0]
    assert center.active_count() == 0


def test_repeat_begin_updates_label_without_double_counting(center: BackgroundTaskCenter) -> None:
    started, _finished, counts = _collect_signals(center)

    center.begin("t1", "启动 Bot 12345")
    center.begin("t1", "启动 Bot 12345 (重试)")

    assert started == [
        ("t1", "启动 Bot 12345"),
        ("t1", "启动 Bot 12345 (重试)"),
    ]
    # 计数稳定在 1: 两次 begin 中第二次只是 label 更新
    assert counts == [1, 1]
    assert center.active_count() == 1
    [task] = center.active_tasks()
    assert task == BackgroundTask(task_id="t1", label="启动 Bot 12345 (重试)")


def test_end_unknown_task_is_silently_ignored(center: BackgroundTaskCenter) -> None:
    _started, finished, counts = _collect_signals(center)

    center.end("never_started")

    assert finished == []
    assert counts == []
    assert center.active_count() == 0


def test_active_tasks_preserves_insertion_order(center: BackgroundTaskCenter) -> None:
    center.begin("alpha", "A")
    center.begin("beta", "B")
    center.begin("gamma", "C")

    assert [t.task_id for t in center.active_tasks()] == ["alpha", "beta", "gamma"]
    center.end("beta")
    assert [t.task_id for t in center.active_tasks()] == ["alpha", "gamma"]


def test_track_context_manager_emits_end_on_success(center: BackgroundTaskCenter) -> None:
    _started, finished, counts = _collect_signals(center)

    with center.track("ctx-1", "label-1"):
        assert center.active_count() == 1

    assert finished == ["ctx-1"]
    assert counts == [1, 0]
    assert center.active_count() == 0


def test_track_context_manager_emits_end_on_exception(center: BackgroundTaskCenter) -> None:
    _started, finished, counts = _collect_signals(center)

    with pytest.raises(RuntimeError):
        with center.track("ctx-2", "label-2"):
            raise RuntimeError("boom")

    assert finished == ["ctx-2"]
    assert counts == [1, 0]


def test_concurrent_begin_end_converges(center: BackgroundTaskCenter) -> None:
    """多线程并发 begin / end 不应导致字典不一致或丢失计数."""
    iterations = 200
    threads_per_role = 4

    def begin_worker(prefix: str) -> None:
        for i in range(iterations):
            center.begin(f"{prefix}-{i}", f"label-{prefix}-{i}")

    def end_worker(prefix: str) -> None:
        # 等所有 begin 完成再 end, 避免 end 还没 begin 的 task
        for i in range(iterations):
            center.end(f"{prefix}-{i}")

    begin_threads = [
        threading.Thread(target=begin_worker, args=(f"role{i}",))
        for i in range(threads_per_role)
    ]
    for t in begin_threads:
        t.start()
    for t in begin_threads:
        t.join()

    assert center.active_count() == iterations * threads_per_role

    end_threads = [
        threading.Thread(target=end_worker, args=(f"role{i}",))
        for i in range(threads_per_role)
    ]
    for t in end_threads:
        t.start()
    for t in end_threads:
        t.join()

    assert center.active_count() == 0


def test_reset_for_test_clears_state(center: BackgroundTaskCenter) -> None:
    center.begin("t1", "L1")
    center.begin("t2", "L2")
    assert center.active_count() == 2

    _started, _finished, counts = _collect_signals(center)
    center.reset_for_test()

    assert center.active_count() == 0
    # reset 后只发一次 count_changed (=0), 不发 task_finished
    assert counts == [0]


# ==================== ProgressInfoBar 桥所需扩展信号 ====================
def test_begin_with_content_carries_through_to_started_signal(
    center: BackgroundTaskCenter,
) -> None:
    """``begin(.., content=...)`` 把 content 字段透传到 ``task_started_signal``."""
    started_full: list[tuple[str, str, str]] = []
    center.task_started_signal.connect(
        lambda task_id, label, content: started_full.append((task_id, label, content))
    )

    center.begin("t1", "启动 Bot 12345", content="正在通过 SSH 连接...")

    assert started_full == [("t1", "启动 Bot 12345", "正在通过 SSH 连接...")]
    [task] = center.active_tasks()
    assert task == BackgroundTask(
        task_id="t1", label="启动 Bot 12345", content="正在通过 SSH 连接..."
    )


def test_end_emits_task_completed_signal_with_success_and_message(
    center: BackgroundTaskCenter,
) -> None:
    """``end(.., success, message)`` 通过 ``task_completed_signal`` 暴露成败 + 文案."""
    completed: list[tuple[str, bool, str]] = []
    center.task_completed_signal.connect(
        lambda task_id, success, message: completed.append((task_id, success, message))
    )

    center.begin("t1", "L1")
    center.end("t1", success=True, message="启动成功")

    center.begin("t2", "L2")
    center.end("t2", success=False, message="SSH 超时")

    assert completed == [
        ("t1", True, "启动成功"),
        ("t2", False, "SSH 超时"),
    ]


def test_fail_is_alias_for_end_with_success_false(center: BackgroundTaskCenter) -> None:
    """``fail(task_id, message)`` 等价于 ``end(task_id, success=False, message=...)``."""
    completed: list[tuple[str, bool, str]] = []
    center.task_completed_signal.connect(
        lambda task_id, success, message: completed.append((task_id, success, message))
    )

    center.begin("t1", "L1")
    center.fail("t1", "连接被拒绝")

    assert completed == [("t1", False, "连接被拒绝")]
    assert center.active_count() == 0


def test_track_context_manager_propagates_success_message(
    center: BackgroundTaskCenter,
) -> None:
    """``track(.., success_message=...)`` 正常退出时把成功文案投递给 completed 信号."""
    completed: list[tuple[str, bool, str]] = []
    center.task_completed_signal.connect(
        lambda task_id, success, message: completed.append((task_id, success, message))
    )

    with center.track("ctx", "label", success_message="完成"):
        pass

    assert completed == [("ctx", True, "完成")]


def test_track_context_manager_marks_failure_with_exception_message(
    center: BackgroundTaskCenter,
) -> None:
    """``track`` 异常路径把 ``str(exc)`` 作为失败文案传给 completed 信号."""
    completed: list[tuple[str, bool, str]] = []
    center.task_completed_signal.connect(
        lambda task_id, success, message: completed.append((task_id, success, message))
    )

    with pytest.raises(RuntimeError):
        with center.track("ctx", "label"):
            raise RuntimeError("boom")

    assert completed == [("ctx", False, "boom")]
