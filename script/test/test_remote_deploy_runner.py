# -*- coding: utf-8 -*-
"""[`LinuxCoreDeployment.install_linuxqq`](src/core/remote/deployment.py)
与 [`install_napcat`](src/core/remote/deployment.py) 单元测试。

测试以 [`FakeExecutionBackend`](script/test/test_remote_deploy_probe.py)
为基础, 验证脚本上传顺序、进度协议解析、错误传播。
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Iterable

import pytest

from src.core.remote.deployment import (
    InstallStepResult,
    LinuxCoreDeployment,
)
from src.core.remote.errors import RemoteCommandError
from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import LinuxCorePaths, RemoteCommandResult


@dataclass
class _RecordingBackend(ExecutionBackend):
    """记录所有调用并按命令前缀返回伪造结果的测试后端。"""

    bash_stdout: str = ""
    bash_stderr: str = ""
    bash_exit_status: int = 0

    history: list[str] = field(default_factory=list)
    upload_calls: list[tuple[str, str]] = field(default_factory=list)
    ensure_dir_calls: list[str] = field(default_factory=list)

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        self.history.append(command)
        if command.startswith("chmod +x"):
            return RemoteCommandResult(command=command, exit_status=0)
        if "bash " in command and command.rstrip().endswith(".sh\""):
            return RemoteCommandResult(
                command=command,
                exit_status=self.bash_exit_status,
                stdout=self.bash_stdout,
                stderr=self.bash_stderr,
            )
        return RemoteCommandResult(command=command, exit_status=0, stdout="", stderr="")

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        self.ensure_dir_calls.append(path)
        return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

    def upload_file(self, local_path, target_path: str) -> None:
        self.upload_calls.append((str(local_path), target_path))

    def download_file(self, source_path: str, local_path) -> None:  # pragma: no cover
        pass


def _make_backend(
    *,
    progress_lines: Iterable[tuple[int, str]] = (),
    extra_stdout: str = "",
    bash_exit_status: int = 0,
    bash_stderr: str = "",
) -> _RecordingBackend:
    progress_block = "\n".join(f"[PROGRESS] {p} {m}" for p, m in progress_lines)
    stdout = (progress_block + ("\n" if progress_block else "") + extra_stdout).strip()
    return _RecordingBackend(
        bash_stdout=stdout,
        bash_stderr=bash_stderr,
        bash_exit_status=bash_exit_status,
    )


class TestInstallLinuxQQ:
    def test_success_emits_progress_events(self) -> None:
        backend = _make_backend(
            progress_lines=[(0, "preparing workspace"), (50, "extracting"), (100, "linuxqq install finished")],
        )
        deployment = LinuxCoreDeployment(backend)

        emitted: list[tuple[str, int]] = []

        def progress(message: str, percent: int) -> None:
            emitted.append((message, percent))

        result = deployment.install_linuxqq(progress=progress)

        assert isinstance(result, InstallStepResult)
        assert result.step == "install_linuxqq"
        assert result.ok is True
        assert result.exit_status == 0
        assert result.progress_events == [
            (0, "preparing workspace"),
            (50, "extracting"),
            (100, "linuxqq install finished"),
        ]
        assert emitted == [
            ("preparing workspace", 0),
            ("extracting", 50),
            ("linuxqq install finished", 100),
        ]

    def test_uploads_install_linuxqq_script(self) -> None:
        backend = _make_backend()
        deployment = LinuxCoreDeployment(backend)

        deployment.install_linuxqq()

        targets = [t for _, t in backend.upload_calls]
        assert any(target.endswith("/remote_install_linuxqq.sh") for target in targets), targets

    def test_force_reinstall_sets_env_var(self) -> None:
        backend = _make_backend()
        deployment = LinuxCoreDeployment(backend)

        deployment.install_linuxqq(force_reinstall=True)

        bash_calls = [cmd for cmd in backend.history if cmd.startswith("FORCE_LINUXQQ_REINSTALL=1 bash ")]
        assert len(bash_calls) == 1, backend.history

    def test_failure_raises_remote_command_error(self) -> None:
        backend = _make_backend(
            bash_exit_status=33,
            bash_stderr="[ERROR] download failed",
            extra_stdout="[ERROR] curl: connection refused",
        )
        deployment = LinuxCoreDeployment(backend)

        with pytest.raises(RemoteCommandError) as exc_info:
            deployment.install_linuxqq()

        assert exc_info.value.exit_status == 33
        assert "download failed" in exc_info.value.stderr or "connection refused" in exc_info.value.stderr

    def test_no_progress_callback_does_not_explode(self) -> None:
        backend = _make_backend(progress_lines=[(50, "ok")])
        deployment = LinuxCoreDeployment(backend)

        result = deployment.install_linuxqq()  # progress=None

        assert result.ok is True
        assert result.progress_events == [(50, "ok")]


class TestInstallNapCat:
    def test_force_update_sets_env_var(self) -> None:
        backend = _make_backend()
        deployment = LinuxCoreDeployment(backend)

        deployment.install_napcat(force_update=True)

        bash_calls = [cmd for cmd in backend.history if "FORCE_NAPCAT_UPDATE=1" in cmd]
        assert any("bash " in cmd for cmd in bash_calls), backend.history

    def test_uploads_napcat_script_and_launcher(self) -> None:
        backend = _make_backend()
        deployment = LinuxCoreDeployment(backend)

        deployment.install_napcat()

        targets = [t for _, t in backend.upload_calls]
        # NapCat 安装脚本
        assert any(t.endswith("/remote_install_napcat.sh") for t in targets), targets
        # launcher 脚本被部署到 workspace_dir/napcat.sh
        paths = LinuxCorePaths()
        assert any(t == paths.launcher_script for t in targets), targets

    def test_custom_download_url_is_quoted_and_passed(self) -> None:
        backend = _make_backend()
        deployment = LinuxCoreDeployment(backend)

        deployment.install_napcat(download_url="https://example.com/x.zip")

        bash_calls = [cmd for cmd in backend.history if "NAPCAT_DOWNLOAD_URL=" in cmd]
        assert bash_calls, backend.history
        assert "https://example.com/x.zip" in bash_calls[0]

    def test_failure_does_not_upload_launcher(self) -> None:
        backend = _make_backend(bash_exit_status=37, bash_stderr="missing archive path")
        deployment = LinuxCoreDeployment(backend)

        with pytest.raises(RemoteCommandError):
            deployment.install_napcat()

        # 失败时 launcher 不应被部署(在脚本失败后才执行的代码)
        targets = [t for _, t in backend.upload_calls]
        paths = LinuxCorePaths()
        assert paths.launcher_script not in targets

    def test_progress_events_are_clamped_to_upper_bound(self) -> None:
        backend = _make_backend(progress_lines=[(150, "weird"), (80, "ok")])
        deployment = LinuxCoreDeployment(backend)

        result = deployment.install_napcat()

        # 150 -> 100 — 上界被钳制
        assert result.progress_events == [(100, "weird"), (80, "ok")]


class TestLogCallback:
    """P1.5: log_callback 把每行 stdout 透传给上层(用于"部署控制台")。"""

    def test_log_callback_receives_every_stdout_line(self) -> None:
        backend = _RecordingBackend(
            bash_stdout="\n".join(
                [
                    "[INFO] hello world",
                    "[PROGRESS] 50 mid",
                    "random line",
                    "[ERROR] something went wrong",
                ]
            )
        )
        deployment = LinuxCoreDeployment(backend)

        captured_lines: list[str] = []
        deployment.install_linuxqq(log_callback=captured_lines.append)

        # 所有行都应该被收到, 不只是 [PROGRESS] 行
        assert "[INFO] hello world" in captured_lines
        assert "[PROGRESS] 50 mid" in captured_lines
        assert "random line" in captured_lines
        assert "[ERROR] something went wrong" in captured_lines

    def test_log_callback_does_not_block_progress(self) -> None:
        backend = _make_backend(progress_lines=[(25, "stage one"), (75, "stage two")])
        deployment = LinuxCoreDeployment(backend)

        progress_events: list[tuple[str, int]] = []
        log_lines: list[str] = []

        result = deployment.install_napcat(
            progress=lambda m, p: progress_events.append((m, p)),
            log_callback=log_lines.append,
        )

        assert result.ok is True
        # 进度协议照常工作
        assert progress_events == [("stage one", 25), ("stage two", 75)]
        # 同时 log_callback 也收到所有行
        assert any("PROGRESS" in line for line in log_lines)

    def test_log_callback_exception_does_not_break_install(self) -> None:
        backend = _make_backend(progress_lines=[(50, "ok")])
        deployment = LinuxCoreDeployment(backend)

        def bad_callback(line: str) -> None:
            raise RuntimeError("intentional callback failure")

        # 即使 callback 抛错, 安装仍应正常完成
        result = deployment.install_linuxqq(log_callback=bad_callback)
        assert result.ok is True
        assert result.progress_events == [(50, "ok")]


class TestProgressLineParsing:
    def test_non_progress_lines_are_ignored(self) -> None:
        backend = _RecordingBackend(
            bash_stdout="\n".join(
                [
                    "[INFO] hello",
                    "[PROGRESS] 25 stage one",
                    "random output",
                    "[PROGRESS] 50 stage two",
                ]
            )
        )
        deployment = LinuxCoreDeployment(backend)

        result = deployment.install_linuxqq()
        assert result.progress_events == [(25, "stage one"), (50, "stage two")]

    def test_malformed_progress_lines_are_skipped(self) -> None:
        backend = _RecordingBackend(
            bash_stdout="\n".join(
                [
                    "[PROGRESS] notanumber message",
                    "[PROGRESS]60 noinitialspace",  # 仍由正则按 \s+ 解析失败
                    "[PROGRESS] 75 valid line",
                ]
            )
        )
        deployment = LinuxCoreDeployment(backend)

        result = deployment.install_linuxqq()
        assert result.progress_events == [(75, "valid line")]


class TestScriptTimeout:
    """回归测试: 部署脚本必须使用 SSHCredentials.script_timeout (默认 1800s),
    而不是 command_timeout (默认 20s), 否则 apt-get 等长耗时命令会被秒杀。
    """

    def test_run_script_uses_ssh_script_timeout(self) -> None:
        from src.core.remote.execution_backend import RemoteExecutionBackend
        from src.core.remote.models import RemoteCommandResult

        # 构造一个最小可用的伪 SSHClient
        class _FakeCredentials:
            command_timeout = 20.0
            script_timeout = 1800.0

        captured: dict[str, object] = {}

        class _FakeSSHClient:
            credentials = _FakeCredentials()

            def exec_stream(
                self,
                command,
                *,
                on_stdout_line=None,
                on_stdout_progress=None,
                check=False,
                merge_stderr=False,
                timeout=None,
            ):
                # 关键断言: timeout 必须是 script_timeout, 不能是 command_timeout
                captured["timeout"] = timeout
                captured["merge_stderr"] = merge_stderr
                if on_stdout_line is not None:
                    on_stdout_line("[PROGRESS] 50 halfway")
                return RemoteCommandResult(command=command, exit_status=0, stdout="", stderr="")

            def run(self, command, *, timeout=None, check=False):  # pragma: no cover
                return RemoteCommandResult(command=command, exit_status=0)

            def ensure_remote_directory(self, path):  # pragma: no cover
                return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

            def upload_file(self, local_path, target_path):  # pragma: no cover
                pass

            def download_file(self, source_path, local_path):  # pragma: no cover
                pass

        backend = RemoteExecutionBackend(_FakeSSHClient())
        deployment = LinuxCoreDeployment(backend)

        # 直接调用内部方法做最小化验证
        result, events = deployment._run_script_with_progress(  # noqa: SLF001
            "bash /tmp/some_script.sh", progress=None
        )

        assert result.exit_status == 0
        assert events == [(50, "halfway")]
        assert captured["timeout"] == 1800.0, (
            "部署脚本必须用 script_timeout(默认 1800s), 不能落到 command_timeout(默认 20s)"
        )
        assert captured["merge_stderr"] is True
