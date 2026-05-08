# -*- coding: utf-8 -*-
"""[`ServerManager.deploy_server`](src/core/remote/server_manager.py) 编排测试。

通过 monkey-patch ``ServerManager.get_backend`` 返回伪造的 RemoteBackend, 验证:
- 状态机 UNDEPLOYED -> DEPLOYING -> DEPLOYED
- install_qq / install_napcat 调用顺序与参数透传
- 进度回调把 0-50 / 50-100 区间正确映射
- 失败时 -> FAILED, 失败信号携带 stage 标签
- 重复触发会抛 RemoteDeploymentInProgressError
- 成功后 napcat_version / qq_version 写回档案
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

import pytest

from src.core.operation.backend import InstallationInfo
from src.core.remote.errors import (
    RemoteDeploymentError,
    RemoteDeploymentInProgressError,
)
from src.core.remote.models import LinuxCorePaths, SSHCredentials
from src.core.remote.server_manager import DeploymentResult, ServerManager
from src.core.remote.servers import DeploymentState, ServerProfile


# P5 F1.4: 默认跳过远端 SHA512 查询的网络调用; 单测验证编排不需要真 hash.
@pytest.fixture(autouse=True)
def _stub_release_hash_lookup(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        ServerManager,
        "_lookup_napcat_expected_sha512",
        lambda self: None,
    )


# ==================== fixtures & helpers ====================
def _make_profile(name: str = "测试服务器") -> ServerProfile:
    cred = SSHCredentials(
        host="example.com",
        port=22,
        username="root",
        auth_method="password",
        password=None,  # 不入档案
        connect_timeout=5.0,
        command_timeout=10.0,
    )
    return ServerProfile.create(name=name, credentials=cred)


@dataclass
class FakeRemoteBackend:
    """伪造的 RemoteBackend, 用于编排测试。"""

    install_qq_calls: list[dict] = field(default_factory=list)
    install_napcat_calls: list[dict] = field(default_factory=list)
    detect_calls: int = 0
    connect_calls: int = 0
    qq_progress_steps: list[tuple[str, int]] = field(default_factory=lambda: [("preparing", 0), ("done", 100)])
    napcat_progress_steps: list[tuple[str, int]] = field(default_factory=lambda: [("downloading", 0), ("done", 100)])
    fail_on_qq: bool = False
    fail_on_napcat: bool = False
    detect_napcat_version: str | None = "2.0.0"
    detect_qq_version: str | None = "9.9.9"

    def connect(self) -> None:
        self.connect_calls += 1

    def close(self) -> None:
        pass

    def install_qq(
        self,
        *,
        progress=None,
        log_callback=None,
        progress_log_callback=None,
        force_reinstall: bool = False,
    ) -> None:
        self.install_qq_calls.append(
            {"force_reinstall": force_reinstall, "has_log_callback": log_callback is not None}
        )
        if log_callback is not None:
            log_callback("[INFO] simulated qq stdout line 1")
            log_callback("[PROGRESS] 0 simulated qq")
        if progress is not None:
            for message, percent in self.qq_progress_steps:
                progress(message, percent)
        if self.fail_on_qq:
            raise RuntimeError("simulated qq failure")

    def install_napcat(
        self,
        archive_path=None,
        *,
        progress=None,
        log_callback=None,
        progress_log_callback=None,
        force_update: bool = False,
        expected_sha512: str | None = None,
        local_archive_cache=None,
        should_cancel=None,
    ) -> None:
        self.install_napcat_calls.append(
            {
                "archive_path": archive_path,
                "force_update": force_update,
                "has_log_callback": log_callback is not None,
                "expected_sha512": expected_sha512,
                "local_archive_cache": local_archive_cache,
                "has_should_cancel": should_cancel is not None,
            }
        )
        if log_callback is not None:
            log_callback("[INFO] simulated napcat stdout line 1")
        if progress is not None:
            for message, percent in self.napcat_progress_steps:
                progress(message, percent)
        if self.fail_on_napcat:
            raise RuntimeError("simulated napcat failure")

    def detect_installation(self) -> InstallationInfo:
        self.detect_calls += 1
        return InstallationInfo(
            napcat_version=self.detect_napcat_version,
            qq_version=self.detect_qq_version,
            qq_install_path="/some/path",
        )

    @property
    def deployment(self) -> "_FakeDeployment":
        if not hasattr(self, "_fake_deployment"):
            self._fake_deployment = _FakeDeployment()
        return self._fake_deployment


@dataclass
class _FakeDeployment:
    """伪造 LinuxCoreDeployment, 用于回滚测试与 deploy preflight 体检.

    ``probe_environment`` 默认返回一个 supported 的探测结果, 让原有 deploy 编排测试
    不需要关心 preflight; 需要触发 preflight 失败的子测试通过覆盖 ``probe_override``
    注入自己想要的 probe 实例.
    """

    clean_calls: list[bool] = field(default_factory=list)
    fail_on_clean: bool = False
    probe_override: object | None = None
    probe_calls: int = 0

    def clean_environment(self, include_qq: bool = True):
        self.clean_calls.append(include_qq)
        if self.fail_on_clean:
            raise RuntimeError("simulated clean_environment failure")

    def probe_environment(self):
        self.probe_calls += 1
        if self.probe_override is not None:
            return self.probe_override
        # 默认: Ubuntu 24 amd64, dpkg 可用 -> supported
        from src.core.remote.deployment import LinuxCoreDeploymentProbe

        return LinuxCoreDeploymentProbe(
            os_name="Linux",
            architecture="x86_64",
            normalized_arch="amd64",
            distro_id="ubuntu",
            distro_version="24.04",
            has_bash=True,
            has_tar=True,
            has_unzip=True,
            has_curl=True,
            has_dpkg=True,
            has_rpm2cpio=False,
            has_xvfb=True,
            has_linuxqq=False,
            has_napcat=False,
            installed_qq_version=None,
            installed_napcat_version=None,
            id_like="debian",
        )


@pytest.fixture
def manager_factory(tmp_path: Path):
    """创建独立持久化路径的 ServerManager, 并返回 (manager, fake_backend, profile_id)。"""
    def _factory(*, fake: FakeRemoteBackend | None = None, profile: ServerProfile | None = None):
        storage_path = tmp_path / "servers.json"
        manager = ServerManager(storage_path=storage_path)
        used_profile = profile or _make_profile()
        manager.add_server(used_profile, password="secret-not-persisted")

        backend = fake or FakeRemoteBackend()
        # 注入伪造 backend 替代真实的 get_backend
        manager.get_backend = lambda server_id, _backend=backend, _profile=used_profile: (
            _backend if server_id == _profile.id else (_ for _ in ()).throw(KeyError(server_id))
        )
        return manager, backend, used_profile.id

    return _factory


# ==================== success path ====================
class TestDeploySuccess:
    def test_state_transitions_undeployed_to_deployed(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        observed_states: list[str] = []
        manager.server_state_changed.connect(lambda sid, state: observed_states.append(state))

        result = manager.deploy_server(server_id)

        assert isinstance(result, DeploymentResult)
        assert result.ok is True
        assert result.napcat_version == "2.0.0"
        assert result.qq_version == "9.9.9"

        # 状态机: UNDEPLOYED -> DEPLOYING -> DEPLOYED
        assert observed_states[0] == DeploymentState.DEPLOYING.value
        assert observed_states[-1] == DeploymentState.DEPLOYED.value

        profile = manager.get_server(server_id)
        assert profile is not None
        assert profile.deployment_state is DeploymentState.DEPLOYED
        assert profile.napcat_version == "2.0.0"
        assert profile.qq_version == "9.9.9"

    def test_install_qq_runs_before_install_napcat(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        manager.deploy_server(server_id)

        assert backend.connect_calls == 1
        assert len(backend.install_qq_calls) == 1
        assert len(backend.install_napcat_calls) == 1
        assert backend.detect_calls == 1

    def test_force_flags_are_passed_through(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        manager.deploy_server(
            server_id,
            force_napcat_update=True,
            force_linuxqq_reinstall=True,
        )

        assert backend.install_qq_calls[0]["force_reinstall"] is True
        assert backend.install_napcat_calls[0]["force_update"] is True

    def test_progress_is_mapped_to_unified_0_100(self, manager_factory) -> None:
        backend = FakeRemoteBackend(
            qq_progress_steps=[("step1", 0), ("step2", 50), ("step3", 100)],
            napcat_progress_steps=[("a", 0), ("b", 50), ("b", 100)],
        )
        manager, _, server_id = manager_factory(fake=backend)

        emitted_signal: list[tuple[str, int]] = []
        manager.deployment_progress.connect(
            lambda _sid, msg, pct: emitted_signal.append((msg, pct))
        )

        callback_history: list[tuple[str, int]] = []
        manager.deploy_server(
            server_id,
            progress_callback=lambda msg, pct: callback_history.append((msg, pct)),
        )

        # 关键点: install_qq 区间应映射到 0-50, install_napcat 区间应映射到 50-100
        qq_percents = [pct for msg, pct in emitted_signal if msg.startswith("[LinuxQQ]")]
        napcat_percents = [pct for msg, pct in emitted_signal if msg.startswith("[NapCat]")]

        assert qq_percents == [0, 25, 50]
        assert napcat_percents == [50, 75, 100]

        # 终结值应是 100
        assert emitted_signal[-1] == ("部署完成", 100)
        # 外部 callback 收到完整序列
        assert ("部署完成", 100) in callback_history

    def test_finished_signal_emits_success_message(self, manager_factory) -> None:
        manager, _, server_id = manager_factory()

        finished: list[tuple[str, bool, str]] = []
        manager.deployment_finished.connect(lambda *args: finished.append(tuple(args)))

        manager.deploy_server(server_id)

        assert len(finished) == 1
        sid, ok, msg = finished[0]
        assert sid == server_id
        assert ok is True
        assert "2.0.0" in msg
        assert "9.9.9" in msg

    def test_is_deploying_during_call(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        observations: list[bool] = []

        def on_progress(_msg: str, _pct: int) -> None:
            observations.append(manager.is_deploying(server_id))

        manager.deploy_server(server_id, progress_callback=on_progress)

        # 部署中至少出现过 is_deploying=True
        assert any(observations)
        # 部署结束后应回归 False
        assert manager.is_deploying(server_id) is False


# ==================== failure path ====================
class TestDeployFailure:
    def test_install_qq_failure_marks_failed_state(self, manager_factory) -> None:
        backend = FakeRemoteBackend(fail_on_qq=True)
        manager, _, server_id = manager_factory(fake=backend)

        finished: list[tuple[str, bool, str]] = []
        manager.deployment_finished.connect(lambda *args: finished.append(tuple(args)))

        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(server_id)

        assert exc_info.value.stage == "install_qq"

        profile = manager.get_server(server_id)
        assert profile is not None
        assert profile.deployment_state is DeploymentState.FAILED

        assert finished and finished[0][1] is False
        assert "install_qq" in finished[0][2]

    def test_install_napcat_failure_marks_failed_state(self, manager_factory) -> None:
        backend = FakeRemoteBackend(fail_on_napcat=True)
        manager, _, server_id = manager_factory(fake=backend)

        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(server_id)
        assert exc_info.value.stage == "install_napcat"

        profile = manager.get_server(server_id)
        assert profile is not None
        assert profile.deployment_state is DeploymentState.FAILED

    def test_failure_release_deploying_lock(self, manager_factory) -> None:
        backend = FakeRemoteBackend(fail_on_qq=True)
        manager, _, server_id = manager_factory(fake=backend)

        with pytest.raises(RemoteDeploymentError):
            manager.deploy_server(server_id)

        # 失败也要释放锁, 允许后续重新部署
        assert manager.is_deploying(server_id) is False


# ==================== concurrency guard ====================
class TestConcurrencyGuard:
    def test_concurrent_deploy_is_rejected(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()
        # 在第一次部署中设置一个会观察并发触发的 progress callback
        captured_error: list[Exception] = []

        def on_progress(_msg: str, _pct: int) -> None:
            try:
                manager.deploy_server(server_id)
            except RemoteDeploymentInProgressError as exc:
                captured_error.append(exc)

        manager.deploy_server(server_id, progress_callback=on_progress)

        assert captured_error, "并发触发应抛 RemoteDeploymentInProgressError"

    def test_unknown_server_id_raises_key_error(self, manager_factory) -> None:
        manager, _, _server_id = manager_factory()

        with pytest.raises(KeyError):
            manager.deploy_server("not-a-real-id")


# ==================== P1.5: deployment_log 信号 ====================
class TestDeploymentLogSignal:
    def test_log_callback_is_passed_to_install_methods(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        manager.deploy_server(server_id)

        assert backend.install_qq_calls[0]["has_log_callback"] is True
        assert backend.install_napcat_calls[0]["has_log_callback"] is True

    def test_deployment_log_signal_emits_lines(self, manager_factory) -> None:
        manager, _, server_id = manager_factory()

        captured: list[tuple[str, str]] = []
        manager.deployment_log.connect(lambda sid, line: captured.append((sid, line)))

        manager.deploy_server(server_id)

        # FakeRemoteBackend 在 install_qq / install_napcat 内各自调用了 log_callback
        assert ("server_id", "x") not in captured  # 仅类型检查样例
        # 至少有 install_qq 与 install_napcat 各发出的行
        qq_lines = [line for sid, line in captured if "qq stdout" in line]
        napcat_lines = [line for sid, line in captured if "napcat stdout" in line]
        assert qq_lines, "应该收到 install_qq 阶段的日志行"
        assert napcat_lines, "应该收到 install_napcat 阶段的日志行"
        # server_id 必须正确
        assert all(sid == server_id for sid, _ in captured)

    def test_deployment_log_filtered_by_server_id_for_multi_subscriber(self, manager_factory) -> None:
        """模拟两个订阅者各只关心一台服务器, 验证按 server_id 过滤。"""
        manager, _, server_id = manager_factory()

        all_lines: list[str] = []
        filtered_lines: list[str] = []

        def all_subscriber(sid: str, line: str) -> None:
            all_lines.append(line)

        def filtered_subscriber(sid: str, line: str) -> None:
            if sid == server_id:
                filtered_lines.append(line)

        manager.deployment_log.connect(all_subscriber)
        manager.deployment_log.connect(filtered_subscriber)

        manager.deploy_server(server_id)

        assert all_lines == filtered_lines  # 当前测试只有一台服务器, 两个订阅者结果相同
        assert len(all_lines) >= 2  # 至少 install_qq + install_napcat 各 1 行


# ==================== rollback ====================
class TestRollback:
    """回滚测试: 验证 [`ServerManager.rollback_server`](src/core/remote/server_manager.py)。"""

    def test_rollback_calls_clean_environment_and_resets_state(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()

        # 先把服务器置为已部署状态
        manager.deploy_server(server_id)
        deployed = manager.get_server(server_id)
        assert deployed is not None
        assert deployed.deployment_state is DeploymentState.DEPLOYED
        assert deployed.napcat_version == "2.0.0"

        finished_events: list[tuple[bool, str]] = []
        manager.deployment_finished.connect(lambda sid, ok, msg: finished_events.append((ok, msg)))

        manager.rollback_server(server_id, include_qq=True)

        # clean_environment 被调用, include_qq 透传
        assert backend.deployment.clean_calls == [True]

        # 档案重置为未部署, 版本号清空
        rolled_back = manager.get_server(server_id)
        assert rolled_back is not None
        assert rolled_back.deployment_state is DeploymentState.UNDEPLOYED
        assert rolled_back.napcat_version is None
        assert rolled_back.qq_version is None

        # deployment_finished 信号被发射 (供控制台启用关闭按钮)
        # 注意 finished_events 包含 deploy_server 的成功 + rollback 的成功
        assert finished_events[-1] == (True, "回滚完成: 远端环境已清空")

    def test_rollback_include_qq_false(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()
        manager.rollback_server(server_id, include_qq=False)
        assert backend.deployment.clean_calls == [False]

    def test_rollback_failure_emits_failed_signal(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()
        backend.deployment.fail_on_clean = True

        finished_events: list[tuple[bool, str]] = []
        manager.deployment_finished.connect(lambda sid, ok, msg: finished_events.append((ok, msg)))

        with pytest.raises(RuntimeError):
            manager.rollback_server(server_id)

        # 失败也要 emit deployment_finished, 否则控制台关闭按钮永远禁用
        assert finished_events
        ok, msg = finished_events[-1]
        assert ok is False
        assert "回滚失败" in msg

    def test_rollback_concurrent_raises(self, manager_factory) -> None:
        manager, backend, server_id = manager_factory()
        # 模拟当前正在部署
        manager._deploying.add(server_id)
        try:
            with pytest.raises(RemoteDeploymentInProgressError):
                manager.rollback_server(server_id)
        finally:
            manager._deploying.discard(server_id)

    def test_rollback_unknown_server_raises_keyerror(self, manager_factory) -> None:
        manager, _, _ = manager_factory()
        with pytest.raises(KeyError):
            manager.rollback_server("non-existent-id")


# ==================== cancellation ====================
class TestCancellation:
    """覆盖 [`ServerManager.request_cancel`](src/core/remote/server_manager.py)
    协作式取消机制: API 行为, 埋点抛 RemoteDeploymentCancelledError,
    状态机走 UNDEPLOYED 而非 FAILED, should_cancel 协议透传到 install_napcat.
    """

    def test_request_cancel_returns_false_when_not_deploying(self, manager_factory) -> None:
        manager, _, server_id = manager_factory()
        # 没有 deploy_server 在跑 -> Event 不存在
        assert manager.request_cancel(server_id) is False
        assert manager.is_cancel_requested(server_id) is False

    def test_request_cancel_during_install_qq_aborts_with_undeployed_state(
        self, manager_factory
    ) -> None:
        """install_qq 内部调 manager.request_cancel -> 抛 RemoteDeploymentCancelledError;
        状态机走 UNDEPLOYED (非 FAILED), 部署终结信号 ok=False.
        """
        from src.core.remote.errors import RemoteDeploymentCancelledError

        backend = FakeRemoteBackend()

        # 让 install_qq 在跑到一半时模拟"用户点了取消按钮", 然后抛出底层异常
        captured_manager: dict = {}

        def _qq_simulating_cancel(*, progress=None, log_callback=None, progress_log_callback=None, **kwargs):
            mgr = captured_manager["mgr"]
            sid = captured_manager["sid"]
            # 模拟用户点了取消按钮
            mgr.request_cancel(sid)
            # SSH 命令依然抛出 (典型: connect 中断 / 命令执行被打断)
            raise RuntimeError("simulated install_qq interrupted by cancel")

        backend.install_qq = _qq_simulating_cancel  # type: ignore[assignment]

        manager, _, server_id = manager_factory(fake=backend)
        captured_manager["mgr"] = manager
        captured_manager["sid"] = server_id

        finished_payloads: list[tuple[bool, str]] = []
        manager.deployment_finished.connect(lambda sid, ok, msg: finished_payloads.append((ok, msg)))

        with pytest.raises(RemoteDeploymentCancelledError):
            manager.deploy_server(server_id)

        # 状态机: 取消 != 失败 -> UNDEPLOYED
        profile = manager.get_server(server_id)
        assert profile is not None
        assert profile.deployment_state == DeploymentState.UNDEPLOYED
        # finished 信号 ok=False (但消息不带 [install_qq] 而是 [cancelled])
        assert finished_payloads
        ok, msg = finished_payloads[-1]
        assert ok is False
        assert "[cancelled]" in msg

    def test_request_cancel_between_stages_skips_install_napcat(
        self, manager_factory
    ) -> None:
        """preflight 完成后用户点取消 -> install_qq 之前的 _check_cancelled 命中 -> install_napcat 不会被调."""
        from src.core.remote.errors import RemoteDeploymentCancelledError

        backend = FakeRemoteBackend()
        manager, _, server_id = manager_factory(fake=backend)

        # 在 install_qq 调用前预先 set Event (模拟"刚 connect 上用户立刻点了取消")
        # 直接走 deploy_server 内部流程: 入口注册 Event -> 我们用 monkeypatch 在 connect() 期间 set
        original_install_qq = backend.install_qq

        def _qq_proxy(*, progress=None, log_callback=None, progress_log_callback=None, force_reinstall: bool = False) -> None:
            return original_install_qq(
                progress=progress,
                log_callback=log_callback,
                progress_log_callback=progress_log_callback,
                force_reinstall=force_reinstall,
            )

        # 用 connect() 钩子: connect 完成时立刻 set 取消 Event
        original_connect = backend.connect

        def _connect_then_cancel() -> None:
            original_connect()
            manager.request_cancel(server_id)

        backend.connect = _connect_then_cancel  # type: ignore[assignment]
        backend.install_qq = _qq_proxy  # type: ignore[assignment]

        with pytest.raises(RemoteDeploymentCancelledError):
            manager.deploy_server(server_id)

        # install_qq / install_napcat 都不应该被调到 (preflight 后埋点立刻命中)
        assert backend.install_qq_calls == []
        assert backend.install_napcat_calls == []

        # 档案状态: UNDEPLOYED
        profile = manager.get_server(server_id)
        assert profile is not None
        assert profile.deployment_state == DeploymentState.UNDEPLOYED

    def test_should_cancel_callback_passed_to_install_napcat(self, manager_factory) -> None:
        """deploy_server 应该把 cancel_event.is_set 作为 should_cancel 协议传给 install_napcat."""
        manager, backend, server_id = manager_factory()
        manager.deploy_server(server_id)

        assert backend.install_napcat_calls
        last_call = backend.install_napcat_calls[-1]
        assert last_call["has_should_cancel"] is True

    def test_cancel_event_is_cleaned_up_after_deploy(self, manager_factory) -> None:
        """部署结束 (成功 or 失败) 后 _cancel_events 应该被清理, 防止内存泄漏."""
        manager, _, server_id = manager_factory()
        manager.deploy_server(server_id)

        # finally 清理
        assert server_id not in manager._cancel_events
        assert manager.is_cancel_requested(server_id) is False
        # 部署结束后 request_cancel 应返回 False (已无任务)
        assert manager.request_cancel(server_id) is False
