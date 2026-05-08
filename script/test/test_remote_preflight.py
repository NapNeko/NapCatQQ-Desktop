# -*- coding: utf-8 -*-
"""[`ServerManager.deploy_server`](src/core/remote/server_manager.py) 的
**preflight 兼容性体检** 路径测试 (扩展 SSH 支持边界后引入).

复用 [`test_server_manager_deploy.py`](script/test/test_server_manager_deploy.py)
的 ``FakeRemoteBackend`` / ``_FakeDeployment``, 通过往 ``probe_override`` 注入不同
``LinuxCoreDeploymentProbe`` 实例覆盖以下分支:

- supported -> install_qq / install_napcat 正常被调用; ``[PREFLIGHT]`` 行 emit
- unsupported -> ``RemoteDeploymentError(stage="preflight")``; install_qq **不** 被调用
- unknown_but_runnable -> 走完整流程 + 额外 emit 一条警告 ``[PREFLIGHT]`` 行
"""
from __future__ import annotations

from pathlib import Path

import pytest

from src.core.remote.deployment import LinuxCoreDeploymentProbe
from src.core.remote.errors import RemoteDeploymentError
from src.core.remote.server_manager import ServerManager
from src.core.remote.servers import DeploymentState

# 复用同目录下的 fake backend (符合现有测试组织习惯)
from script.test.test_server_manager_deploy import FakeRemoteBackend, _make_profile


# 跳过 SHA512 网络查询, 与 test_server_manager_deploy 保持一致
@pytest.fixture(autouse=True)
def _stub_release_hash_lookup(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        ServerManager,
        "_lookup_napcat_expected_sha512",
        lambda self: None,
    )


def _make_probe(**overrides) -> LinuxCoreDeploymentProbe:
    base = dict(
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
        id_like=None,
    )
    base.update(overrides)
    return LinuxCoreDeploymentProbe(**base)


@pytest.fixture
def manager_with_backend(tmp_path: Path):
    """创建 ServerManager + FakeRemoteBackend, 返回 (manager, backend, server_id)."""

    def _factory(probe: LinuxCoreDeploymentProbe):
        storage_path = tmp_path / "servers.json"
        manager = ServerManager(storage_path=storage_path)
        profile = _make_profile()
        manager.add_server(profile, password="secret")

        backend = FakeRemoteBackend()
        # 拿到 backend.deployment 后设 probe_override
        backend.deployment.probe_override = probe

        manager.get_backend = lambda server_id, _b=backend, _p=profile: (
            _b if server_id == _p.id else (_ for _ in ()).throw(KeyError(server_id))
        )
        return manager, backend, profile.id

    return _factory


# ==================== supported ====================
def test_preflight_supported_runs_full_pipeline(manager_with_backend) -> None:
    manager, backend, server_id = manager_with_backend(_make_probe(distro_id="ubuntu"))

    log_lines: list[str] = []
    manager.deployment_log.connect(lambda _sid, line: log_lines.append(line))

    result = manager.deploy_server(server_id)

    assert result.ok is True
    assert backend.deployment.probe_calls == 1
    assert len(backend.install_qq_calls) == 1
    assert len(backend.install_napcat_calls) == 1

    preflight_lines = [line for line in log_lines if line.startswith("[PREFLIGHT]")]
    assert preflight_lines, "supported 路径仍应至少 emit 一条 [PREFLIGHT] 行"
    summary = next((l for l in preflight_lines if "status=supported" in l), None)
    assert summary is not None
    assert "distro=Ubuntu" in summary
    assert "family=debian" in summary
    assert "installer=dpkg" in summary


# ==================== unsupported ====================
def test_preflight_unsupported_arch_blocks_install(manager_with_backend) -> None:
    probe = _make_probe(architecture="riscv64", normalized_arch=None)
    manager, backend, server_id = manager_with_backend(probe)

    finished: list[tuple[str, bool, str]] = []
    manager.deployment_finished.connect(lambda *args: finished.append(tuple(args)))
    log_lines: list[str] = []
    manager.deployment_log.connect(lambda _sid, line: log_lines.append(line))

    with pytest.raises(RemoteDeploymentError) as exc_info:
        manager.deploy_server(server_id)

    assert exc_info.value.stage == "preflight"
    # install_qq / install_napcat 都不应被触发
    assert backend.install_qq_calls == []
    assert backend.install_napcat_calls == []
    # 状态机回到 FAILED
    profile = manager.get_server(server_id)
    assert profile is not None
    assert profile.deployment_state is DeploymentState.FAILED
    # 失败信号文案应含中文原因
    assert finished
    assert finished[0][1] is False
    assert "兼容性体检" in finished[0][2]
    assert any("CPU 架构" in line for line in log_lines)


def test_preflight_unsupported_distro_blocks_install(manager_with_backend) -> None:
    """centos 但只有 dpkg 没有 rpm2cpio -> installer mismatch -> unsupported."""
    probe = _make_probe(distro_id="centos", has_dpkg=True, has_rpm2cpio=False)
    manager, backend, server_id = manager_with_backend(probe)

    with pytest.raises(RemoteDeploymentError) as exc_info:
        manager.deploy_server(server_id)

    assert exc_info.value.stage == "preflight"
    assert backend.install_qq_calls == []
    # 锁应释放
    assert manager.is_deploying(server_id) is False


# ==================== unknown_but_runnable ====================
def test_preflight_unknown_but_runnable_logs_warning_and_continues(manager_with_backend) -> None:
    probe = _make_probe(distro_id="arch", has_dpkg=True)  # arch 不在 KNOWN_DISTROS
    manager, backend, server_id = manager_with_backend(probe)

    log_lines: list[str] = []
    manager.deployment_log.connect(lambda _sid, line: log_lines.append(line))

    result = manager.deploy_server(server_id)

    assert result.ok is True
    # 走通完整流程
    assert len(backend.install_qq_calls) == 1
    assert len(backend.install_napcat_calls) == 1

    # 必须 emit 一条警告级 [PREFLIGHT] 行 (含"未识别")
    warning_lines = [l for l in log_lines if l.startswith("[PREFLIGHT]") and "未识别" in l]
    assert warning_lines, "unknown_but_runnable 必须 emit 警告行"


# ==================== friendly_errors 联动 ====================
def test_preflight_error_message_is_user_friendly(manager_with_backend) -> None:
    """[`to_friendly`](src/core/remote/friendly_errors.py) 应识别 stage='preflight'."""
    from src.core.remote.friendly_errors import to_friendly

    probe = _make_probe(architecture="riscv64", normalized_arch=None)
    manager, _, server_id = manager_with_backend(probe)

    try:
        manager.deploy_server(server_id)
    except RemoteDeploymentError as exc:
        message = to_friendly(exc)
        assert "兼容性体检" in message
        # 不能有英文 stage 漏出
        assert "preflight" not in message.lower() or "兼容性体检" in message
        return
    pytest.fail("应当抛出 RemoteDeploymentError(stage='preflight')")
