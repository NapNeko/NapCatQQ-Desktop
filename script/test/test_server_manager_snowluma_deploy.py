# -*- coding: utf-8 -*-
""":class:`ServerManager._deploy_snowluma_flavor` 集成测试 (W8).

策略: mock ``SSHClient`` + ``SnowLumaDeployment`` 整链路, 仅验证 ``ServerManager``
的分支判断与 writeback 逻辑; 不真连 SSH 也不依赖远端 fixture.

覆盖:
- flavor=SNOWLUMA 时 ``deploy_server`` dispatch 到 ``_deploy_snowluma_flavor``
- 成功路径: framework_version 写回 + DeploymentState=DEPLOYED + finished 信号
- preflight ``unsupported`` → stage="preflight"
- 缺 dpkg → stage="preflight"
- ``SnowLumaFrameworkNotBundledError`` → stage="install_snowluma_framework"
- verify 失败 → stage="verify"
"""

from __future__ import annotations

from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest

from src.core.remote import (
    BackendFlavor,
    DeploymentState,
    ServerProfile,
    SSHCredentials,
)
from src.core.remote.errors import RemoteDeploymentError
from src.core.remote.server_manager import ServerManager
from src.core.remote.snowluma import SnowLumaRemotePaths


# ==================== fixture: ServerManager + SL profile ====================
@pytest.fixture
def manager(tmp_path: Path) -> ServerManager:
    return ServerManager(storage_path=tmp_path / "servers.json")


@pytest.fixture
def sl_profile() -> ServerProfile:
    cred = SSHCredentials(
        host="example.com",
        username="ubuntu",
        port=22,
        auth_method="password",
        password="x",
    )
    return ServerProfile.create(
        name="sl-test",
        credentials=cred,
        backend_flavor=BackendFlavor.SNOWLUMA,
        snowluma_paths=SnowLumaRemotePaths.from_base("/opt/sl"),
    )


def _make_probe(
    *,
    has_dpkg: bool = True,
    compat_status: str = "supported",
    distro_id: str | None = "ubuntu",
    distro_version: str | None = "22.04",
    arch: str = "amd64",
) -> MagicMock:
    """构造模拟 :class:`LinuxCoreDeploymentProbe`."""
    probe = MagicMock()
    probe.has_dpkg = has_dpkg
    probe.has_rpm2cpio = False
    probe.distro_id = distro_id
    probe.distro_version = distro_version
    probe.normalized_arch = arch
    probe.architecture = arch

    report = MagicMock()
    report.compat_status = compat_status
    report.family = "debian"
    report.distro_entry = None
    report.reasons = ()
    probe.evaluate_compatibility.return_value = report
    return probe


@pytest.fixture
def patched_ssh_and_deployment(
    monkeypatch: pytest.MonkeyPatch,
) -> dict[str, Any]:
    """把 ``SSHClient`` 与 ``SnowLumaDeployment`` 替换成可控 mock.

    通过 patch ``server_manager`` 模块的导入入口实现.
    """
    from src.core.remote import server_manager as sm_mod
    from src.core.remote.snowluma import deployment as sl_dep_mod

    # 1. SSHClient 替换为不做实际连接的 mock
    fake_ssh = MagicMock()
    fake_ssh.is_connected = False

    def _connect_side_effect() -> None:
        fake_ssh.is_connected = True

    fake_ssh.connect.side_effect = _connect_side_effect
    fake_ssh.close.side_effect = lambda: setattr(fake_ssh, "is_connected", False)

    def _ssh_factory(cred: Any) -> MagicMock:
        return fake_ssh

    monkeypatch.setattr(sm_mod, "SSHClient", _ssh_factory)

    # 2. SnowLumaDeployment 替换为 mock; 默认所有方法返回成功
    fake_deployer = MagicMock()
    fake_deployer.probe_environment.return_value = _make_probe()
    fake_deployer.install_linuxqq.return_value = MagicMock(ok=True)
    fake_deployer.install_snowluma_framework.return_value = MagicMock(ok=True)
    fake_deployer.upload_daemon_launcher_script.return_value = "/opt/sl/.../daemon.sh"
    fake_deployer.upload_bot_launcher_script.return_value = "/opt/sl/.../bot.sh"
    fake_deployer.verify_deployment.return_value = (True, [])

    def _deployer_factory(backend: Any, paths: Any) -> MagicMock:
        return fake_deployer

    monkeypatch.setattr(sm_mod, "RemoteExecutionBackend", lambda ssh: MagicMock())
    # 关键: 把 lazy import 也 patch (代码里 ``from src.core.remote.snowluma import SnowLumaDeployment``)
    import src.core.remote.snowluma as sl_pkg

    monkeypatch.setattr(sl_pkg, "SnowLumaDeployment", _deployer_factory)
    monkeypatch.setattr(sl_pkg, "read_bundled_version", lambda: "0.1.0")
    # Framework error class 保持原样 (raise 路径会用到)

    return {"ssh": fake_ssh, "deployer": fake_deployer, "sl_pkg": sl_pkg}


# ==================== 成功路径 ====================
class TestSuccessPath:
    def test_dispatch_to_snowluma_flavor(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        manager._registry.add(sl_profile)
        result = manager.deploy_server(sl_profile.id)

        assert result.ok
        # 4 个核心 deployer 方法都被调用
        deployer = patched_ssh_and_deployment["deployer"]
        deployer.probe_environment.assert_called_once()
        deployer.install_linuxqq.assert_called_once()
        deployer.install_snowluma_framework.assert_called_once()
        deployer.upload_daemon_launcher_script.assert_called_once()
        deployer.upload_bot_launcher_script.assert_called_once()
        deployer.verify_deployment.assert_called_once()

    def test_framework_version_written_back(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        manager._registry.add(sl_profile)
        manager.deploy_server(sl_profile.id)

        updated = manager._registry.get(sl_profile.id)
        assert updated is not None
        assert updated.snowluma_framework_version == "0.1.0"
        assert updated.deployment_state == DeploymentState.DEPLOYED

    def test_ssh_client_closed_after_deploy(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        manager._registry.add(sl_profile)
        manager.deploy_server(sl_profile.id)

        fake_ssh = patched_ssh_and_deployment["ssh"]
        fake_ssh.close.assert_called_once()


# ==================== preflight 失败 ====================
class TestPreflightFailure:
    def test_unsupported_compat_raises(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        deployer = patched_ssh_and_deployment["deployer"]
        bad_probe = _make_probe(compat_status="unsupported")
        bad_probe.evaluate_compatibility.return_value.reasons = ("CPU 架构不支持",)
        deployer.probe_environment.return_value = bad_probe

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "preflight"

    def test_missing_dpkg_raises_preflight(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        deployer = patched_ssh_and_deployment["deployer"]
        deployer.probe_environment.return_value = _make_probe(has_dpkg=False)

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "preflight"
        assert "dpkg" in str(exc_info.value)


# ==================== stage 错误映射 ====================
class TestStageErrorMapping:
    def test_framework_not_bundled_maps_to_install_stage(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        from src.core.remote.snowluma import SnowLumaFrameworkNotBundledError

        deployer = patched_ssh_and_deployment["deployer"]
        deployer.install_snowluma_framework.side_effect = (
            SnowLumaFrameworkNotBundledError("test missing")
        )

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "install_snowluma_framework"

    def test_install_linuxqq_failure_maps_to_install_qq(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        deployer = patched_ssh_and_deployment["deployer"]
        deployer.install_linuxqq.side_effect = RuntimeError("apt repo broken")

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "install_qq"

    def test_install_linuxqq_exit_37_maps_to_install_qq_verify(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        """退出码 37 (LinuxQQ 包完整性校验失败) 应该映射到独立 stage,
        让用户能区分 "网络不稳定导致包损坏" 与 "真正的安装失败"."""
        from src.core.remote.errors import RemoteCommandError

        deployer = patched_ssh_and_deployment["deployer"]
        deployer.install_linuxqq.side_effect = RemoteCommandError(
            command="bash install_linuxqq.sh",
            exit_status=37,
            stderr="QQ package download/verify failed after 3 attempts",
        )

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "install_qq_verify"
        assert "完整性校验" in str(exc_info.value)

    def test_verify_failure_maps_to_verify(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        deployer = patched_ssh_and_deployment["deployer"]
        # W3 修正: SL release lite 入口位于顶层 index.mjs (非旧假设的 dist/index.mjs)
        deployer.verify_deployment.return_value = (False, ["index.mjs"])

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError) as exc_info:
            manager.deploy_server(sl_profile.id)
        assert exc_info.value.stage == "verify"
        assert "index.mjs" in str(exc_info.value)


# ==================== W10b-Driver: get_backend 按 flavor 分发 ====================
class TestGetBackendDispatch:
    """ServerManager.get_backend() 必须按 backend_flavor 返回正确的 backend 实现.

    NC profile → RemoteBackend (NC launcher: napcat.sh)
    SL profile → RemoteSnowLumaBackend (SL launcher: snowluma_bot_launcher.sh + daemon)
    """

    def test_sl_profile_returns_snowluma_backend(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
    ) -> None:
        from src.core.operation.remote_snowluma_backend import RemoteSnowLumaBackend

        manager._registry.add(sl_profile)
        backend = manager.get_backend(sl_profile.id)
        assert isinstance(backend, RemoteSnowLumaBackend)
        # 路径必须是 SL 的 SnowLumaRemotePaths, 不是 LinuxCorePaths
        assert backend.sl_paths is sl_profile.snowluma_paths
        # 同一 server_id 二次调用必须返同一缓存实例
        backend2 = manager.get_backend(sl_profile.id)
        assert backend2 is backend

    def test_nc_profile_returns_napcat_backend(
        self,
        manager: ServerManager,
    ) -> None:
        from src.core.operation.remote_backend import RemoteBackend

        nc_cred = SSHCredentials(
            host="example.com",
            username="root",
            port=22,
            auth_method="password",
            password="x",
        )
        nc_profile = ServerProfile.create(
            name="nc-test",
            credentials=nc_cred,
            backend_flavor=BackendFlavor.NAPCAT,
        )
        manager._registry.add(nc_profile)
        backend = manager.get_backend(nc_profile.id)
        assert isinstance(backend, RemoteBackend)


# ==================== 状态机 ====================
class TestStateTransitions:
    def test_failed_deploy_sets_state_to_failed(
        self,
        manager: ServerManager,
        sl_profile: ServerProfile,
        patched_ssh_and_deployment: dict[str, Any],
    ) -> None:
        deployer = patched_ssh_and_deployment["deployer"]
        deployer.install_linuxqq.side_effect = RuntimeError("fail")

        manager._registry.add(sl_profile)
        with pytest.raises(RemoteDeploymentError):
            manager.deploy_server(sl_profile.id)

        updated = manager._registry.get(sl_profile.id)
        assert updated is not None
        assert updated.deployment_state == DeploymentState.FAILED
