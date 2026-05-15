# -*- coding: utf-8 -*-
"""[`RemotePage`](src/ui/page/remote_page/__init__.py) P3.W2 维护 / 回滚 UI 入口测试.

回归保护点 (对应 [`docs/general/remote_ssh_p3_plan.md`](../../docs/general/remote_ssh_p3_plan.md) §3.2):

- "刷新版本(单台)" 仅启动一个 [`RedetectRunner`](src/ui/page/remote_page/deployment_runner.py)
- "强制更新 NapCat" / "强制重装 LinuxQQ" 启动 [`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py)
  并传对应的 force 参数
- "回滚部署" 弹 [`RollbackConfirmBox`](src/ui/page/remote_page/maintenance_dialog.py),
  按用户选择把 ``include_qq`` 透传给 [`RollbackRunner`](src/ui/page/remote_page/deployment_runner.py)
- ``_update_button_state`` 在 ``DeploymentState`` 不同值下正确 enable/disable 维护与回滚按钮

不依赖真实 SSH; 通过 monkeypatch:
- 替换 ``it(ServerManager)`` 为内置 fake
- 替换 ``QThreadPool.globalInstance().start`` 捕获 runner
- ``AskBox`` / ``RollbackConfirmBox`` 的 ``exec`` 返回值由 monkeypatch 控制
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any

import pytest
from PySide6.QtWidgets import QApplication

import src.ui.page.remote_page as remote_page_mod
from src.core.remote.models import LinuxCorePaths, SSHCredentials
from src.core.remote.servers import DeploymentState, ServerProfile
from src.ui.page.remote_page import RemotePage
from src.ui.page.remote_page.deployment_runner import (
    DeploymentRunner,
    RedetectRunner,
    RollbackRunner,
)
from src.ui.page.remote_page.maintenance_dialog import RollbackConfirmBox


# ==================== 辅助 ====================
def _ensure_qapp() -> QApplication:
    """创建或复用测试用 offscreen QApplication."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _make_profile(*, server_id: str = "srv-1", state: DeploymentState = DeploymentState.DEPLOYED) -> ServerProfile:
    """构造测试用 ServerProfile."""
    cred = SSHCredentials(
        host="192.0.2.10",
        username="napcat",
        auth_method="password",
        password=None,  # 密码不落档案, 由 password_cache 管理
    )
    profile = ServerProfile.create(
        name=f"测试服务器-{server_id}",
        credentials=cred,
        notes="",
        paths=LinuxCorePaths(),
    )
    # 覆写 id 与 state 为可预测值
    profile.id = server_id
    profile.deployment_state = state
    return profile


@dataclass
class _FakeServerManager:
    """``ServerManager`` 替身, 仅暴露 [`RemotePage`](src/ui/page/remote_page/__init__.py) 实际调用到的方法."""

    profiles: list[ServerProfile] = field(default_factory=list)
    deploying: set[str] = field(default_factory=set)
    _password_cache: dict[str, str] = field(default_factory=dict)

    def list_servers(self) -> list[ServerProfile]:
        return list(self.profiles)

    def get_server(self, server_id: str) -> ServerProfile | None:
        for p in self.profiles:
            if p.id == server_id:
                return p
        return None

    def is_deploying(self, server_id: str) -> bool:
        return server_id in self.deploying

    # 信号占位 - RemotePage 在 _connect_manager_signals 里 .connect 了一组回调
    class _DummySignal:
        def connect(self, _slot: Any) -> None:
            return None

    server_added = _DummySignal()
    server_updated = _DummySignal()
    server_removed = _DummySignal()
    server_state_changed = _DummySignal()
    deployment_progress = _DummySignal()
    deployment_finished = _DummySignal()


@dataclass
class _CapturedRunners:
    """捕获 ``QThreadPool.start(runner)`` 的所有调用."""

    started: list[Any] = field(default_factory=list)


@pytest.fixture
def captured_runners(monkeypatch: pytest.MonkeyPatch) -> _CapturedRunners:
    """把 ``QThreadPool.globalInstance().start`` 替换为捕获器, 不真正调度 runner."""
    captured = _CapturedRunners()

    class _FakePool:
        def start(self, runner: Any) -> None:
            captured.started.append(runner)

    fake_pool = _FakePool()

    class _FakeQThreadPool:
        @staticmethod
        def globalInstance() -> _FakePool:  # noqa: N802 - 模拟 Qt 命名
            return fake_pool

    monkeypatch.setattr(remote_page_mod, "QThreadPool", _FakeQThreadPool)
    # P3 perf W4: RemotePage 的所有 SSH 派发改走 ``remote_ssh_pool()``;
    # 把它也指向同一个 fake_pool, 让捕获器能拿到 deploy / redetect / rollback runner.
    monkeypatch.setattr(remote_page_mod, "remote_ssh_pool", lambda: fake_pool)
    return captured


@pytest.fixture
def fake_manager(monkeypatch: pytest.MonkeyPatch) -> _FakeServerManager:
    """注入 [`_FakeServerManager`](script/test/test_remote_page_actions.py) 到 ``it(...)``."""
    manager = _FakeServerManager()

    def _fake_it(_cls: Any) -> _FakeServerManager:
        return manager

    monkeypatch.setattr(remote_page_mod, "it", _fake_it)
    return manager


@pytest.fixture
def remote_page(
    fake_manager: _FakeServerManager,
    captured_runners: _CapturedRunners,  # noqa: ARG001 - 仅需启用 fixture
    monkeypatch: pytest.MonkeyPatch,
) -> RemotePage:
    """构造一个最小可用的 [`RemotePage`](src/ui/page/remote_page/__init__.py).

    避免触发部署控制台 / PageStyleSheet 等副作用.
    """
    _ensure_qapp()
    page = RemotePage()
    # 直接构建 UI 而不走 initialize() 的 setParent / PageStyleSheet 副作用
    page.setObjectName("RemotePage")
    page._build_ui()
    page._connect_manager_signals()
    page._reload()
    # 屏蔽部署控制台弹窗 (单元测试不需要真实窗口)
    monkeypatch.setattr(page, "_open_or_focus_console", lambda *_a, **_k: None)
    monkeypatch.setattr(page, "_ensure_usage_notice_accepted", lambda *_a, **_k: True)
    return page


def _select_first_server(page: RemotePage, manager: _FakeServerManager, profile: ServerProfile) -> None:
    """把 profile 加入 manager, 触发 _reload, 并选中它."""
    manager.profiles.append(profile)
    # 给密码认证模式准备好密码缓存, 防止被 "未保存密码" 提前 return
    manager._password_cache[profile.id] = "pw"
    page._reload()
    page.select_server(profile.id)


# ==================== RollbackConfirmBox ====================
class TestRollbackConfirmBox:
    def test_default_includes_qq(self) -> None:
        """对话框默认应勾选 ``include_qq=True``, 与 ServerManager.rollback_server 默认一致."""
        _ensure_qapp()
        # 不传父窗口可避免 main_window 依赖; MessageBoxBase 容许 parent=None
        from PySide6.QtWidgets import QWidget

        parent = QWidget()
        dialog = RollbackConfirmBox("测试服务器", parent)
        try:
            assert dialog.get_include_qq() is True
        finally:
            dialog.deleteLater()

    def test_unchecking_returns_false(self) -> None:
        _ensure_qapp()
        from PySide6.QtWidgets import QWidget

        parent = QWidget()
        dialog = RollbackConfirmBox("测试服务器", parent)
        try:
            dialog.include_qq_checkbox.setChecked(False)
            assert dialog.get_include_qq() is False
        finally:
            dialog.deleteLater()


# ==================== _refresh_card_states ====================
class TestButtonState:
    def test_no_servers_no_cards(self, remote_page: RemotePage) -> None:
        """空服务器列表时不应有任何卡片 (即没有任何按钮可被点击)."""
        assert remote_page._cards == {}

    @pytest.mark.parametrize(
        ("state", "maintenance_expected"),
        [
            (DeploymentState.UNDEPLOYED, False),
            (DeploymentState.DEPLOYING, False),
            (DeploymentState.DEPLOYED, True),
            # FAILED 仍能用 maintenance_btn 进入对话框选择 "回滚"
            (DeploymentState.FAILED, True),
        ],
    )
    def test_state_drives_enabled_flags(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        state: DeploymentState,
        maintenance_expected: bool,
    ) -> None:
        """卡片维护按钮应根据部署状态自动 enable/disable.

        v3 重构后, 维护是单一入口 (打开 [`MaintenanceDialog`]):
        - UNDEPLOYED / DEPLOYING: 禁用 (无可用维护项)
        - DEPLOYED: 启用 (4 项均可用)
        - FAILED:   启用 (用户可通过对话框选 "回滚" 清理失败残留)
        """
        profile = _make_profile(state=state)
        if state is DeploymentState.DEPLOYING:
            fake_manager.deploying.add(profile.id)
        _select_first_server(remote_page, fake_manager, profile)

        card = remote_page._cards[profile.id]
        assert card.maintenance_btn.isEnabled() is maintenance_expected, (
            f"state={state} 下维护按钮应 enabled={maintenance_expected}"
        )


# ==================== A: 单台刷新 / 强制更新 ====================
class TestMaintenanceActions:
    def test_redetect_versions_selected_starts_redetect_runner(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()

        remote_page._on_redetect_versions_selected()

        assert len(captured_runners.started) == 1
        runner = captured_runners.started[0]
        assert isinstance(runner, RedetectRunner)
        assert runner._server_id == profile.id  # type: ignore[attr-defined]

    def test_redetect_skipped_when_no_selection(
        self,
        remote_page: RemotePage,
        captured_runners: _CapturedRunners,
    ) -> None:
        captured_runners.started.clear()
        remote_page._on_redetect_versions_selected()
        assert captured_runners.started == [], "无选中时不应启动任何 runner"

    def test_force_update_napcat_runs_deployment_runner_with_napcat_flag(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()

        # 让 AskBox.exec 返回 1 (accepted)
        monkeypatch.setattr(
            "src.ui.page.remote_page.AskBox.exec",
            lambda self: 1,
        )

        remote_page._on_force_update_napcat()

        assert len(captured_runners.started) == 1
        runner = captured_runners.started[0]
        assert isinstance(runner, DeploymentRunner)
        assert runner._server_id == profile.id  # type: ignore[attr-defined]
        assert runner._force_napcat_update is True  # type: ignore[attr-defined]
        assert runner._force_linuxqq_reinstall is False  # type: ignore[attr-defined]

    def test_force_reinstall_linuxqq_runs_deployment_runner_with_linuxqq_flag(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()
        monkeypatch.setattr(
            "src.ui.page.remote_page.AskBox.exec",
            lambda self: 1,
        )

        remote_page._on_force_reinstall_linuxqq()

        assert len(captured_runners.started) == 1
        runner = captured_runners.started[0]
        assert isinstance(runner, DeploymentRunner)
        assert runner._force_napcat_update is False  # type: ignore[attr-defined]
        assert runner._force_linuxqq_reinstall is True  # type: ignore[attr-defined]

    def test_force_update_aborted_on_user_cancel(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """二次确认对话框被取消时不应启动任何 runner."""
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()
        monkeypatch.setattr("src.ui.page.remote_page.AskBox.exec", lambda self: 0)

        remote_page._on_force_update_napcat()

        assert captured_runners.started == [], "用户取消后不应启动 DeploymentRunner"


# ==================== F: 回滚部署 ====================
class TestRollbackAction:
    def test_rollback_starts_runner_with_include_qq_true(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()

        # RollbackConfirmBox.exec=1, get_include_qq=True
        monkeypatch.setattr(RollbackConfirmBox, "exec", lambda self: 1)
        monkeypatch.setattr(RollbackConfirmBox, "get_include_qq", lambda self: True)

        remote_page._on_rollback()

        assert len(captured_runners.started) == 1
        runner = captured_runners.started[0]
        assert isinstance(runner, RollbackRunner)
        assert runner._server_id == profile.id  # type: ignore[attr-defined]
        assert runner._include_qq is True  # type: ignore[attr-defined]

    def test_rollback_starts_runner_with_include_qq_false(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()

        monkeypatch.setattr(RollbackConfirmBox, "exec", lambda self: 1)
        monkeypatch.setattr(RollbackConfirmBox, "get_include_qq", lambda self: False)

        remote_page._on_rollback()

        runner = captured_runners.started[0]
        assert isinstance(runner, RollbackRunner)
        assert runner._include_qq is False  # type: ignore[attr-defined]

    def test_rollback_aborted_on_user_cancel(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        captured_runners.started.clear()
        monkeypatch.setattr(RollbackConfirmBox, "exec", lambda self: 0)

        remote_page._on_rollback()

        assert captured_runners.started == [], "用户取消时不应启动 RollbackRunner"

    def test_rollback_skipped_when_already_deploying(
        self,
        remote_page: RemotePage,
        fake_manager: _FakeServerManager,
        captured_runners: _CapturedRunners,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        profile = _make_profile()
        _select_first_server(remote_page, fake_manager, profile)
        # 模拟此时正在部署中
        fake_manager.deploying.add(profile.id)
        captured_runners.started.clear()

        # 即使用户接受对话框也不应启动 - 但我们不应到达对话框
        monkeypatch.setattr(RollbackConfirmBox, "exec", lambda self: 1)

        remote_page._on_rollback()

        assert captured_runners.started == [], "deploy_in_progress 时应直接 return, 不启动 runner"
