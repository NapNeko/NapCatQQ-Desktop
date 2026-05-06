# -*- coding: utf-8 -*-
"""[`BatchDispatcher`](src/core/operation/batch_dispatcher.py) 单元测试 (P4 W1·F2).

通过 ``executor=_inline_executor`` 同步执行所有 worker, 不依赖 QApplication /
QThreadPool, 也不依赖真实 ``BackgroundTaskCenter`` (creart 上下文为空时, dispatcher
会静默跳过 BackgroundTaskCenter 调用).

覆盖:

- 全成功: progress 信号 N 次, finished 携带 N 个 ok=True outcome
- 部分失败: 失败子项 ok=False + error 文案非空
- ``sequential=True`` 时执行顺序与 items 一致
- ``sequential=False`` (默认) 全部 op 都被调用
- 空 items: 立刻 emit ``finished_signal([])``, 不进 tracker
- BackgroundTaskCenter 集成: 真实注入 center 时, begin / end 被调用
- ``batch_started_signal`` 同步 emit 在 dispatch() 返回前
"""
from __future__ import annotations

# 标准库导入
from collections.abc import Callable

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.operation.batch_dispatcher import (
    BatchDispatcher,
    BatchOutcome,
    _inline_executor,
)


# ==================== fixtures ====================
@pytest.fixture
def dispatcher() -> BatchDispatcher:
    return BatchDispatcher()


def _ok_op() -> Callable[[], None]:
    return lambda: None


def _fail_op(message: str) -> Callable[[], None]:
    def _op() -> None:
        raise RuntimeError(message)

    return _op


def _collect(dispatcher: BatchDispatcher):
    progress: list[tuple[int, int]] = []
    finished: list[list[BatchOutcome]] = []
    started: list[tuple[str, str, int]] = []
    dispatcher.progress_signal.connect(lambda done, total: progress.append((done, total)))
    dispatcher.finished_signal.connect(lambda outcomes: finished.append(list(outcomes)))
    dispatcher.batch_started_signal.connect(
        lambda batch_id, label, total: started.append((batch_id, label, total))
    )
    return progress, finished, started


# ==================== 全成功 ====================
def test_all_success_emits_progress_and_finished_with_ok_outcomes(
    dispatcher: BatchDispatcher,
) -> None:
    progress, finished, started = _collect(dispatcher)

    items = [(f"qq-{i}", _ok_op()) for i in range(3)]
    batch_id = dispatcher.dispatch("批量启动", items, executor=_inline_executor)

    assert batch_id.startswith("batch-")
    # batch_started 在 dispatch 内部同步 emit
    assert started == [(batch_id, "批量启动", 3)]
    # 3 次 progress, 累加到 (3, 3)
    assert progress == [(1, 3), (2, 3), (3, 3)]
    # finished 1 次, 全部 ok
    assert len(finished) == 1
    outcomes = finished[0]
    assert [o.key for o in outcomes] == ["qq-0", "qq-1", "qq-2"]
    assert all(o.ok for o in outcomes)
    assert all(o.error is None for o in outcomes)


# ==================== 部分失败 ====================
def test_partial_failure_aggregates_error_messages(dispatcher: BatchDispatcher) -> None:
    _progress, finished, _started = _collect(dispatcher)

    items = [
        ("qq-1", _ok_op()),
        ("qq-2", _fail_op("SSH 超时")),
        ("qq-3", _ok_op()),
    ]
    dispatcher.dispatch("批量启动", items, executor=_inline_executor)

    assert len(finished) == 1
    outcomes = finished[0]
    by_key = {o.key: o for o in outcomes}

    assert by_key["qq-1"].ok is True
    assert by_key["qq-2"].ok is False
    assert "SSH 超时" in (by_key["qq-2"].error or "")
    assert by_key["qq-3"].ok is True


def test_failure_uses_friendly_error_for_known_exception_types(
    dispatcher: BatchDispatcher,
) -> None:
    """``to_friendly`` 命中 ConnectionRefusedError 时给出中文文案."""
    _progress, finished, _started = _collect(dispatcher)

    def boom() -> None:
        raise ConnectionRefusedError(111, "refused")

    dispatcher.dispatch("批量启动", [("qq-1", boom)], executor=_inline_executor)

    [outcomes] = finished
    [outcome] = outcomes
    assert outcome.ok is False
    assert "目标端口拒绝连接" in (outcome.error or "")


# ==================== sequential 模式 ====================
def test_sequential_mode_preserves_call_order(dispatcher: BatchDispatcher) -> None:
    call_order: list[str] = []

    def make_op(label: str) -> Callable[[], None]:
        return lambda: call_order.append(label)

    items = [
        ("qq-A", make_op("A")),
        ("qq-B", make_op("B")),
        ("qq-C", make_op("C")),
    ]
    _progress, finished, _started = _collect(dispatcher)

    dispatcher.dispatch(
        "批量迁移",
        items,
        sequential=True,
        executor=_inline_executor,
    )

    assert call_order == ["A", "B", "C"]
    assert len(finished) == 1
    assert [o.key for o in finished[0]] == ["qq-A", "qq-B", "qq-C"]


def test_sequential_mode_continues_on_individual_failure(dispatcher: BatchDispatcher) -> None:
    """sequential=True 时单个失败不应阻断后续."""
    call_order: list[str] = []

    def fail(label: str) -> Callable[[], None]:
        def _op() -> None:
            call_order.append(label)
            raise RuntimeError(f"{label} failed")

        return _op

    def ok(label: str) -> Callable[[], None]:
        def _op() -> None:
            call_order.append(label)

        return _op

    items = [("qq-A", ok("A")), ("qq-B", fail("B")), ("qq-C", ok("C"))]
    _progress, finished, _started = _collect(dispatcher)

    dispatcher.dispatch("批量迁移", items, sequential=True, executor=_inline_executor)

    assert call_order == ["A", "B", "C"]
    by_key = {o.key: o for o in finished[0]}
    assert by_key["qq-A"].ok is True
    assert by_key["qq-B"].ok is False
    assert by_key["qq-C"].ok is True


# ==================== 空 items ====================
def test_empty_items_emits_finished_immediately(dispatcher: BatchDispatcher) -> None:
    progress, finished, started = _collect(dispatcher)

    batch_id = dispatcher.dispatch("批量启动", [], executor=_inline_executor)

    assert batch_id.startswith("batch-")
    assert started == [(batch_id, "批量启动", 0)]
    assert progress == []
    assert finished == [[]]
    # 0 项不进 tracker
    assert dispatcher.active_batch_ids() == []


# ==================== batch_id ====================
def test_explicit_batch_id_is_used_verbatim(dispatcher: BatchDispatcher) -> None:
    bid = dispatcher.dispatch(
        "批量启动",
        [("qq-1", _ok_op())],
        executor=_inline_executor,
        batch_id="my-batch-id",
    )
    assert bid == "my-batch-id"


def test_active_batch_ids_is_empty_after_completion(dispatcher: BatchDispatcher) -> None:
    """tracker 在 finalize 后从 dispatcher 中移除."""
    dispatcher.dispatch(
        "批量启动",
        [("qq-1", _ok_op())],
        executor=_inline_executor,
    )
    assert dispatcher.active_batch_ids() == []


# ==================== BackgroundTaskCenter 集成 ====================
def test_dispatcher_reports_single_aggregated_task_to_background_center(
    dispatcher: BatchDispatcher,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """dispatcher 应在 BackgroundTaskCenter 上**只**注册 1 个 task, 不论 N 多大."""
    from src.core.runtime.background_tasks import BackgroundTaskCenter

    fake_center = BackgroundTaskCenter()
    begin_calls: list[tuple[str, str, str]] = []
    end_calls: list[tuple[str, bool, str]] = []

    fake_center.task_started_signal.connect(
        lambda task_id, label, content: begin_calls.append((task_id, label, content))
    )
    fake_center.task_completed_signal.connect(
        lambda task_id, success, message: end_calls.append((task_id, success, message))
    )

    # 让 ``it(BackgroundTaskCenter)`` 在 batch_dispatcher 内部返回我们的 fake
    import src.core.operation.batch_dispatcher as bd_mod
    real_it = bd_mod.it

    def fake_it(target):
        if target is BackgroundTaskCenter:
            return fake_center
        return real_it(target)

    monkeypatch.setattr(bd_mod, "it", fake_it)

    dispatcher.dispatch(
        "批量启动",
        [("qq-1", _ok_op()), ("qq-2", _ok_op()), ("qq-3", _fail_op("err"))],
        executor=_inline_executor,
    )

    # begin 在 dispatch 启动时 emit 1 次, 然后每个完成回调里也会 begin 一次更新 content.
    # 但 task_id 始终是同一个 batch_id, 不会有 N 个独立 task_id.
    task_ids = {b[0] for b in begin_calls}
    assert len(task_ids) == 1

    # content 应在累加: "0/3" -> "1/3" -> "2/3" -> "3/3"
    contents = [b[2] for b in begin_calls]
    assert contents[0] == "0/3"
    assert "3/3" in contents

    # end 1 次, success=False (有 1 项失败)
    assert len(end_calls) == 1
    [end_event] = end_calls
    assert end_event[1] is False
    assert "成功 2 / 失败 1" in end_event[2]


def test_dispatcher_reports_full_success_message_when_no_failures(
    dispatcher: BatchDispatcher,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from src.core.runtime.background_tasks import BackgroundTaskCenter

    fake_center = BackgroundTaskCenter()
    end_calls: list[tuple[str, bool, str]] = []
    fake_center.task_completed_signal.connect(
        lambda task_id, success, message: end_calls.append((task_id, success, message))
    )

    import src.core.operation.batch_dispatcher as bd_mod
    real_it = bd_mod.it

    def fake_it(target):
        if target is BackgroundTaskCenter:
            return fake_center
        return real_it(target)

    monkeypatch.setattr(bd_mod, "it", fake_it)

    dispatcher.dispatch(
        "批量启动",
        [("qq-1", _ok_op()), ("qq-2", _ok_op())],
        executor=_inline_executor,
    )

    [end_event] = end_calls
    assert end_event[1] is True
    assert "全部完成" in end_event[2]
    assert "2/2" in end_event[2]
