# -*- coding: utf-8 -*-
"""SSH 连接测试后台运行器, 避免在 UI 线程内同步阻塞。"""

from __future__ import annotations

from PySide6.QtCore import QObject, QRunnable, Signal

from src.core.remote import ServerProfile


class ConnectionTesterSignals(QObject):
    """[`ConnectionTester`](src/ui/page/remote_page/connection_tester.py) 信号载体。

    Qt 不允许 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 直接定义信号,
    需通过独立 [`QObject`](https://doc.qt.io/qt-6/qobject.html) 中转。
    """

    finished = Signal(str, bool, str)  # (server_id, ok, message)


class ConnectionTester(QRunnable):
    """后台执行 SSH 连接测试。

    用法::

        from creart import it
        from src.core.remote import ServerManager
        from PySide6.QtCore import QThreadPool

        tester = ConnectionTester(profile=profile, password=password)
        tester.signals.finished.connect(on_finished)
        QThreadPool.globalInstance().start(tester)
    """

    def __init__(self, profile: ServerProfile, *, password: str | None = None) -> None:
        super().__init__()
        self.signals = ConnectionTesterSignals()
        self._profile = profile
        self._password = password
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        from creart import it

        from src.core.remote import ServerManager
        from src.ui.page.remote_page.deployment_runner import _tracked

        manager = it(ServerManager)
        with _tracked(
            f"ssh-test-{self._profile.id}",
            f"测试 SSH 连接 ({self._profile.name or self._profile.id})",
            content="正在尝试建立 SSH 会话…",
        ) as tracker:
            ok, message = manager.test_connection(self._profile, password=self._password)
            self.signals.finished.emit(self._profile.id, ok, message)
            if ok:
                tracker.success(message or "SSH 连接成功")
            else:
                tracker.fail(message or "SSH 连接失败")
