# -*- coding: utf-8 -*-
"""SnowLumaDriver 单测 (W2 daemon 解耦重构后).

W2 之前: 这里测的是 ``build_node_process`` 与 ``_render_configs`` (driver 自己渲染 3 个
JSON + 构造 node QProcess). W2 之后这些职责迁到 :class:`SnowLumaDaemon`, 见
``test_snowluma_daemon.py``.

本文件聚焦 driver per-Bot 行为:

- ``_render_onebot_config``: driver 只渲染 ``onebot_<uin>.json`` (一 Bot 一文件)
- ``start_async`` (W2): 不再触发单实例守护 ``RuntimeError``; 多 Bot 都注册成功
- ``start_async`` 通过 mock 的 daemon 验证 ``ensure_running`` 被调
- ``stop`` 释放 daemon 引用计数

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.2 / §4.2,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W2.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest
from creart import it
from PySide6.QtCore import QProcess
from PySide6.QtWidgets import QApplication

from src.core.runtime.paths import PathFunc

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    """创建或复用测试用 offscreen QApplication."""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


# ==================== fixtures ====================
@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


@pytest.fixture
def snowluma_install(tmp_path: Path, monkeypatch) -> Path:
    """在 tmp_path 下伪造一份 SnowLuma 发布包, monkeypatch PathFunc.snowluma_path."""
    fake_root = tmp_path / "SnowLuma"
    fake_root.mkdir()
    (fake_root / "node.exe").write_bytes(b"\x4dZ")
    (fake_root / "index.mjs").write_text("// stub")
    (fake_root / "package.json").write_text('{"name":"@snowluma/runtime","version":"0.1.0"}')

    path_func = it(PathFunc)
    monkeypatch.setattr(path_func, "snowluma_path", fake_root)
    return fake_root


@pytest.fixture
def isolated_session(tmp_path: Path, monkeypatch) -> Path:
    """把 ``snowluma_session.session_path`` 重定向到 tmp_path."""
    fake_session_path = tmp_path / "snowluma-session.json"
    monkeypatch.setattr(
        "src.core.runtime.snowluma_session.session_path",
        lambda: fake_session_path,
    )
    return fake_session_path


@pytest.fixture
def fake_qq_path(tmp_path: Path, monkeypatch) -> Path:
    """伪造 QQ 安装路径; monkeypatch ``PathFunc.get_qq_path`` 返回它. COLD 模式需要."""
    fake_qq = tmp_path / "QQ"
    fake_qq.mkdir()
    (fake_qq / "QQ.exe").write_bytes(b"\x4dZ")
    path_func = it(PathFunc)
    monkeypatch.setattr(path_func, "get_qq_path", lambda: fake_qq)
    return fake_qq


@pytest.fixture
def mock_daemon(monkeypatch):
    """Monkeypatch ``it(SnowLumaDaemon)`` 返回 mock 实例; 同时把 driver 模块内
    的 ``it`` 替换成放行 PathFunc 但拦截 SnowLumaDaemon 的版本.
    """
    from src.core.runtime.snowluma_daemon import SnowLumaDaemon
    real_it = it

    daemon_mock = MagicMock(spec=SnowLumaDaemon)
    client_mock = MagicMock()
    client_mock.load_process.return_value = MagicMock(status="online", error="")
    daemon_mock.ensure_running.return_value = client_mock
    daemon_mock.webui_client.return_value = client_mock
    daemon_mock.is_running.return_value = True
    daemon_mock.release.return_value = None

    def _it_proxy(target):
        if target is SnowLumaDaemon:
            return daemon_mock
        return real_it(target)

    monkeypatch.setattr("src.core.runtime.snowluma_driver.it", _it_proxy)
    return {"daemon": daemon_mock, "client": client_mock}


def _make_bot_config(qqid: int, name: str = "TestBot"):
    """构造一份最小有效的 Config 对象 (SnowLuma 后端)."""
    from src.core.config.config_model import (
        AdvancedConfig,
        AutoRestartScheduleConfig,
        BotConfig,
        Config,
        ConnectConfig,
    )
    from src.core.runtime.backend_type import BackendType

    return Config(
        bot=BotConfig(
            name=name,
            QQID=qqid,
            musicSignUrl="",
            autoRestartSchedule=AutoRestartScheduleConfig(),
            offlineAutoRestart=False,
            backend_type=BackendType.SNOWLUMA,
        ),
        connect=ConnectConfig(),
        advanced=AdvancedConfig(),
    )


# ==================== _render_onebot_config (W2: 只渲染 onebot_<uin>.json) ====================
class TestRenderOnebotConfig:
    """W2: driver 不再渲染 runtime.json / webui.json (daemon 接管), 只渲染 onebot.json."""

    def test_renders_only_onebot_json(
        self, snowluma_install: Path, isolated_session: Path
    ) -> None:
        """W2: ``_render_onebot_config`` 只写 ``onebot_<uin>.json``;
        ``runtime.json`` / ``webui.json`` 由 daemon 负责, 本调用不应触发它们."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver

        config = _make_bot_config(qqid=20001)
        driver = SnowLumaDriver()
        driver._render_onebot_config(config)

        onebot_json = snowluma_install / "config" / f"onebot_{config.bot.QQID}.json"
        assert onebot_json.exists(), "onebot_<uin>.json 应被渲染"

        # daemon 全局配置不应被 driver 触碰
        runtime_json = snowluma_install / "config" / "runtime.json"
        webui_json = snowluma_install / "config" / "webui.json"
        assert not runtime_json.exists(), "runtime.json 不应由 driver 渲染 (daemon 职责)"
        assert not webui_json.exists(), "webui.json 不应由 driver 渲染 (daemon 职责)"

    def test_render_consumes_connect_config(
        self, snowluma_install: Path, isolated_session: Path
    ) -> None:
        """``_render_onebot_config`` 应把 ``config.connect`` 映射到 onebot.json."""
        import json

        from src.core.config.config_model import (
            ConnectConfig,
            HttpServersConfig,
        )
        from src.core.runtime.snowluma_driver import SnowLumaDriver

        config = _make_bot_config(qqid=30001)
        config.connect = ConnectConfig(
            httpServers=[
                HttpServersConfig(
                    name="user-http",
                    host="0.0.0.0",
                    port=4242,
                    token="USER-TOKEN",
                    path="/api",
                )
            ],
        )

        driver = SnowLumaDriver()
        driver._render_onebot_config(config)

        onebot_json = snowluma_install / "config" / f"onebot_{config.bot.QQID}.json"
        payload = json.loads(onebot_json.read_text(encoding="utf-8"))
        http = payload["networks"]["httpServers"][0]
        assert http["port"] == 4242
        assert http["accessToken"] == "USER-TOKEN"
        assert http["path"] == "/api"


# ==================== start_async (W2: 多 Bot 共享 daemon) ====================
class TestStartAsync:
    """W2 主目标: 多 Bot 启动均成功 + 共用 daemon."""

    def test_start_async_returns_handle_with_qq_primary_in_cold(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """COLD 模式下 ``ProcessHandle.primary_process`` 是 QQ.exe QProcess (非 None)."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=11001)
        handle, worker, session = driver.start_async(
            config, start_mode=SnowLumaStartMode.COLD_START
        )
        try:
            assert handle.qq_id == "11001"
            assert handle.primary_process is not None
            assert isinstance(handle.primary_process, QProcess)
            assert handle.secondary_process is None  # W2: daemon 持 node
            assert session is None  # W2: daemon 持 password, session 不再传给 driver
            # worker 携带的 daemon 应是 mock 注入的同一个
            assert worker._daemon is mock_daemon["daemon"]
        finally:
            # 清理: 把 model 从 driver 字典移除, 避免污染后续测试
            if handle.primary_process is not None:
                handle.primary_process.deleteLater()
            driver._processes.pop("11001", None)

    def test_start_async_hot_mode_primary_is_none(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_daemon: dict[str, Any],
        monkeypatch,
    ) -> None:
        """HOT 模式下 ``ProcessHandle.primary_process`` 应为 ``None``."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        # mock psutil.Process 让 HOT 校验通过
        import psutil

        class _FakePsutilProcess:
            def __init__(self, pid: int) -> None:
                pass

        monkeypatch.setattr(psutil, "Process", _FakePsutilProcess)

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=12001)
        handle, _worker, _session = driver.start_async(
            config, start_mode=SnowLumaStartMode.HOT_START, attach_pid=99999
        )
        try:
            assert handle.qq_id == "12001"
            assert handle.primary_process is None
            assert handle.secondary_process is None
            # qq_pid 应填好为 attach_pid
            model = driver.get_process_model("12001")
            assert model is not None
            assert model.qq_pid == 99999
            assert model.qq_process is None
        finally:
            driver._processes.pop("12001", None)

    def test_multi_bot_starts_without_runtime_error(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """W2 核心: 连续启动 2 个 Bot 都进 driver 字典, 不再抛 ``一期仅支持 1 个 SnowLuma Bot``."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        driver = SnowLumaDriver()

        config_a = _make_bot_config(qqid=21001, name="BotA")
        config_b = _make_bot_config(qqid=21002, name="BotB")

        handle_a, worker_a, _ = driver.start_async(
            config_a, start_mode=SnowLumaStartMode.COLD_START
        )
        # 关键断言: 第二个 Bot 启动**不**抛 RuntimeError (单实例守护已删).
        handle_b, worker_b, _ = driver.start_async(
            config_b, start_mode=SnowLumaStartMode.COLD_START
        )

        try:
            assert "21001" in driver._processes
            assert "21002" in driver._processes
            assert len(driver._processes) == 2
            # 两个 worker 引用同一个 daemon mock
            assert worker_a._daemon is worker_b._daemon
        finally:
            for h in (handle_a, handle_b):
                if h.primary_process is not None:
                    h.primary_process.deleteLater()
            driver._processes.clear()

    def test_start_async_duplicate_qqid_raises(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """同 QQID 重复启动应被 driver 拒绝 (新单实例守护: per-QQID, 不是 per-driver)."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=30099)
        handle, _w, _ = driver.start_async(config)

        try:
            with pytest.raises(RuntimeError, match="已在跑"):
                driver.start_async(config)
        finally:
            if handle.primary_process is not None:
                handle.primary_process.deleteLater()
            driver._processes.pop("30099", None)


# ==================== Phase C worker 调 daemon ====================
class TestPhaseCWorker:
    """W2: Phase C worker 调 ``daemon.ensure_running`` + ``client.load_process``."""

    def test_phase_c_worker_calls_daemon_and_inject(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """Worker.run 应调 ``daemon.ensure_running`` + ``client.load_process``, 然后 emit succeeded."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=33001)
        handle, worker, _ = driver.start_async(
            config, start_mode=SnowLumaStartMode.COLD_START
        )
        # 把 qq_pid 写一个非 0 值, 否则 load_process(0) 触发奇怪逻辑
        driver._processes["33001"].qq_pid = 12345

        # 捕获 emit
        succeeded_payloads: list = []
        failed_payloads: list[str] = []
        worker.succeeded.connect(lambda client: succeeded_payloads.append(client))
        worker.failed.connect(lambda msg: failed_payloads.append(msg))

        # 直接调 run (绕过 QThreadPool, 同步跑)
        worker.run()

        try:
            assert mock_daemon["daemon"].ensure_running.called
            mock_daemon["client"].load_process.assert_called_once_with(12345)
            assert len(succeeded_payloads) == 1
            assert succeeded_payloads[0] is mock_daemon["client"]
            assert failed_payloads == []
        finally:
            if handle.primary_process is not None:
                handle.primary_process.deleteLater()
            driver._processes.pop("33001", None)


# ==================== stop ====================
class TestStop:
    """W2: ``stop`` 应调 ``daemon.release()`` 释放引用计数."""

    def test_stop_releases_daemon_ref_count(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """stop 应调 ``daemon.release`` 一次."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver, SnowLumaStartMode

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=44001)
        handle, _w, _ = driver.start_async(
            config, start_mode=SnowLumaStartMode.COLD_START
        )
        driver._processes["44001"].qq_pid = 12345  # 让 unload 有 pid 跑

        # stop
        driver.stop("44001")

        # 断言: daemon.release 被调; model 已从字典移除
        mock_daemon["daemon"].release.assert_called_once()
        assert "44001" not in driver._processes

        if handle.primary_process is not None:
            handle.primary_process.deleteLater()

    def test_stop_unknown_qqid_is_no_op(
        self, snowluma_install: Path, isolated_session: Path, mock_daemon: dict[str, Any]
    ) -> None:
        """未注册的 qq_id 调 stop 应静默 no-op, **不**调 daemon.release."""
        from src.core.runtime.snowluma_driver import SnowLumaDriver

        driver = SnowLumaDriver()
        driver.stop("nonexistent-id")
        mock_daemon["daemon"].release.assert_not_called()


# ==================== ProcessModel 字段裁剪 ====================
class TestProcessModelFields:
    """W2: ``SnowLumaProcessModel`` 字段裁剪验证."""

    def test_model_does_not_have_node_process_field(self) -> None:
        """W2: ``node_process`` / ``webui_client`` / ``auth_token`` / ``effective_password`` 已删."""
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        model = SnowLumaProcessModel(qq_id="x")
        assert not hasattr(model, "node_process")
        assert not hasattr(model, "webui_client")
        assert not hasattr(model, "auth_token")
        assert not hasattr(model, "effective_password")

    def test_model_has_new_uin_and_ancillary_pids(self) -> None:
        """W2 新增: ``uin`` (默认 "") 和 ``ancillary_pids`` (默认 set())."""
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        model = SnowLumaProcessModel(qq_id="x")
        assert model.uin == ""
        assert model.ancillary_pids == set()
        assert isinstance(model.ancillary_pids, set)


# ==================== _start_phase_a_processes_async 异步语义 (2026-05-11 主线程卡顿修复) ====================
class TestStartPhaseAAsync:
    """**主线程卡顿修复**: ``_start_phase_a_processes_async`` 必须是 signal-driven 异步,
    主线程不阻塞 ``waitForStarted``.

    历史 (旧版同步): ``_start_phase_a_processes`` 内含 ``qq_process.waitForStarted(5000)``,
    在主线程阻塞最多 5 秒等 OS 启动 QQ.exe; 用户实测【启动 Bot】点击瞬间 UI 明显卡顿.

    现在 (新版异步): ``qq_process.start()`` 后立即返回, 连 ``started`` 信号; 信号 emit 时
    填 ``model.qq_pid`` 并调 ``on_started`` 让 manager 推进 Phase C.
    """

    def test_hot_mode_calls_on_started_synchronously(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
        monkeypatch,
    ) -> None:
        """HOT 模式 (``model.qq_process is None``): ``_start_phase_a_processes_async`` 应
        **同步直调** ``on_started(model)`` (PID 已知, 无需等 OS 启动).
        """
        import psutil

        from src.core.runtime.snowluma_driver import (
            SnowLumaDriver,
            SnowLumaStartMode,
        )

        # mock psutil.Process 让 HOT 校验通过
        monkeypatch.setattr(
            psutil,
            "Process",
            lambda pid: MagicMock(),
        )

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=55001)
        handle, _w, _ = driver.start_async(
            config, start_mode=SnowLumaStartMode.HOT_START, attach_pid=99999
        )

        try:
            model = driver.get_process_model("55001")
            assert model is not None
            assert model.qq_process is None  # HOT: 无 QProcess

            calls: list[Any] = []
            driver._start_phase_a_processes_async(
                model, on_started=lambda m: calls.append(m)
            )

            # 同步直调, 不需事件循环
            assert len(calls) == 1
            assert calls[0] is model
        finally:
            driver._processes.pop("55001", None)

    def test_cold_mode_does_not_block_main_thread(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """COLD 模式: ``_start_phase_a_processes_async`` 调用应**立即返回**, 不调
        ``waitForStarted`` 阻塞主线程.

        QQ.exe 实际不会在测试 fixture 下启动 (write_bytes b"\\x4dZ" 不是有效 PE),
        但本测试只验证调用立即返回 — ``on_started`` 永远不被调因为没真实 ``started``
        信号; 关键是函数本身**不阻塞**.
        """
        import time

        from src.core.runtime.snowluma_driver import (
            SnowLumaDriver,
            SnowLumaStartMode,
        )

        driver = SnowLumaDriver()
        config = _make_bot_config(qqid=55002)
        handle, _w, _ = driver.start_async(
            config, start_mode=SnowLumaStartMode.COLD_START
        )

        try:
            model = driver.get_process_model("55002")
            assert model is not None
            assert model.qq_process is not None  # COLD: 有 QProcess

            calls: list[Any] = []
            t0 = time.monotonic()
            driver._start_phase_a_processes_async(
                model, on_started=lambda m: calls.append(m)
            )
            elapsed = time.monotonic() - t0

            # 关键断言: 函数立即返回 (远小于旧版 waitForStarted 5s timeout / 即便
            # QQ 启动最快 100ms, 旧版同步等也至少 100ms; 异步必须 <100ms 才能算修复).
            assert elapsed < 0.5, (
                f"_start_phase_a_processes_async 耗时 {elapsed:.3f}s 太长, "
                f"应 signal-driven 异步立即返回"
            )

            # on_started 在测试环境永远不会被调 (假 QQ.exe 不会真启动)
            assert calls == []
        finally:
            # 清理: kill QProcess (即便没启动也调 kill 安全)
            qq_process = model.qq_process if model is not None else None
            if qq_process is not None:
                qq_process.kill()
                qq_process.waitForFinished(1000)
                qq_process.deleteLater()
            driver._processes.pop("55002", None)

    def test_cold_mode_started_signal_triggers_callback(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        fake_qq_path: Path,
        mock_daemon: dict[str, Any],
    ) -> None:
        """COLD 模式: 模拟 ``QProcess.started`` 信号 emit, ``on_started`` 应被调,
        且 ``model.qq_pid`` 被设.

        通过 mock ``model.qq_process`` 为可控 QObject 子类, 手动 emit ``started``
        信号验证回调链.
        """
        from PySide6.QtCore import QObject, Signal

        from src.core.runtime.snowluma_driver import (
            SnowLumaDriver,
            SnowLumaProcessModel,
        )

        # 自构造 fake QProcess (只需 ``started`` / ``start`` / ``processId``)
        class _FakeQQProcess(QObject):
            started = Signal()
            errorOccurred = Signal(int)
            stateChanged = Signal(int)
            finished = Signal(int, int)

            def __init__(self, fake_pid: int) -> None:
                super().__init__()
                self._fake_pid = fake_pid
                self.start_called = False

            def start(self) -> None:
                self.start_called = True

            def processId(self) -> int:
                return self._fake_pid

        fake_qq = _FakeQQProcess(fake_pid=88888)
        model = SnowLumaProcessModel(qq_id="55003", qq_process=fake_qq)  # type: ignore[arg-type]

        driver = SnowLumaDriver()
        driver._processes["55003"] = model

        try:
            calls: list[Any] = []
            driver._start_phase_a_processes_async(
                model, on_started=lambda m: calls.append(m)
            )

            # 此时 start() 已调, 但 started 信号尚未 emit, on_started 不应被调
            assert fake_qq.start_called is True
            assert calls == []
            assert model.qq_pid == 0  # 初始, 未填

            # 模拟 OS 启动完成: emit started 信号
            fake_qq.started.emit()

            # 现在 on_started 应被调, model.qq_pid 应被填
            assert len(calls) == 1
            assert calls[0] is model
            assert model.qq_pid == 88888
        finally:
            driver._processes.pop("55003", None)
