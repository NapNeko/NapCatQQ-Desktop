# -*- coding: utf-8 -*-

# 标准库导入
from pathlib import Path
from types import SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtCore import QProcess

# 项目内模块导入
import src.core.runtime.napcat as run_napcat


class FakeTimer:
    """最小可用的 QTimer 替身。"""

    single_shots: list[tuple[int, object]] = []

    def __init__(self, parent=None) -> None:
        self.parent = parent
        self.interval = None
        self.timeout = SimpleNamespace(
            connect=lambda callback: setattr(self, "_callback", callback),
            disconnect=lambda: setattr(self, "_disconnected", True),
        )

    def start(self, interval=None) -> None:
        if interval is not None:
            self.interval = interval

    def setInterval(self, interval: int) -> None:
        self.interval = interval

    def stop(self) -> None:
        self.stopped = True

    def deleteLater(self) -> None:
        self.deleted = True

    @staticmethod
    def singleShot(interval: int, callback) -> None:
        FakeTimer.single_shots.append((interval, callback))


class FakeProcess:
    """用于测试 ManagerNapCatQQProcess 的假进程。"""

    def __init__(self, *, started: bool = True) -> None:
        self.started = started
        self.start_called = False
        self.deleted = False

    def start(self) -> None:
        self.start_called = True

    def waitForStarted(self, timeout: int) -> bool:
        self.timeout = timeout
        return self.started

    def errorString(self) -> str:
        return "process failed"

    def state(self):
        return QProcess.ProcessState.Running

    def deleteLater(self) -> None:
        self.deleted = True


class FakeManagedProcess(FakeProcess):
    """支持 stop/get_memory_usage 分支的进程替身。"""

    def __init__(self, *, pid: int = 1, started: bool = True, state=QProcess.ProcessState.Running) -> None:
        super().__init__(started=started)
        self._pid = pid
        self._state = state
        self.killed = False
        self.waited = False

    def processId(self) -> int:
        return self._pid

    def kill(self) -> None:
        self.killed = True

    def waitForFinished(self) -> None:
        self.waited = True

    def state(self):
        return self._state


@pytest.fixture
def mute_run_napcat_logger(monkeypatch: pytest.MonkeyPatch) -> None:
    """屏蔽 run_napcat 模块的真实日志副作用。"""
    monkeypatch.setattr(run_napcat.logger, "trace", lambda *args, **kwargs: None)
    monkeypatch.setattr(run_napcat.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(run_napcat.logger, "warning", lambda *args, **kwargs: None)
    monkeypatch.setattr(run_napcat.logger, "error", lambda *args, **kwargs: None)


def test_process_log_extracts_webui_port_and_token(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """日志解析应把 WebUI 端口和 token 转交给登录状态管理器。"""
    captured: dict[str, object] = {}

    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: SimpleNamespace(
            create_login_state=lambda **kwargs: captured.update(kwargs),
        ),
    )

    process = QProcess()
    log_buffer = run_napcat.NapCatQQProcessLog(config_factory(223344), process)

    log_buffer.slot_get_web_ui_port(
        "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:3456/webui?token=test-token"
    )

    assert captured["config"].bot.QQID == 223344
    assert captured["port"] == 3456
    assert captured["token"] == "test-token"


def test_login_state_offline_autorestart_sends_notifications_and_restarts_once(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """从在线转为离线且开启自动重启时，应先通知再重启，并避免重复触发。"""
    notifications: list[tuple[object, str]] = []
    restart_requests: list[int] = []

    monkeypatch.setattr(run_napcat, "QTimer", FakeTimer)
    monkeypatch.setattr(
        run_napcat,
        "cfg",
        SimpleNamespace(
            bot_offline_web_hook_notice="webhook",
            bot_offline_email_notice="email",
            get=lambda item: True,
        ),
    )
    monkeypatch.setattr(run_napcat, "create_offline_webhook_task", lambda config: ("webhook-task", config.bot.QQID))
    monkeypatch.setattr(run_napcat, "create_offline_email_task", lambda config: ("email-task", config.bot.QQID))
    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: SimpleNamespace(restart_process=lambda config: restart_requests.append(config.bot.QQID)),
    )

    config = config_factory(667788)
    config.bot.offlineAutoRestart = True
    config.advanced.offlineNotice = True

    login_state = run_napcat.NapCatQQLoginState(config=config, port=8080, token="token")
    monkeypatch.setattr(
        login_state,
        "_start_notification_task",
        lambda task, message: notifications.append((task, message)),
    )
    login_state._is_logged_in = True
    login_state._online_status = True

    login_state.slot_update_online_status(False)
    login_state.slot_update_online_status(False)

    assert notifications == [
        (("webhook-task", 667788), "已发送离线通知到配置的 WebHook 地址"),
        (("email-task", 667788), "已发送离线通知到配置的邮箱地址"),
    ]
    assert restart_requests == [667788]
    assert login_state._offline_notice is True


def test_get_auth_status_runnable_emits_credential(monkeypatch: pytest.MonkeyPatch) -> None:
    """认证任务在接口成功时应发出 Credential。"""
    monkeypatch.setattr(
        run_napcat,
        "post",
        lambda *args, **kwargs: SimpleNamespace(status_code=200, json=lambda: {"data": {"Credential": "auth-token"}}),
    )
    credentials: list[str] = []

    runner = run_napcat.GetAuthStatusRunnable(port=3000, token="token")
    runner.login_auth_signal.connect(credentials.append)
    runner.run()

    assert credentials == ["auth-token"]


def test_get_login_status_runnable_emits_login_qrcode_and_online(monkeypatch: pytest.MonkeyPatch) -> None:
    """登录状态任务应分别发出登录态、二维码和在线状态。"""
    responses = {
        "/api/QQLogin/CheckLoginStatus": SimpleNamespace(
            status_code=200,
            json=lambda: {"data": {"isLogin": False, "qrcodeurl": "https://example.com/qr"}},
        ),
        "/api/QQLogin/GetQQLoginInfo": SimpleNamespace(
            status_code=200,
            json=lambda: {"data": {"online": True}},
        ),
    }
    login_states: list[bool] = []
    qrcodes: list[str] = []
    online_states: list[bool] = []

    runner = run_napcat.GetLoginStatusRunnable(port=3000, token="token", auth="auth-token")
    runner.client = SimpleNamespace(post=lambda path: responses[path])
    runner.login_status_signal.connect(login_states.append)
    runner.login_qrcode_signal.connect(qrcodes.append)
    runner.online_status_signal.connect(online_states.append)

    runner.get_login_status()
    runner.get_online_status()

    assert login_states == [False]
    assert qrcodes == ["https://example.com/qr"]
    assert online_states == [True]


def test_login_state_remove_stops_timers_and_emits_removed(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """移除登录状态时应停止两个定时器并发出移除信号。"""
    monkeypatch.setattr(run_napcat, "QTimer", FakeTimer)
    removed: list[str] = []

    login_state = run_napcat.NapCatQQLoginState(config=config_factory(778899), port=8080, token="token")
    login_state.qr_code_removed_signal.connect(removed.append)

    login_state.remove()

    assert removed == ["778899"]
    assert getattr(login_state._auth_timer, "stopped", False) is True
    assert getattr(login_state._login_state_timer, "stopped", False) is True


def test_login_state_emits_qrcode_removed_when_login_succeeds(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """更新为已登录时应移除二维码提示。"""
    monkeypatch.setattr(run_napcat, "QTimer", FakeTimer)
    removed: list[str] = []

    login_state = run_napcat.NapCatQQLoginState(config=config_factory(998877), port=8080, token="token")
    login_state.qr_code_removed_signal.connect(removed.append)

    login_state.slot_update_login_state(True)

    assert login_state.get_login_state() is True
    assert removed == ["998877"]


def test_create_napcat_process_emits_error_when_qq_path_missing(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """缺少 QQ 安装路径时应拒绝启动并提示错误。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    emitted: list[tuple[str, str]] = []
    manager.notification_signal.connect(lambda level, message: emitted.append((level, message)))

    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: SimpleNamespace(get_qq_path=lambda: None),
    )

    manager.create_napcat_process(config_factory(112233))

    assert emitted == [("error", "未检测到 QQ 安装路径，无法启动 NapCatQQ 进程!")]
    assert manager.napcat_process_dict == {}


def test_create_napcat_process_rejects_more_than_four_instances(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """已有 4 个进程时应拒绝继续创建。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    manager.napcat_process_dict = {str(i): object() for i in range(4)}
    emitted: list[tuple[str, str]] = []
    manager.notification_signal.connect(lambda level, message: emitted.append((level, message)))

    manager.create_napcat_process(config_factory(123456))

    assert emitted == [("error", "NapCatQQ 进程数量已达上限，无法创建新进程!")]


def test_create_napcat_process_emits_error_when_start_fails(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """启动失败时应删除进程对象并发出错误提示。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    emitted: list[tuple[str, str]] = []
    manager.notification_signal.connect(lambda level, message: emitted.append((level, message)))

    process = FakeProcess(started=False)
    monkeypatch.setattr(manager, "_create_napcat_process", lambda config, qq_path: process)
    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: {
            "PathFunc": SimpleNamespace(get_qq_path=lambda: Path("C:/Program Files/Tencent/QQ")),
            "ManagerNapCatQQLog": SimpleNamespace(create_log=lambda config, proc: None),
        }[cls.__name__],
    )

    manager.create_napcat_process(config_factory(654321))

    assert emitted == [("error", "NapCatQQ 进程启动失败!")]
    assert process.deleted is True
    assert manager.napcat_process_dict == {}


def test_create_napcat_process_starts_process_and_registers_state(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """启动成功后应登记进程、创建日志，并启动自动重启计时器。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    created_logs: list[int] = []
    auto_restart: list[int] = []
    state_changes: list[tuple[str, QProcess.ProcessState]] = []
    manager.process_changed_signal.connect(lambda qq_id, state: state_changes.append((qq_id, state)))

    process = FakeProcess(started=True)

    monkeypatch.setattr(manager, "_create_napcat_process", lambda config, qq_path: process)
    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: {
            "PathFunc": SimpleNamespace(get_qq_path=lambda: Path("C:/Program Files/Tencent/QQ")),
            "ManagerNapCatQQLog": SimpleNamespace(create_log=lambda config, proc: created_logs.append(config.bot.QQID)),
            "ManagerAutoRestartProcess": SimpleNamespace(
                create_auto_restart_timer=lambda config: auto_restart.append(config.bot.QQID)
            ),
        }[cls.__name__],
    )

    manager.create_napcat_process(config_factory(445566))

    stored = manager.napcat_process_dict["445566"]
    assert process.start_called is True
    assert created_logs == [445566]
    assert auto_restart == [445566]
    assert stored.qq_id == "445566"
    assert stored.process is process
    assert stored.state == QProcess.ProcessState.Running
    assert stored.started_at > 0
    assert state_changes == [("445566", QProcess.ProcessState.Running)]


def test_auto_restart_timer_uses_schedule_duration(
    monkeypatch: pytest.MonkeyPatch, config_factory, mute_run_napcat_logger
) -> None:
    """自动重启定时器应按配置换算毫秒间隔。"""
    monkeypatch.setattr(run_napcat, "QTimer", FakeTimer)
    restart_requests: list[int] = []
    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: SimpleNamespace(restart_process=lambda config: restart_requests.append(config.bot.QQID)),
    )
    manager = run_napcat.ManagerAutoRestartProcess()
    config = config_factory(121212)

    manager.create_auto_restart_timer(config)

    timer = manager.auto_restart_process_dict["121212"]
    assert timer.interval == 2 * 3600 * 1000

    timer._callback()
    assert restart_requests == [121212]


def test_stop_process_removes_process_and_login_state(
    monkeypatch: pytest.MonkeyPatch, mute_run_napcat_logger
) -> None:
    """停止进程时应终止子进程树、移除字典并通知登录状态管理器。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    state_changes: list[tuple[str, QProcess.ProcessState]] = []
    manager.process_changed_signal.connect(lambda qq_id, state: state_changes.append((qq_id, state)))
    process = FakeManagedProcess(pid=99)
    manager.napcat_process_dict["334455"] = run_napcat.NapCatProcessModel(
        qq_id="334455",
        process=process,
        state=QProcess.ProcessState.Running,
        started_at=1.0,
    )
    removed_login_states: list[str] = []

    class FakeChild:
        def __init__(self) -> None:
            self.killed = False

        def kill(self) -> None:
            self.killed = True

    class FakeParent:
        pid = 99

        def __init__(self) -> None:
            self.children_list = [FakeChild(), FakeChild()]
            self.killed = False

        def children(self, recursive: bool = False):
            return self.children_list

        def kill(self) -> None:
            self.killed = True

    monkeypatch.setattr(run_napcat.psutil, "Process", lambda pid: FakeParent())
    monkeypatch.setattr(
        run_napcat,
        "it",
        lambda cls: SimpleNamespace(remove_login_state=lambda qq_id: removed_login_states.append(qq_id)),
    )

    manager.stop_process("334455")

    assert removed_login_states == ["334455"]
    assert process.killed is True
    assert process.waited is True
    assert process.deleted is True
    assert "334455" not in manager.napcat_process_dict
    assert state_changes == [("334455", QProcess.ProcessState.NotRunning)]


def test_get_memory_usage_sums_process_tree(monkeypatch: pytest.MonkeyPatch, mute_run_napcat_logger) -> None:
    """内存统计应汇总主进程和子进程 RSS。"""
    manager = run_napcat.ManagerNapCatQQProcess()
    process = FakeManagedProcess(pid=1)
    manager.napcat_process_dict["556677"] = run_napcat.NapCatProcessModel(
        qq_id="556677",
        process=process,
        state=QProcess.ProcessState.Running,
        started_at=1.0,
    )

    class FakePsProcess:
        def __init__(self, pid: int) -> None:
            self.pid = pid

        def memory_info(self):
            rss_map = {1: 5 * 1024 * 1024, 2: 3 * 1024 * 1024}
            return SimpleNamespace(rss=rss_map[self.pid])

        def children(self):
            child_map = {1: [SimpleNamespace(pid=2)], 2: []}
            return child_map[self.pid]

    monkeypatch.setattr(run_napcat.psutil, "Process", lambda pid: FakePsProcess(pid))

    assert manager.get_memory_usage("556677") == 8

