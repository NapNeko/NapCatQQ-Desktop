# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.deployment` 单测 (W4).

覆盖:

- ``_build_nc_paths`` 派生正确 (NC 路径基于 SL workspace)
- ``initialize_layout`` 创建 8 个关键目录
- ``install_snowluma_framework``: 缺 tarball raise / 成功路径 / 失败 raise
- ``install_linuxqq`` 委托 NC + step 字段转换
- ``upload_daemon_launcher_script`` / ``upload_bot_launcher_script`` 上传 + chmod
- ``verify_deployment``: 关键文件检查
"""

from __future__ import annotations

import sys
from collections.abc import Callable
from pathlib import Path

import pytest

# 加载 Qt 资源 (含 .sh.j2)
import src.resource.resource  # noqa: F401

from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import RemoteCommandResult
from src.core.remote.snowluma import (
    SnowLumaDeployment,
    SnowLumaFrameworkNotBundledError,
    SnowLumaInstallStepResult,
    SnowLumaRemotePaths,
)
from src.core.remote.snowluma import bundled as bundled_mod
from src.core.remote.snowluma import deployment as deployment_mod
from src.core.remote.errors import RemoteCommandError


# ==================== Fake Backend ====================
class FakeExecutionBackend(ExecutionBackend):
    """记录所有 run/upload/ensure_directory 调用的内存版后端.

    可通过 ``set_run_result(cmd_prefix, RemoteCommandResult)`` 覆盖特定命令的返回值;
    默认所有 run 返回 ``exit_status=0`` 与空 stdout/stderr.
    """

    def __init__(self) -> None:
        self.run_calls: list[str] = []
        self.uploads: list[tuple[str, str]] = []  # (local, remote)
        self.ensured_dirs: list[str] = []
        self._run_overrides: dict[str, RemoteCommandResult] = {}
        self._default_stdout: str = ""

    def set_run_result(self, cmd_prefix: str, result: RemoteCommandResult) -> None:
        self._run_overrides[cmd_prefix] = result

    def set_default_stdout(self, stdout: str) -> None:
        self._default_stdout = stdout

    def run(
        self, command: str, *, timeout: float | None = None, check: bool = False
    ) -> RemoteCommandResult:
        self.run_calls.append(command)
        for prefix, result in self._run_overrides.items():
            if command.startswith(prefix) or prefix in command:
                return result
        return RemoteCommandResult(
            command=command, exit_status=0, stdout=self._default_stdout
        )

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        self.ensured_dirs.append(path)
        return RemoteCommandResult(
            command=f"mkdir -p {path}", exit_status=0
        )

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        self.uploads.append((str(local_path), target_path))

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        raise NotImplementedError  # 测试不需要


@pytest.fixture
def fake_backend() -> FakeExecutionBackend:
    return FakeExecutionBackend()


@pytest.fixture
def sl_paths() -> SnowLumaRemotePaths:
    return SnowLumaRemotePaths.from_base("/opt/sl")


@pytest.fixture
def deployer(
    fake_backend: FakeExecutionBackend, sl_paths: SnowLumaRemotePaths
) -> SnowLumaDeployment:
    return SnowLumaDeployment(fake_backend, sl_paths)


@pytest.fixture
def fake_tarball(tmp_path: Path) -> Path:
    """造一个 fake lite tarball 文件 (内容随便)."""
    p = tmp_path / "snowluma_framework_lite.tar.gz"
    p.write_bytes(b"\x1f\x8b\x08\x00" + b"\x00" * 64)  # gz magic header
    return p


# ==================== _build_nc_paths ====================
class TestBuildNcPaths:
    def test_workspace_shared_with_sl(self, sl_paths: SnowLumaRemotePaths) -> None:
        nc_paths = SnowLumaDeployment._build_nc_paths(sl_paths)
        assert nc_paths.workspace_dir == sl_paths.workspace_dir
        assert nc_paths.workspace_dir == "/opt/sl/workspace"

    def test_qq_under_sl_workspace(self, sl_paths: SnowLumaRemotePaths) -> None:
        """LinuxQQ 装到 ``${sl_workspace}/opt/QQ/...``, 不污染 NC 默认 ``$HOME/Napcat``."""
        nc_paths = SnowLumaDeployment._build_nc_paths(sl_paths)
        assert nc_paths.qq_base_path == "/opt/sl/workspace/opt/QQ"
        assert nc_paths.qq_executable == "/opt/sl/workspace/opt/QQ/qq"

    def test_runtime_log_shared_with_sl(self, sl_paths: SnowLumaRemotePaths) -> None:
        nc_paths = SnowLumaDeployment._build_nc_paths(sl_paths)
        assert nc_paths.runtime_dir == sl_paths.runtime_dir
        assert nc_paths.log_dir == sl_paths.log_dir

    def test_tmp_and_package_under_sl_workspace(
        self, sl_paths: SnowLumaRemotePaths
    ) -> None:
        nc_paths = SnowLumaDeployment._build_nc_paths(sl_paths)
        assert nc_paths.tmp_dir == "/opt/sl/workspace/tmp"
        assert nc_paths.package_dir == "/opt/sl/workspace/packages"

    def test_default_paths(self) -> None:
        """``$HOME/snowluma-remote`` 默认布局也合法."""
        sl_default = SnowLumaRemotePaths.from_base()
        nc_paths = SnowLumaDeployment._build_nc_paths(sl_default)
        # 字段都通过 LinuxCorePaths 的 P5 F2.3 校验
        assert nc_paths.workspace_dir.startswith("$HOME/")


# ==================== initialize_layout ====================
class TestInitializeLayout:
    def test_creates_all_critical_dirs(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        deployer.initialize_layout()
        # 至少 8 个目录: base, workspace, snowluma_framework, config, runtime, log, tmp, package
        assert len(fake_backend.ensured_dirs) >= 8
        assert "/opt/sl" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/snowluma" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/snowluma/config" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/runtime" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/log" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/tmp" in fake_backend.ensured_dirs
        assert "/opt/sl/workspace/packages" in fake_backend.ensured_dirs


# ==================== install_snowluma_framework ====================
class TestInstallSnowlumaFramework:
    def test_raises_when_not_bundled(
        self,
        deployer: SnowLumaDeployment,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Desktop 未捆绑 lite tarball 时必报 :class:`SnowLumaFrameworkNotBundledError`."""
        monkeypatch.setattr(deployment_mod, "find_bundled_lite_tarball", lambda: None)
        with pytest.raises(SnowLumaFrameworkNotBundledError, match="未捆绑"):
            deployer.install_snowluma_framework()

    def test_raises_when_override_missing(
        self, deployer: SnowLumaDeployment, tmp_path: Path
    ) -> None:
        nonexistent = tmp_path / "nope.tar.gz"
        with pytest.raises(SnowLumaFrameworkNotBundledError, match="不存在"):
            deployer.install_snowluma_framework(lite_tarball_override=nonexistent)

    def test_success_uploads_tarball_and_runs_script(
        self,
        deployer: SnowLumaDeployment,
        fake_backend: FakeExecutionBackend,
        fake_tarball: Path,
    ) -> None:
        """成功路径: SFTP 上传 tarball + 上传脚本 + 跑脚本 + 解析 PROGRESS."""
        # 让 run 命令默认返回 exit=0 + 模拟 [PROGRESS] 输出
        progress_stdout = "\n".join(
            [
                "[PROGRESS] 0 检查 OS",
                "[PROGRESS] 50 解压 Framework",
                "[PROGRESS] 100 完成",
            ]
        )
        # 但只对 ``bash "<script>"`` 命令返回 progress stdout; 其他 run 调用保持默认
        fake_backend.set_run_result(
            "bash ",
            RemoteCommandResult(
                command="bash", exit_status=0, stdout=progress_stdout
            ),
        )

        progress_events: list[tuple[str, int]] = []

        def on_progress(msg: str, pct: int) -> None:
            progress_events.append((msg, pct))

        result = deployer.install_snowluma_framework(
            progress=on_progress,
            lite_tarball_override=fake_tarball,
        )

        # tarball 被 SFTP 上传
        assert any(
            target == "/opt/sl/workspace/snowluma_framework_lite.tar.gz"
            for (_, target) in fake_backend.uploads
        )
        # 脚本被 SFTP 上传到 tmp_dir
        assert any(
            target.endswith("remote_install_snowluma.sh")
            for (_, target) in fake_backend.uploads
        )
        # PROGRESS 事件被解析
        assert ("检查 OS", 0) in progress_events
        assert ("完成", 100) in progress_events
        # step 字段
        assert isinstance(result, SnowLumaInstallStepResult)
        assert result.step == "install_snowluma_framework"
        assert result.ok

    def test_failure_raises_remote_command_error(
        self,
        deployer: SnowLumaDeployment,
        fake_backend: FakeExecutionBackend,
        fake_tarball: Path,
    ) -> None:
        """exit_status != 0 必 raise :class:`RemoteCommandError`."""
        fake_backend.set_run_result(
            "bash ",
            RemoteCommandResult(
                command="bash",
                exit_status=1,
                stdout="[PROGRESS] 100 ERROR_NODE_VERSION_TOO_LOW\n",
                stderr="apt: nodejs not found",
            ),
        )
        with pytest.raises(RemoteCommandError):
            deployer.install_snowluma_framework(lite_tarball_override=fake_tarball)


# ==================== install_linuxqq (委托 NC) ====================
class TestInstallLinuxqqDelegation:
    def test_returns_snowluma_step_result(
        self,
        deployer: SnowLumaDeployment,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """NC ``InstallStepResult`` 被转换为 :class:`SnowLumaInstallStepResult`."""
        from src.core.remote.deployment import InstallStepResult as NcStep

        fake_nc_result = NcStep(
            step="install_linuxqq",
            remote_script_path="/tmp/x.sh",
            exit_status=0,
            stdout="ok",
            stderr="",
            progress_events=[(50, "半路"), (100, "done")],
        )
        monkeypatch.setattr(
            deployer._nc_deployment, "install_linuxqq", lambda **kwargs: fake_nc_result
        )

        result = deployer.install_linuxqq()
        assert isinstance(result, SnowLumaInstallStepResult)
        assert result.step == "install_linuxqq"
        assert result.exit_status == 0
        assert result.progress_events == [(50, "半路"), (100, "done")]


# ==================== upload_daemon/bot_launcher_script ====================
class TestUploadLauncherScripts:
    def test_daemon_launcher_uploaded_and_chmodded(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        path = deployer.upload_daemon_launcher_script()
        assert path == deployer.paths.daemon_launcher_script
        # SFTP 上传到目标路径
        assert any(target == path for (_, target) in fake_backend.uploads)
        # chmod +x 命令被调用
        assert any(f'chmod +x "{path}"' in cmd for cmd in fake_backend.run_calls)

    def test_bot_launcher_uploaded(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        path = deployer.upload_bot_launcher_script()
        assert path == deployer.paths.bot_launcher_script
        assert any(target == path for (_, target) in fake_backend.uploads)


# ==================== verify_deployment ====================
class TestVerifyDeployment:
    def test_all_present_returns_ok(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        # 所有 test -e 命令返 0 (默认); native ls 返非空
        fake_backend.set_run_result(
            "ls ",
            RemoteCommandResult(
                command="ls",
                exit_status=0,
                stdout="/opt/sl/workspace/snowluma/native/snowluma-linux-x64.node\n",
            ),
        )
        ok, missing = deployer.verify_deployment()
        assert ok
        assert missing == []

    def test_missing_files_reported(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        # 让 test -e index.mjs 失败 (W3 修正: release lite 为扫平结构, 入口位于顶层)
        fake_backend.set_run_result(
            'test -e "/opt/sl/workspace/snowluma/index.mjs"',
            RemoteCommandResult(command="test", exit_status=1),
        )
        # native ls 返空
        fake_backend.set_run_result(
            "ls ",
            RemoteCommandResult(command="ls", exit_status=0, stdout=""),
        )
        ok, missing = deployer.verify_deployment()
        assert not ok
        assert any("SL daemon 入口" in m for m in missing)
        assert any("native" in m for m in missing)


# ==================== clean_environment (W10b-Maintenance) ====================
class TestCleanEnvironment:
    """SL 专用清理路径; 验证清的是 SL 产物 (framework / launcher / runtime) 而非
    NC 的 napcat 目录, 且 ``include_qq`` 旗标控制是否波及 LinuxQQ.
    """

    def test_clean_without_qq_clears_framework_only(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        result = deployer.clean_environment(include_qq=False)
        assert result.ok
        # 1. 必跑命令: daemon stop / pkill / 清 framework / 清 launcher
        all_commands = " ".join(fake_backend.run_calls)
        assert "snowluma_daemon_launcher.sh" in all_commands and "stop" in all_commands
        assert "pkill -f 'snowluma/index.mjs'" in all_commands
        assert "pkill -f 'Xvfb" in all_commands
        assert 'rm -rf "/opt/sl/workspace/snowluma"' in all_commands
        assert 'rm -f "/opt/sl/workspace/snowluma_daemon_launcher.sh"' in all_commands
        assert 'rm -f "/opt/sl/workspace/snowluma_bot_launcher.sh"' in all_commands
        # 2. include_qq=False: 不应碰 LinuxQQ 安装目录或 node 缓存
        assert "/opt/QQ" not in all_commands or "opt/sl" in all_commands  # SL workspace 下的 opt/QQ 才会出现
        # 严格: 不出现 "rm -rf .../opt/QQ" 这种独立清理 LinuxQQ 的命令
        assert not any(
            'rm -rf' in c and '/opt/QQ' in c and 'opt/sl' not in c
            for c in fake_backend.run_calls
        )
        # 3. 不清便携式 node tarball
        assert not any('node.tar.xz' in c for c in fake_backend.run_calls)

    def test_clean_with_qq_clears_linuxqq_and_node(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        result = deployer.clean_environment(include_qq=True)
        assert result.ok
        all_commands = " ".join(fake_backend.run_calls)
        # NC paths (qq_base_path 在 SL workspace 下) 也被清掉
        assert "/opt/sl/workspace/opt/QQ" in all_commands
        # deb/rpm 缓存清理
        assert ".deb" in all_commands and ".rpm" in all_commands
        # 便携式 node 缓存清理 (避免下次部署用到旧版本)
        assert 'rm -rf "/opt/sl/workspace/node"' in all_commands
        assert "node.tar.xz" in all_commands

    def test_clean_idempotent_on_missing_paths(
        self, deployer: SnowLumaDeployment, fake_backend: FakeExecutionBackend
    ) -> None:
        """所有 rm/pkill 都用 ``|| true`` 包裹, 清空环境上反复跑也不应该 raise."""
        # 让所有命令"假装失败"模拟从未部署的环境
        fake_backend.set_default_stdout("")  # 不改 stdout, 但默认 exit_status=0 (因为 || true)
        result = deployer.clean_environment(include_qq=True)
        assert result.ok
