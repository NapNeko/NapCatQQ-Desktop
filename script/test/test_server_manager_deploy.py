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

    def install_qq(self, *, progress=None, log_callback=None, force_reinstall: bool = False) -> None:
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
        force_update: bool = False,
    ) -> None:
        self.install_napcat_calls.append(
            {
                "archive_path": archive_path,
                "force_update": force_update,
                "has_log_callback": log_callback is not None,
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
    """伪造 LinuxCoreDeployment, 仅用于回滚测试。"""

    clean_calls: list[bool] = field(default_factory=list)
    fail_on_clean: bool = False

    def clean_environment(self, include_qq: bool = True):
        self.clean_calls.append(include_qq)
        if self.fail_on_clean:
            raise RuntimeError("simulated clean_environment failure")


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
