# -*- coding: utf-8 -*-
"""SnowLumaStatusPoller 单测 (W3: 按 UIN 聚合 + pid_set_changed).

覆盖范围:

- :func:`_is_real_uin` 上游 isRealUin 规则
- UIN 锁定: 首次探测优先 ``initial_pid`` 命中, fallback ``qq_instances[0].uin``;
  锁定后 ``initial_pid`` 不再 filter
- 状态合成集合语义:
  - 任一 ``online`` → ``logged_in``
  - 否则 ``loaded`` → ``waiting_for_qr_scan``
  - 否则 ``available/loading/connecting`` → ``starting``
  - 否则 全 ``error/disconnected`` → ``disconnected``
  - ``matched`` 空但 ``qq_list`` 含 UIN → fallback ``logged_in``
- ``pid_set_changed`` 信号: matched PID 集合变化时 emit, sorted list payload
- ``_STATUS_TRANSLATION_TABLE`` 已删除 (W3 集合语义替代)

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.3,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W3.
"""
from __future__ import annotations

import os
from unittest.mock import MagicMock

import pytest
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


# ==================== 测试辅助 ====================
def _make_hook_info(pid: int, uin: str, status: str, error: str = ""):
    """构造 :class:`HookProcessInfo` mock 替代 (dataclass 的轻量实例)."""
    from src.core.runtime.snowluma_webui_client import HookProcessInfo

    return HookProcessInfo(
        pid=pid,
        name="QQ.exe",
        path="C:\\QQ\\QQ.exe",
        uin=uin,
        status=status,
        error=error,
    )


def _make_qq_instance(uin: str, nickname: str = "Nick"):
    """构造 :class:`OneBotInstanceInfo`."""
    from src.core.runtime.snowluma_webui_client import OneBotInstanceInfo

    return OneBotInstanceInfo(uin=uin, nickname=nickname)


def _make_poller(qq_id: str = "11001", initial_pid: int = 12345):
    """构造一个不真跑 timer 的 poller (mock webui_client)."""
    from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller

    client_mock = MagicMock()
    poller = SnowLumaStatusPoller(
        qq_id=qq_id,
        initial_pid=initial_pid,
        webui_client=client_mock,
    )
    return poller


def _collect_signals(poller):
    """绑定一组 list 收集 poller 三个信号 emit 的 payload."""
    state_payloads: list[tuple[str, str]] = []
    uin_payloads: list[tuple[str, str]] = []
    pid_set_payloads: list[tuple[str, list[int]]] = []
    poller.state_changed.connect(lambda qq, st: state_payloads.append((qq, st)))
    poller.uin_detected.connect(lambda qq, u: uin_payloads.append((qq, u)))
    poller.pid_set_changed.connect(lambda qq, ps: pid_set_payloads.append((qq, list(ps))))
    return {
        "state": state_payloads,
        "uin": uin_payloads,
        "pid_set": pid_set_payloads,
    }


# ==================== _is_real_uin ====================
class TestIsRealUin:
    """`_is_real_uin` 上游 isRealUin 规则: 非空 + 非 "0" + 全数字 + 长度 ≥ 5."""

    def test_valid_uin(self) -> None:
        from src.core.runtime.snowluma_status_poller import _is_real_uin

        assert _is_real_uin("12345") is True
        assert _is_real_uin("114514") is True
        assert _is_real_uin("  10000000  ") is True  # strip

    def test_empty_returns_false(self) -> None:
        from src.core.runtime.snowluma_status_poller import _is_real_uin

        assert _is_real_uin("") is False
        assert _is_real_uin("   ") is False

    def test_zero_returns_false(self) -> None:
        from src.core.runtime.snowluma_status_poller import _is_real_uin

        assert _is_real_uin("0") is False

    def test_short_returns_false(self) -> None:
        from src.core.runtime.snowluma_status_poller import _is_real_uin

        assert _is_real_uin("123") is False  # < 5

    def test_non_digit_returns_false(self) -> None:
        from src.core.runtime.snowluma_status_poller import _is_real_uin

        assert _is_real_uin("abc12") is False
        assert _is_real_uin("123a5") is False


# ==================== UIN 锁定 ====================
class TestUinLock:
    """UIN 锁定: 首次探测来源优先级 + 锁定后不再变."""

    def test_initial_pid_match_locks_uin(self) -> None:
        """首次 tick 含匹配 ``initial_pid`` 的 hook_info → 锁定其 uin."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        processes = [
            _make_hook_info(pid=10001, uin="20020202", status="online"),
            _make_hook_info(pid=10002, uin="20020202", status="online"),  # 同 UIN 多 PID
        ]
        poller._on_processes(processes, qq_instances=[])

        assert poller.uin == "20020202"
        assert signals["uin"] == [("11001", "20020202")]

    def test_does_not_fallback_to_qq_instances_when_processes_has_other_bots(self) -> None:
        """**多 Bot 安全 (2026-05-11 bugfix)**: ``initial_pid`` 不在 processes 中 **但**
        ``processes`` 非空 (说明有别的 Bot 在跑) → **不**锁定; 等下次 tick 重试.

        历史 bug: 旧版 fallback 直接拿 ``qq_instances[0].uin``, 多 Bot 场景下错锁
        别 Bot 的 UIN, 导致 manager 报 'UIN 不匹配' 误停 Bot.
        参见 ``snowluma_status_poller.py:_try_lock_uin`` docstring.
        """
        poller = _make_poller(initial_pid=99999)
        signals = _collect_signals(poller)

        # 模拟场景: 当前 Bot 注入未完成 (processes 没有 99999 也没其后代)
        # 但 processes 已有别 Bot 的 88888, qq_instances 也含别 Bot 的 30030303.
        processes = [
            _make_hook_info(pid=88888, uin="30030303", status="online"),
        ]
        qq_instances = [_make_qq_instance(uin="30030303", nickname="OtherBot")]
        poller._on_processes(processes, qq_instances=qq_instances)

        # 关键: 不应锁定别 Bot 的 UIN
        assert poller.uin == ""
        assert signals["uin"] == []

    def test_fallback_to_qq_instances_only_when_processes_empty_and_single_instance(
        self,
    ) -> None:
        """**严格 fallback (2026-05-11)**: 仅当 ``processes`` **完全为空** (Windows enum
        bug) **且** ``qq_instances`` 恰好 1 条时, 才信任 ``qq_instances[0].uin`` 锁定.

        覆盖单 Bot 场景下 Windows ``getAllMainProcess()`` 偶发返空时的兜底路径.
        """
        poller = _make_poller(initial_pid=99999)
        signals = _collect_signals(poller)

        processes: list = []
        qq_instances = [_make_qq_instance(uin="30030303", nickname="Bob")]
        poller._on_processes(processes, qq_instances=qq_instances)

        assert poller.uin == "30030303"
        assert signals["uin"] == [("11001", "30030303")]

    def test_does_not_fallback_when_processes_empty_and_multiple_instances(self) -> None:
        """**多 instance 不可知**: ``processes`` 空 但 ``qq_instances`` 有 2+ 条时,
        无法可靠判断哪个属于本 Bot (qq_instances 没有 pid 字段对照) → 不锁定.
        """
        poller = _make_poller(initial_pid=99999)
        signals = _collect_signals(poller)

        processes: list = []
        qq_instances = [
            _make_qq_instance(uin="30030303", nickname="Alice"),
            _make_qq_instance(uin="40040404", nickname="Bob"),
        ]
        poller._on_processes(processes, qq_instances=qq_instances)

        assert poller.uin == ""
        assert signals["uin"] == []

    def test_pid_tree_match_via_candidate_pids_arg(self) -> None:
        """**psutil 子进程树匹配 (2026-05-11 bugfix)**: ``initial_pid`` 是 Electron
        launcher (parent), 实际 hook 的是子进程 PID. ``_ListProcessesRunnable`` 在
        工作线程预算 ``candidate_pids = {initial_pid, child_pid_1, ...}`` 后通过信号
        传给 ``_on_processes``, 让 ``processes`` 里的子进程 PID 也能匹中.

        本测试直接给 ``_on_processes`` 传 ``candidate_pids`` 模拟 runnable 已计算完.
        """
        poller = _make_poller(initial_pid=99999)
        signals = _collect_signals(poller)

        # processes 里只有子进程 PID, 不直接含 99999
        processes = [
            _make_hook_info(pid=88888, uin="50050505", status="online"),
        ]
        # 模拟 runnable 通过信号传入的 candidate_pids 列表 (含 launcher + 2 个子进程)
        poller._on_processes(
            processes,
            qq_instances=[],
            candidate_pids=[99999, 88888, 88889],
        )

        # 应通过 candidate_pids 匹中子进程, 锁定其 UIN
        assert poller.uin == "50050505"
        assert signals["uin"] == [("11001", "50050505")]

    def test_main_thread_slot_does_not_call_psutil(self, monkeypatch) -> None:
        """**线程安全 (2026-05-11 卡顿修复)**: ``_on_processes`` 是 Qt 主线程槽,
        **绝不能**调用 ``psutil`` (会卡 UI). 没传 ``candidate_pids`` 时也只能 fallback
        到 ``{initial_pid}``, 而不是临时 walk 进程树.

        本测试通过 monkeypatch 让 ``psutil.Process`` 在被调时直接 raise, 验证主线程
        slot 路径不会触发它.
        """
        import src.core.runtime.snowluma_status_poller as poller_mod

        def _explode(*_args, **_kwargs):
            raise AssertionError("psutil.Process 不应在主线程 slot 中被调用")

        monkeypatch.setattr(poller_mod.psutil, "Process", _explode)

        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # 没传 candidate_pids; slot 应 fallback 到 {initial_pid} 不动 psutil
        poller._on_processes(
            [_make_hook_info(pid=10001, uin="20020202", status="online")],
            qq_instances=[],
        )

        # 用 initial_pid fallback 也能匹 (因为 processes 含 10001)
        assert poller.uin == "20020202"
        assert signals["uin"] == [("11001", "20020202")]

    def test_uin_locked_after_first_detect_even_if_initial_pid_disappears(self) -> None:
        """UIN 锁定后, 即使 ``initial_pid`` 在后续 tick 中消失也不影响匹配."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # Tick 1: 锁定 UIN
        poller._on_processes(
            [_make_hook_info(pid=10001, uin="40040404", status="online")],
            qq_instances=[],
        )
        assert poller.uin == "40040404"
        assert signals["uin"] == [("11001", "40040404")]

        # Tick 2: initial_pid 没了, 但有同 UIN 的另一条 PID
        poller._on_processes(
            [_make_hook_info(pid=20002, uin="40040404", status="online")],
            qq_instances=[],
        )
        assert poller.uin == "40040404"
        # uin_detected 不应再 emit
        assert signals["uin"] == [("11001", "40040404")]
        # 状态仍是 logged_in
        assert signals["state"][-1] == ("11001", "logged_in")

    def test_unreal_uin_does_not_lock(self) -> None:
        """``uin="0"`` 或长度 < 5 等无效值不应锁定."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # Tick 1: 假 UIN
        poller._on_processes(
            [_make_hook_info(pid=10001, uin="0", status="loading")],
            qq_instances=[],
        )
        assert poller.uin == ""
        assert signals["uin"] == []


# ==================== 状态合成集合语义 ====================
class TestSynthesizeState:
    """状态合成: 集合语义优先级 online > loaded > starting-tier > disconnected."""

    def test_any_online_emits_logged_in(self) -> None:
        """多 PID 中任一 online → logged_in."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # 一个 loading + 一个 online (同 UIN)
        poller._on_processes(
            [
                _make_hook_info(pid=10001, uin="55555", status="loading"),
                _make_hook_info(pid=10002, uin="55555", status="online"),
            ],
            qq_instances=[],
        )
        assert signals["state"] == [("11001", "logged_in")]

    def test_all_loaded_emits_waiting_for_qr_scan(self) -> None:
        """全 loaded (无 online) → waiting_for_qr_scan."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        poller._on_processes(
            [
                _make_hook_info(pid=10001, uin="55555", status="loaded"),
                _make_hook_info(pid=10002, uin="55555", status="loaded"),
            ],
            qq_instances=[],
        )
        assert signals["state"] == [("11001", "waiting_for_qr_scan")]

    def test_any_starting_tier_emits_starting(self) -> None:
        """无 online/loaded 但有 available/loading/connecting → starting."""
        poller = _make_poller(initial_pid=10001)
        # 先锁 UIN (用一个 loading 状态触发锁定但不立即匹配 logged_in)
        poller._uin = "55555"
        signals = _collect_signals(poller)

        for status in ("available", "loading", "connecting"):
            poller._last_state = None  # reset 以便每次 emit
            signals["state"].clear()
            poller._on_processes(
                [_make_hook_info(pid=10001, uin="55555", status=status)],
                qq_instances=[],
            )
            assert signals["state"] == [
                ("11001", "starting")
            ], f"status={status} 应触发 starting"

    def test_all_disconnected_or_error_emits_disconnected(self) -> None:
        """全 error/disconnected (无更高优先级) → disconnected."""
        poller = _make_poller(initial_pid=10001)
        poller._uin = "55555"
        signals = _collect_signals(poller)

        poller._on_processes(
            [
                _make_hook_info(pid=10001, uin="55555", status="error", error="oops"),
                _make_hook_info(pid=10002, uin="55555", status="disconnected"),
            ],
            qq_instances=[],
        )
        assert signals["state"] == [("11001", "disconnected")]

    def test_mixed_online_and_error_prefers_logged_in(self) -> None:
        """混合 online + error → logged_in 胜出 (任一 online 即在线)."""
        poller = _make_poller(initial_pid=10001)
        poller._uin = "55555"
        signals = _collect_signals(poller)

        poller._on_processes(
            [
                _make_hook_info(pid=10001, uin="55555", status="error"),
                _make_hook_info(pid=10002, uin="55555", status="online"),
            ],
            qq_instances=[],
        )
        assert signals["state"] == [("11001", "logged_in")]


# ==================== qq_instances fallback ====================
class TestQqInstancesFallback:
    """``matched`` 空 但 qq_instances 含本 UIN → fallback ``logged_in``
    (修复 Windows ``getAllMainProcess()`` 返空场景, W7 旧版语义沿用).
    """

    def test_processes_empty_qq_list_match_emits_logged_in(self) -> None:
        """processes 空 (没找到任何 hook), 但 qq_list 含本 UIN → logged_in."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # 第一 tick: 由 qq_instances fallback 锁 UIN
        poller._on_processes(
            [],
            qq_instances=[_make_qq_instance(uin="66666666")],
        )

        assert poller.uin == "66666666"
        # fallback logged_in
        assert signals["state"] == [("11001", "logged_in")]


# ==================== pid_set_changed 信号 ====================
class TestPidSetChanged:
    """``pid_set_changed`` 信号: 本 UIN 关联 PID 集合变化时 emit, sorted list payload."""

    def test_emit_on_first_tick_with_match(self) -> None:
        """首次 tick matched 非空 → emit pid_set_changed."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        poller._on_processes(
            [_make_hook_info(pid=10001, uin="77777", status="online")],
            qq_instances=[],
        )

        assert signals["pid_set"] == [("11001", [10001])]

    def test_emit_on_watcher_expansion(self) -> None:
        """Tick 2 中 watcher 自动发现新同 UIN PID → emit 新 sorted list."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        # Tick 1: 单 PID
        poller._on_processes(
            [_make_hook_info(pid=10001, uin="88888", status="online")],
            qq_instances=[],
        )
        # Tick 2: watcher 发现第二条 PID
        poller._on_processes(
            [
                _make_hook_info(pid=10001, uin="88888", status="online"),
                _make_hook_info(pid=10002, uin="88888", status="online"),
            ],
            qq_instances=[],
        )

        assert signals["pid_set"] == [
            ("11001", [10001]),
            ("11001", [10001, 10002]),  # sorted
        ]

    def test_no_emit_when_pid_set_unchanged(self) -> None:
        """连续 tick PID 集合不变 → 不重复 emit pid_set_changed."""
        poller = _make_poller(initial_pid=10001)
        signals = _collect_signals(poller)

        for _ in range(3):
            poller._on_processes(
                [_make_hook_info(pid=10001, uin="99999", status="online")],
                qq_instances=[],
            )

        # 仅首次 emit
        assert signals["pid_set"] == [("11001", [10001])]


# ==================== _STATUS_TRANSLATION_TABLE 已删除 ====================
class TestTranslationTableRemoved:
    """W3: ``_STATUS_TRANSLATION_TABLE`` 模块级映射表已被集合语义替代; 应不可 import."""

    def test_translation_table_no_longer_exported(self) -> None:
        import src.core.runtime.snowluma_status_poller as poller_module

        assert not hasattr(poller_module, "_STATUS_TRANSLATION_TABLE"), (
            "W3 应删除 _STATUS_TRANSLATION_TABLE; 集合语义已在 _synthesize_state 内联"
        )

    def test_translate_status_static_method_no_longer_exists(self) -> None:
        from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller

        assert not hasattr(SnowLumaStatusPoller, "_translate_status"), (
            "W3 应删除 _translate_status; 状态合成移到 _synthesize_state (集合语义)"
        )


# ==================== 失败计数 ====================
class TestFailureCounter:
    """连续失败 3 次 → emit disconnected (旧行为沿用)."""

    def test_three_consecutive_errors_emit_disconnected(self) -> None:
        poller = _make_poller()
        signals = _collect_signals(poller)

        for i in range(3):
            poller._on_error(f"error {i}")

        # 第 3 次失败 emit disconnected
        assert signals["state"] == [("11001", "disconnected")]


# ==================== _ListProcessesRunnable 工作线程 psutil 行为 ====================
class TestRunnablePsutilWorker:
    """**2026-05-11 卡顿修复**: psutil 子进程树 walk 必须在工作线程 (``QThreadPool``)
    跑, 主线程 slot 不再调 psutil. 本组测试覆盖 ``_ListProcessesRunnable.run`` 的
    candidate_pids 计算与 emit.
    """

    def test_run_collects_candidate_pids_when_uin_not_locked(self, monkeypatch) -> None:
        """``uin_locked=False`` → run 在工作线程 walk psutil + 把 candidate_pids
        通过信号传出.
        """
        from unittest.mock import MagicMock

        import src.core.runtime.snowluma_status_poller as poller_mod
        from src.core.runtime.snowluma_status_poller import _ListProcessesRunnable

        # mock psutil: initial_pid=99999 有子进程 88888 / 88889
        fake_proc = MagicMock()
        fake_proc.children.return_value = [
            MagicMock(pid=88888),
            MagicMock(pid=88889),
        ]
        monkeypatch.setattr(poller_mod.psutil, "Process", lambda pid: fake_proc)

        # mock webui_client: list_processes / list_qq_instances 返空
        client = MagicMock()
        client.list_processes.return_value = []
        client.list_qq_instances.return_value = []

        runnable = _ListProcessesRunnable(client, initial_pid=99999, uin_locked=False)
        emitted: list[tuple] = []
        runnable.processes_signal.connect(
            lambda procs, qq, cands: emitted.append((procs, qq, cands))
        )

        runnable.run()

        assert len(emitted) == 1
        _procs, _qq, candidate_pids = emitted[0]
        # candidate_pids 应含 99999 + 子进程 88888/88889 (升序 list)
        assert sorted(candidate_pids) == [88888, 88889, 99999]

    def test_run_skips_psutil_when_uin_locked(self, monkeypatch) -> None:
        """``uin_locked=True`` → 不调 psutil (省开销), candidate_pids emit 空 list."""
        import src.core.runtime.snowluma_status_poller as poller_mod
        from src.core.runtime.snowluma_status_poller import _ListProcessesRunnable
        from unittest.mock import MagicMock

        def _explode(*_args, **_kwargs):
            raise AssertionError("uin_locked=True 时不应调 psutil")

        monkeypatch.setattr(poller_mod.psutil, "Process", _explode)

        client = MagicMock()
        client.list_processes.return_value = []
        client.list_qq_instances.return_value = []

        runnable = _ListProcessesRunnable(client, initial_pid=99999, uin_locked=True)
        emitted: list[tuple] = []
        runnable.processes_signal.connect(
            lambda procs, qq, cands: emitted.append((procs, qq, cands))
        )

        runnable.run()

        assert len(emitted) == 1
        _procs, _qq, candidate_pids = emitted[0]
        assert candidate_pids == []  # uin_locked=True 时不算 candidate_pids


# ==================== poller stop 后 in-flight runnable 不再触发槽 ====================
class TestStopDisconnectsInFlightRunnable:
    """方案 A 重构: ``stop()`` 必须断开 in-flight runnable 的槽连接, 否则后台
    HTTP 跑完时 ``processes_signal`` queued 投递, poller 已被上层
    ``deleteLater``, PySide 抛 ``RuntimeError: Signal source has been deleted``.

    复现路径: 用户停止 Bot → poller.stop() → driver.stop() 调 poller.deleteLater();
    此时若 ``_tick`` 已经把 runnable 投到 QThreadPool, runnable 还在后台 HTTP,
    后台跑完 emit signal → 崩溃.
    """

    def test_stop_disconnects_processes_signal(self, monkeypatch) -> None:
        """``stop()`` 应断开 in-flight runnable 的 processes_signal 连接."""
        import src.core.runtime.snowluma_status_poller as poller_mod

        captured: list = []

        class _Pool:
            def start(self, runnable):
                captured.append(runnable)

        monkeypatch.setattr(poller_mod.QThreadPool, "globalInstance", lambda: _Pool())

        poller = _make_poller()
        state_payloads: list = []
        poller.state_changed.connect(lambda qq, st: state_payloads.append((qq, st)))

        poller._tick()

        assert len(captured) == 1
        runnable = captured[0]
        assert poller._in_flight_runnable is runnable

        # poller stop: 模拟用户停止 Bot
        poller.stop()
        assert poller._in_flight_runnable is None

        # 后台 runnable 这时才跑完 emit (模拟 HTTP 终于回来)
        # 槽连接已被 stop 断开, _on_processes 不应被触发, 不会崩溃
        runnable.processes_signal.emit([], [], [])
        runnable.error_signal.emit("simulated late error")

        # poller 已 dispose, 不应再发任何 logged_in/disconnected (除了 stop 本身那条)
        # stop() 内部会 emit 一次 disconnected
        assert state_payloads == [(poller.qq_id, "disconnected")]

    def test_on_processes_releases_in_flight_reference(self, monkeypatch) -> None:
        """正常路径: runnable 跑完后槽入口应释放 in-flight 引用, 防止泄漏堆积."""
        import src.core.runtime.snowluma_status_poller as poller_mod

        captured: list = []

        class _Pool:
            def start(self, runnable):
                captured.append(runnable)

        monkeypatch.setattr(poller_mod.QThreadPool, "globalInstance", lambda: _Pool())

        poller = _make_poller()
        poller._tick()
        runnable = captured[0]
        assert poller._in_flight_runnable is runnable

        # runnable 正常跑完 emit processes_signal
        runnable.processes_signal.emit([], [], [])

        assert poller._in_flight_runnable is None
