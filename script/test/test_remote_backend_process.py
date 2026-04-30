# -*- coding: utf-8 -*-
"""[`RemoteBackend.start_napcat`](src/core/operation/remote_backend.py) /
[`stop_napcat`](src/core/operation/remote_backend.py) /
[`get_process_status`](src/core/operation/remote_backend.py) 单元测试 (P2.3).

测试通过把 ``ssh_client`` / ``_exec_backend`` / ``_runtime`` 三件套替换为 fake,
完全绕过真实 SSH 通讯, 仅验证:

- launcher 命令拼装是否符合 ``bash $launcher start <qq_id>`` 形式
- 是否在调用前确认 launcher 文件存在
- qq_id 注入防御 (非数字 / 长度异常)
- 启动成功后 ProcessStatus 字段正确填充
- 启动失败 / 状态查询失败时的错误传播
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest

from src.core.operation.remote_backend import RemoteBackend
from src.core.remote.errors import RemoteCommandError
from src.core.remote.models import LinuxCorePaths, RemoteCommandResult, SSHCredentials
from src.core.remote.status import RemoteNapCatStatus


# ==================== Fakes ====================
@dataclass
class _FakeSSHClient:
    """最小可用 SSH 客户端替身, 仅暴露 ``RemoteBackend`` 实际调用到的方法."""

    is_connected: bool = True
    connect_calls: int = 0
    remote_exists_paths: list[str] = field(default_factory=list)
    remote_exists_result: bool = True
    write_text_calls: list[tuple[str, str]] = field(default_factory=list)

    def connect(self) -> None:
        self.connect_calls += 1
        self.is_connected = True

    def close(self) -> None:
        self.is_connected = False

    def remote_exists(self, path: str) -> bool:
        self.remote_exists_paths.append(path)
        return self.remote_exists_result

    def write_text(self, path: str, content: str) -> None:
        self.write_text_calls.append((path, content))

    @staticmethod
    def _quote_remote_argument(value: str) -> str:
        # 与真实 SSHClient._quote_remote_argument 行为对齐: 含 $HOME 时双引号, 否则 shlex.quote
        import shlex
        if value.startswith("$HOME"):
            return f'"{value}"'
        return shlex.quote(value)


@dataclass
class _FakeExecBackend:
    """RemoteExecutionBackend 替身, 按 (command_prefix, result) 顺序返回伪结果."""

    history: list[tuple[str, dict[str, Any]]] = field(default_factory=list)
    canned: list[RemoteCommandResult] = field(default_factory=list)
    raise_on_check: bool = False

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        self.history.append((command, {"timeout": timeout, "check": check}))
        result = self.canned.pop(0) if self.canned else RemoteCommandResult(command=command, exit_status=0)
        if check and not result.ok:
            raise RemoteCommandError(command=command, exit_status=result.exit_status, stderr=result.stderr)
        return result


@dataclass
class _FakeRuntime:
    """RemoteRuntimeService 替身, 仅 mock ``get_status_for_bot``."""

    status_responses: dict[str, RemoteNapCatStatus] = field(default_factory=dict)
    queried_qq_ids: list[str] = field(default_factory=list)

    def get_status_for_bot(self, qq_id: str) -> RemoteNapCatStatus:
        self.queried_qq_ids.append(qq_id)
        return self.status_responses.get(
            qq_id,
            RemoteNapCatStatus(running=False, pid=None, qq=None, version=None, log_file=None),
        )


# ==================== Fixtures ====================
@pytest.fixture
def credentials() -> SSHCredentials:
    return SSHCredentials(host="192.0.2.10", username="napcat", auth_method="password", password="x")


@pytest.fixture
def backend(credentials: SSHCredentials) -> RemoteBackend:
    """构造 RemoteBackend 后立即把内部依赖替换为 fake; 不触发任何真实 SSH."""
    rb = RemoteBackend(credentials, LinuxCorePaths())
    rb.ssh_client = _FakeSSHClient()  # type: ignore[assignment]
    rb._exec_backend = _FakeExecBackend()  # type: ignore[attr-defined]
    rb._runtime = _FakeRuntime()  # type: ignore[attr-defined]
    return rb


# ==================== qq_id 注入防御 ====================
class TestQQIdGuard:
    @pytest.mark.parametrize("invalid", ["", "abc", "12; rm -rf /", "12", "1234567890123"])
    def test_invalid_qq_id_raises_value_error_before_ssh(self, backend: RemoteBackend, invalid: str, config_factory) -> None:
        with pytest.raises(ValueError, match="非法 qq_id"):
            backend.start_napcat(invalid, config_factory())
        # 不应触发任何 launcher 调用
        assert backend._exec_backend.history == []  # type: ignore[attr-defined]


# ==================== 启动 ====================
class TestStartNapcat:
    def test_start_runs_launcher_with_qq_id(self, backend: RemoteBackend, config_factory) -> None:
        backend._runtime.status_responses["114514"] = RemoteNapCatStatus(  # type: ignore[attr-defined]
            running=True, pid=4321, qq="114514", version="4.18.1", log_file="/x.log"
        )
        result = backend.start_napcat("114514", config_factory(qqid=114514))

        history = backend._exec_backend.history  # type: ignore[attr-defined]
        # 第一条命令必须是 launcher start; 后续可能含 _fetch_rss_bytes 的 ps 命令
        assert len(history) >= 1
        command, kwargs = history[0]
        assert command.startswith("bash ")
        assert command.endswith(" start 114514")
        # launcher 默认 60s 超时
        assert kwargs == {"timeout": 60.0, "check": True}

        assert result.qq_id == "114514"
        assert result.running is True
        assert result.pid == 4321
        assert result.extra["version"] == "4.18.1"
        assert result.extra["log_file"] == "/x.log"

    def test_start_verifies_launcher_present(self, backend: RemoteBackend, config_factory) -> None:
        backend.ssh_client.remote_exists_result = False  # type: ignore[attr-defined]
        with pytest.raises(FileNotFoundError, match="远端 launcher 脚本缺失"):
            backend.start_napcat("114514", config_factory())
        assert backend._exec_backend.history == []  # type: ignore[attr-defined]

    def test_start_raises_when_launcher_returns_nonzero(self, backend: RemoteBackend, config_factory) -> None:
        # 让 launcher 返回非零退出码 -> RemoteCommandError
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(command="bash launcher", exit_status=4, stderr="failed")
        )
        with pytest.raises(RemoteCommandError) as exc_info:
            backend.start_napcat("114514", config_factory())
        assert exc_info.value.exit_status == 4

    def test_start_raises_when_launcher_ok_but_not_running(self, backend: RemoteBackend, config_factory) -> None:
        # launcher 返回 0 但状态查询说没跑 -> 视为竞态错误
        # status_responses 不设置 -> _FakeRuntime 默认返回 running=False
        with pytest.raises(RemoteCommandError) as exc_info:
            backend.start_napcat("114514", config_factory())
        assert "launcher reported success but" in (exc_info.value.stderr or "")


# ==================== 停止 ====================
class TestStopNapcat:
    def test_stop_runs_launcher_with_qq_id(self, backend: RemoteBackend) -> None:
        backend.stop_napcat("114514")

        history = backend._exec_backend.history  # type: ignore[attr-defined]
        assert len(history) == 1
        command, kwargs = history[0]
        assert command.endswith(" stop 114514")
        assert kwargs == {"timeout": 30.0, "check": True}

    def test_stop_verifies_launcher_present(self, backend: RemoteBackend) -> None:
        backend.ssh_client.remote_exists_result = False  # type: ignore[attr-defined]
        with pytest.raises(FileNotFoundError):
            backend.stop_napcat("114514")

    def test_stop_invalid_qq_id_raises(self, backend: RemoteBackend) -> None:
        with pytest.raises(ValueError):
            backend.stop_napcat("abc")


# ==================== 状态查询 ====================
class TestGetProcessStatus:
    def test_get_process_status_dispatches_to_runtime_for_bot(self, backend: RemoteBackend) -> None:
        backend._runtime.status_responses["114514"] = RemoteNapCatStatus(  # type: ignore[attr-defined]
            running=True, pid=99, qq="114514", version="4.18.1", log_file="/y.log"
        )
        status = backend.get_process_status("114514")

        assert backend._runtime.queried_qq_ids == ["114514"]  # type: ignore[attr-defined]
        assert status.running is True
        assert status.pid == 99
        assert status.qq_id == "114514"

    def test_get_process_status_returns_offline_when_not_running(self, backend: RemoteBackend) -> None:
        status = backend.get_process_status("114514")
        assert status.running is False
        assert status.pid is None


# ==================== 配置同步 (P2.4) ====================
class TestRuntimeConfigSync:
    def test_write_bot_runtime_config_uploads_two_json_files(
        self, backend: RemoteBackend, config_factory
    ) -> None:
        config = config_factory(qqid=1145141919)
        onebot_path, napcat_path = backend.write_bot_runtime_config(config)

        write_calls = backend.ssh_client.write_text_calls  # type: ignore[attr-defined]
        # 两次 SFTP 写入
        assert len(write_calls) == 2

        paths = [call[0] for call in write_calls]
        # 路径必须落在 paths.config_dir 下
        config_dir = backend.paths.config_dir
        assert paths[0] == f"{config_dir}/onebot11_1145141919.json"
        assert paths[1] == f"{config_dir}/napcat_1145141919.json"
        assert (onebot_path, napcat_path) == tuple(paths)

        # 内容必须能反序列化为 JSON
        import json as _json
        onebot_payload = _json.loads(write_calls[0][1])
        napcat_payload = _json.loads(write_calls[1][1])
        assert "network" in onebot_payload
        assert "fileLog" in napcat_payload

    def test_write_bot_runtime_config_validates_qq_id(
        self, backend: RemoteBackend, config_factory
    ) -> None:
        config = config_factory(qqid=12)  # 仅 2 位, 触发 _shell_quote_qq 校验
        with pytest.raises(ValueError, match="非法 qq_id"):
            backend.write_bot_runtime_config(config)
        assert backend.ssh_client.write_text_calls == []  # type: ignore[attr-defined]

    def test_delete_bot_runtime_config_runs_rm_for_each_file(
        self, backend: RemoteBackend
    ) -> None:
        backend.delete_bot_runtime_config("1145141919")
        history = backend._exec_backend.history  # type: ignore[attr-defined]
        # 两次 rm -f
        assert len(history) == 2
        for command, kwargs in history:
            assert command.startswith("rm -f --")
            assert kwargs["check"] is False
        assert "onebot11_1145141919.json" in history[0][0]
        assert "napcat_1145141919.json" in history[1][0]

    def test_start_napcat_invokes_config_sync_before_launcher(
        self, backend: RemoteBackend, config_factory
    ) -> None:
        backend._runtime.status_responses["1145141919"] = RemoteNapCatStatus(  # type: ignore[attr-defined]
            running=True, pid=1, qq="1145141919", version="4.18.1", log_file="/x"
        )
        backend.start_napcat("1145141919", config_factory(qqid=1145141919))
        # 配置同步在 start_napcat 内部走 ssh_client.write_text 路径
        assert len(backend.ssh_client.write_text_calls) == 2  # type: ignore[attr-defined]


# ==================== WebUI 隧道 (P2.5) ====================
@dataclass
class _FakeForwarder:
    """LocalPortForwarder 替身, 仅记录生命周期与端口."""

    remote_port: int
    local_port: int = 50001
    stopped: bool = False

    def stop(self) -> None:
        self.stopped = True


class TestWebUIEndpoint:
    def test_returns_none_when_log_missing(self, backend: RemoteBackend) -> None:
        # _exec_backend 默认返回 exit_status=0 + stdout="" -> 无匹配
        assert backend.get_webui_endpoint("114514") is None

    def test_extracts_port_token_and_opens_tunnel(self, backend: RemoteBackend) -> None:
        # 模拟 grep 输出
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=abc123\n",
            )
        )
        # 替换 SSHClient.open_local_tunnel 为 fake
        opened: list[int] = []

        def _open_tunnel(remote_port: int, *, remote_host: str, label: str):
            opened.append(remote_port)
            return _FakeForwarder(remote_port=remote_port, local_port=51234)

        backend.ssh_client.open_local_tunnel = _open_tunnel  # type: ignore[attr-defined]

        endpoint = backend.get_webui_endpoint("114514")

        assert endpoint is not None
        assert endpoint.token == "abc123"
        assert endpoint.base_url == "http://127.0.0.1:51234"
        assert opened == [6099]
        # 缓存生效
        assert "114514" in backend._webui_tunnels  # type: ignore[attr-defined]

    def test_reuses_tunnel_for_same_remote_port(self, backend: RemoteBackend) -> None:
        # 第一次 grep
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t1",
            )
        )
        # 第二次 grep (相同端口)
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t1",
            )
        )
        opens = 0

        def _open_tunnel(remote_port: int, *, remote_host: str, label: str):
            nonlocal opens
            opens += 1
            return _FakeForwarder(remote_port=remote_port)

        backend.ssh_client.open_local_tunnel = _open_tunnel  # type: ignore[attr-defined]

        backend.get_webui_endpoint("114514")
        backend.get_webui_endpoint("114514")
        assert opens == 1  # 第二次应复用

    def test_recreates_tunnel_when_remote_port_changes(self, backend: RemoteBackend) -> None:
        # 第一次端口 6099
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t1",
            )
        )
        # 第二次端口 6100
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi User Panel Url: http://127.0.0.1:6100/webui?token=t2",
            )
        )
        forwarders: list[_FakeForwarder] = []

        def _open_tunnel(remote_port: int, *, remote_host: str, label: str):
            f = _FakeForwarder(remote_port=remote_port)
            forwarders.append(f)
            return f

        backend.ssh_client.open_local_tunnel = _open_tunnel  # type: ignore[attr-defined]

        backend.get_webui_endpoint("114514")
        backend.get_webui_endpoint("114514")
        assert len(forwarders) == 2
        assert forwarders[0].stopped is True  # 旧隧道被关闭
        assert forwarders[1].stopped is False  # 新隧道正在工作

    def test_close_webui_tunnel_stops_forwarder(self, backend: RemoteBackend) -> None:
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t1",
            )
        )
        forwarder = _FakeForwarder(remote_port=6099)
        backend.ssh_client.open_local_tunnel = lambda *a, **k: forwarder  # type: ignore[attr-defined]
        backend.get_webui_endpoint("114514")

        backend.close_webui_tunnel("114514")
        assert forwarder.stopped is True
        assert "114514" not in backend._webui_tunnels  # type: ignore[attr-defined]

    def test_close_backend_stops_all_tunnels(self, backend: RemoteBackend) -> None:
        f1 = _FakeForwarder(remote_port=6099)
        f2 = _FakeForwarder(remote_port=6100)
        backend._webui_tunnels["a"] = f1  # type: ignore[arg-type,attr-defined]
        backend._webui_tunnels["b"] = f2  # type: ignore[arg-type,attr-defined]
        backend.close()
        assert f1.stopped and f2.stopped
        assert backend._webui_tunnels == {}  # type: ignore[attr-defined]

    @pytest.mark.parametrize(
        "stdout_line",
        [
            # 现代 NapCat: WebUi Local Panel Url
            "[info] [NapCat] [WebUi] WebUi Local Panel Url: http://127.0.0.1:6099/webui?token=abc123\n",
            # 0.0.0.0 监听 (NapCat WebUI 默认监听全网卡时常见)
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://0.0.0.0:6099/webui?token=abc123\n",
            # 公网 IP host
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://1.2.3.4:6099/webui?token=abc123\n",
            # IPv6 dual-stack ``[::]`` (Linux 默认 bindv6only=0 时实测 NapCat 输出)
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://[::]:6099/webui?token=abc123\n",
            # IPv6 loopback ``[::1]``
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://[::1]:6099/webui?token=abc123\n",
            # 完整 IPv6 公网地址
            "[info] [NapCat] [WebUi] WebUi User Panel Url: http://[2001:db8::1]:6099/webui?token=abc123\n",
            # 通用 AccessUrl 标签 (新版本可能用)
            "[info] [NapCat] [WebUi] AccessUrl: http://127.0.0.1:6099/webui?token=abc123\n",
            # 末尾带额外标点 / 空白, 不应吃进 token
            "WebUi User Panel Url: http://127.0.0.1:6099/webui?token=abc123 (copy this)\n",
            # 带 ANSI 颜色码的真实日志行 (NapCat 终端着色), 颜色码不应干扰 URL 抽取
            "22:54:12 [\x1b[32minfo\x1b[39m] [NapCat] [WebUi] WebUi User Panel Url: http://[::]:6099/webui?token=abc123\n",
        ],
    )
    def test_extracts_port_token_from_various_log_formats(
        self, backend: RemoteBackend, stdout_line: str
    ) -> None:
        """非 ``127.0.0.1`` host / 不同标签 / 行尾噪音都应能提取出 port+token."""
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(command="grep", exit_status=0, stdout=stdout_line)
        )
        forwarder = _FakeForwarder(remote_port=6099, local_port=51234)
        backend.ssh_client.open_local_tunnel = lambda *a, **k: forwarder  # type: ignore[attr-defined]

        endpoint = backend.get_webui_endpoint("114514")

        assert endpoint is not None
        assert endpoint.token == "abc123"
        assert endpoint.base_url == "http://127.0.0.1:51234"

    def test_grep_command_uses_token_substring_filter(self, backend: RemoteBackend) -> None:
        """grep 必须按 ``/webui?token=`` 子串过滤而非 ``WebUi User Panel Url`` 字面量,
        否则新版本 NapCat (用 ``Local Panel Url`` 或 ``AccessUrl``) 会全部漏掉.
        """
        backend._exec_backend.canned.append(  # type: ignore[attr-defined]
            RemoteCommandResult(
                command="grep",
                exit_status=0,
                stdout="WebUi Local Panel Url: http://127.0.0.1:6099/webui?token=t1\n",
            )
        )
        backend.ssh_client.open_local_tunnel = lambda *a, **k: _FakeForwarder(remote_port=6099)  # type: ignore[attr-defined]
        backend.get_webui_endpoint("114514")

        # 检查最近一次 grep 命令字面量
        history = backend._exec_backend.history  # type: ignore[attr-defined]
        grep_cmds = [cmd for cmd, _ in history if "grep" in cmd]
        assert grep_cmds, "应当至少触发一次 grep"
        assert "/webui?token=" in grep_cmds[-1]
        assert "WebUi User Panel Url" not in grep_cmds[-1]
