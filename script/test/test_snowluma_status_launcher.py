# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.status` 与 :mod:`...launcher` 单测 (W5).

覆盖:

- ``SnowLumaRemoteDaemonStatus.from_json``: 完整 / 缺字段 / 损坏 JSON
- ``state`` property: STOPPED / STARTING / READY 派生
- ``SnowLumaRemoteBotStatus.from_json``: qq_id 一致性 / pid 解析
- ``SnowLumaRemoteRuntimeService.get_daemon_status / get_bot_status / list_bots``:
  远端文件缺失 / 损坏 / 正常 3 种路径
- ``SnowLumaLauncherCommands``: 8 种命令格式 + qq_id/uin 校验
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import RemoteCommandResult
from src.core.remote.snowluma import (
    SnowLumaLauncherCommands,
    SnowLumaRemoteBotState,
    SnowLumaRemoteBotStatus,
    SnowLumaRemoteDaemonState,
    SnowLumaRemoteDaemonStatus,
    SnowLumaRemotePaths,
    SnowLumaRemoteRuntimeService,
)


# ==================== Fake Backend ====================
class FakeBackend(ExecutionBackend):
    """超轻量内存 backend, ``set_run_result(cmd_predicate, result)`` 控制返回值."""

    def __init__(self) -> None:
        self._matchers: list[tuple[str, RemoteCommandResult]] = []
        self.calls: list[str] = []

    def add(self, contains: str, *, stdout: str = "", exit_status: int = 0) -> None:
        self._matchers.append(
            (
                contains,
                RemoteCommandResult(
                    command=contains, exit_status=exit_status, stdout=stdout
                ),
            )
        )

    def run(
        self, command: str, *, timeout: float | None = None, check: bool = False
    ) -> RemoteCommandResult:
        self.calls.append(command)
        for contains, result in self._matchers:
            if contains in command:
                return result
        return RemoteCommandResult(command=command, exit_status=0, stdout="")

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

    def upload_file(self, local_path: str | Path, target_path: str) -> None:
        pass

    def download_file(self, source_path: str, local_path: str | Path) -> None:
        pass


@pytest.fixture
def paths() -> SnowLumaRemotePaths:
    return SnowLumaRemotePaths.from_base("/opt/sl")


@pytest.fixture
def backend() -> FakeBackend:
    return FakeBackend()


# ==================== SnowLumaRemoteDaemonStatus ====================
class TestDaemonStatusFromJson:
    def test_full_payload(self) -> None:
        payload = json.dumps(
            {
                "running": True,
                "ready": True,
                "started_at": "2026-05-11T13:30:00Z",
                "pids": {"xvfb": 1234, "fluxbox": 1235, "node": 1240},
                "ports": {"vnc": 5900, "novnc": 6081, "webui": 5099},
                "display": ":0",
            }
        )
        status = SnowLumaRemoteDaemonStatus.from_json(payload)
        assert status.running
        assert status.ready
        assert status.pids["node"] == 1240
        assert status.ports["webui"] == 5099
        assert status.display == ":0"
        assert status.state == SnowLumaRemoteDaemonState.READY

    def test_starting_state(self) -> None:
        """running=true ready=false → STARTING."""
        payload = json.dumps({"running": True, "ready": False})
        assert (
            SnowLumaRemoteDaemonStatus.from_json(payload).state
            == SnowLumaRemoteDaemonState.STARTING
        )

    def test_stopped_state(self) -> None:
        payload = json.dumps({"running": False, "ready": False})
        assert (
            SnowLumaRemoteDaemonStatus.from_json(payload).state
            == SnowLumaRemoteDaemonState.STOPPED
        )

    def test_invalid_json_raises(self) -> None:
        with pytest.raises(ValueError, match="不是合法 JSON"):
            SnowLumaRemoteDaemonStatus.from_json("not json")

    def test_non_object_raises(self) -> None:
        with pytest.raises(ValueError, match="顶层不是 object"):
            SnowLumaRemoteDaemonStatus.from_json("[]")

    def test_stopped_classmethod(self) -> None:
        s = SnowLumaRemoteDaemonStatus.stopped()
        assert s.state == SnowLumaRemoteDaemonState.STOPPED
        assert not s.running


# ==================== SnowLumaRemoteBotStatus ====================
class TestBotStatusFromJson:
    def test_full_payload(self) -> None:
        payload = json.dumps(
            {
                "qq_id": "114514",
                "uin": "2707600964",
                "pid": 9999,
                "running": True,
                "started_at": "2026-05-11T13:32:00Z",
            }
        )
        bot = SnowLumaRemoteBotStatus.from_json("114514", payload)
        assert bot.qq_id == "114514"
        assert bot.uin == "2707600964"
        assert bot.pid == 9999
        assert bot.running
        assert bot.state == SnowLumaRemoteBotState.RUNNING

    def test_pid_missing_running_true_is_crashed(self) -> None:
        payload = json.dumps(
            {"qq_id": "114514", "uin": None, "pid": None, "running": True}
        )
        bot = SnowLumaRemoteBotStatus.from_json("114514", payload)
        assert bot.state == SnowLumaRemoteBotState.CRASHED

    def test_qq_id_mismatch_raises(self) -> None:
        payload = json.dumps({"qq_id": "999", "running": False})
        with pytest.raises(ValueError, match="qq_id 字段不一致"):
            SnowLumaRemoteBotStatus.from_json("114514", payload)


# ==================== SnowLumaRemoteRuntimeService ====================
class TestRuntimeServiceDaemon:
    def test_returns_stopped_when_file_missing(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        # backend 默认返空 stdout
        service = SnowLumaRemoteRuntimeService(backend, paths)
        status = service.get_daemon_status()
        assert status.state == SnowLumaRemoteDaemonState.STOPPED

    def test_returns_ready_on_full_payload(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        payload = json.dumps(
            {"running": True, "ready": True, "started_at": "2026-05-11T13:30:00Z"}
        )
        backend.add(f'cat "{paths.status_daemon}"', stdout=payload)
        service = SnowLumaRemoteRuntimeService(backend, paths)
        assert service.get_daemon_status().state == SnowLumaRemoteDaemonState.READY

    def test_corrupted_json_returns_stopped(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.add(f'cat "{paths.status_daemon}"', stdout="not json {{{{")
        service = SnowLumaRemoteRuntimeService(backend, paths)
        assert service.get_daemon_status().state == SnowLumaRemoteDaemonState.STOPPED

    def test_tail_daemon_log_returns_stdout(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        backend.add(f'tail -n 200 "{paths.log_daemon}"', stdout="line1\nline2\n")
        service = SnowLumaRemoteRuntimeService(backend, paths)
        assert service.tail_daemon_log() == "line1\nline2\n"

    def test_tail_daemon_log_rejects_zero_lines(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        service = SnowLumaRemoteRuntimeService(backend, paths)
        with pytest.raises(ValueError, match="lines 必须 > 0"):
            service.tail_daemon_log(0)


class TestRuntimeServiceBot:
    def test_get_bot_returns_stopped_when_missing(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        service = SnowLumaRemoteRuntimeService(backend, paths)
        bot = service.get_bot_status("114514")
        assert bot.state == SnowLumaRemoteBotState.STOPPED
        assert bot.qq_id == "114514"

    def test_get_bot_parses_full(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        payload = json.dumps(
            {"qq_id": "114514", "uin": "2707600964", "pid": 9999, "running": True}
        )
        backend.add(f'cat "{paths.status_bot("114514")}"', stdout=payload)
        service = SnowLumaRemoteRuntimeService(backend, paths)
        bot = service.get_bot_status("114514")
        assert bot.state == SnowLumaRemoteBotState.RUNNING
        assert bot.uin == "2707600964"

    def test_get_bot_invalid_qq_id_raises(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        service = SnowLumaRemoteRuntimeService(backend, paths)
        with pytest.raises(ValueError):
            service.get_bot_status("abc")
        with pytest.raises(ValueError):
            service.get_bot_status("")

    def test_list_bots_empty(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        service = SnowLumaRemoteRuntimeService(backend, paths)
        assert service.list_bots() == []

    def test_list_bots_parses_multiple(
        self, backend: FakeBackend, paths: SnowLumaRemotePaths
    ) -> None:
        ls_output = (
            f"{paths.runtime_dir}/status_bot_999.json\n"
            f"{paths.runtime_dir}/status_bot_114514.json\n"
        )
        backend.add("ls -1", stdout=ls_output)
        backend.add(
            f'cat "{paths.status_bot("114514")}"',
            stdout=json.dumps(
                {"qq_id": "114514", "uin": "2707600964", "pid": 1, "running": True}
            ),
        )
        backend.add(
            f'cat "{paths.status_bot("999")}"',
            stdout=json.dumps(
                {"qq_id": "999", "uin": None, "pid": None, "running": False}
            ),
        )

        service = SnowLumaRemoteRuntimeService(backend, paths)
        bots = service.list_bots()
        assert [b.qq_id for b in bots] == ["999", "114514"]
        assert bots[0].state == SnowLumaRemoteBotState.STOPPED
        assert bots[1].state == SnowLumaRemoteBotState.RUNNING


# ==================== SnowLumaLauncherCommands ====================
class TestLauncherCommands:
    @pytest.fixture
    def cmds(self, paths: SnowLumaRemotePaths) -> SnowLumaLauncherCommands:
        return SnowLumaLauncherCommands(paths)

    def test_daemon_start(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        expected = f'bash "{paths.daemon_launcher_script}" start'
        assert cmds.daemon_start_cmd() == expected

    def test_daemon_stop(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        assert cmds.daemon_stop_cmd().endswith(' stop')

    def test_daemon_status(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        assert cmds.daemon_status_cmd().endswith(' status')

    def test_daemon_restart(self, cmds: SnowLumaLauncherCommands) -> None:
        assert cmds.daemon_restart_cmd().endswith(' restart')

    def test_daemon_wait_ready_default(self, cmds: SnowLumaLauncherCommands) -> None:
        assert cmds.daemon_wait_ready_cmd().endswith(' wait-ready 60')

    def test_daemon_wait_ready_custom_timeout(
        self, cmds: SnowLumaLauncherCommands
    ) -> None:
        assert cmds.daemon_wait_ready_cmd(120).endswith(' wait-ready 120')

    def test_daemon_wait_ready_invalid_timeout(
        self, cmds: SnowLumaLauncherCommands
    ) -> None:
        with pytest.raises(ValueError, match="正整数"):
            cmds.daemon_wait_ready_cmd(0)
        with pytest.raises(ValueError, match="正整数"):
            cmds.daemon_wait_ready_cmd(-1)

    def test_bot_start_without_uin(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        cmd = cmds.bot_start_cmd("114514")
        assert cmd == f'bash "{paths.bot_launcher_script}" start 114514'

    def test_bot_start_with_uin(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        cmd = cmds.bot_start_cmd("114514", uin="2707600964")
        assert cmd == f'bash "{paths.bot_launcher_script}" start 114514 2707600964'

    def test_bot_stop(self, cmds: SnowLumaLauncherCommands) -> None:
        assert cmds.bot_stop_cmd("114514").endswith(' stop 114514')

    def test_bot_status(self, cmds: SnowLumaLauncherCommands) -> None:
        assert cmds.bot_status_cmd("114514").endswith(' status 114514')

    @pytest.mark.parametrize("bad_qq_id", ["", "abc", "12 34", "1;2", "$(rm)"])
    def test_bot_start_rejects_invalid_qq_id(
        self, cmds: SnowLumaLauncherCommands, bad_qq_id: str
    ) -> None:
        with pytest.raises(ValueError, match="qq_id"):
            cmds.bot_start_cmd(bad_qq_id)

    @pytest.mark.parametrize("bad_uin", ["", "abc", "12 34"])
    def test_bot_start_rejects_invalid_uin(
        self, cmds: SnowLumaLauncherCommands, bad_uin: str
    ) -> None:
        with pytest.raises(ValueError, match="uin"):
            cmds.bot_start_cmd("114514", uin=bad_uin)

    def test_frozen_dataclass(
        self, cmds: SnowLumaLauncherCommands, paths: SnowLumaRemotePaths
    ) -> None:
        """``frozen=True`` 保证不变 (避免 UI 误改导致命令构造漂移)."""
        with pytest.raises((AttributeError, Exception)):
            cmds.paths = paths  # type: ignore[misc]
