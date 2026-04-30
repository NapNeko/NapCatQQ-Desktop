# -*- coding: utf-8 -*-
"""[`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py): 后台执行远端部署。

仿 [`ConnectionTester`](src/ui/page/remote_page/connection_tester.py) 的模式,
通过 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 把 [`ServerManager.deploy_server`](src/core/remote/server_manager.py)
推到 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 里执行。

注意: 本 runner 仅负责调度, **不**重复 emit `progress` / `finished` 信号:
- 进度实时性: 由 [`ServerManager.deployment_progress`](src/core/remote/server_manager.py) 直接 emit, UI 端订阅该信号即可
- 终结回调: 同上, 由 ServerManager 在 deploy_server 内部 emit ``deployment_finished``
- runner 自带的 ``finished`` 仅用于 UI 端复位临时按钮态(无差错语义)
"""

from __future__ import annotations

from PySide6.QtCore import QObject, QRunnable, Signal

from src.core.logging import LogSource, LogType, logger


class DeploymentRunnerSignals(QObject):
    """[`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体。"""

    finished = Signal(str)  # (server_id)


class DeploymentRunner(QRunnable):
    """后台执行 [`ServerManager.deploy_server`](src/core/remote/server_manager.py)。

    用法::

        runner = DeploymentRunner(server_id="...", force_napcat_update=False)
        runner.signals.finished.connect(on_runner_finished)
        QThreadPool.globalInstance().start(runner)

        # 进度 / 成败请订阅 ServerManager 的 deployment_progress / deployment_finished 信号
    """

    def __init__(
        self,
        server_id: str,
        *,
        force_napcat_update: bool = False,
        force_linuxqq_reinstall: bool = False,
    ) -> None:
        super().__init__()
        self.signals = DeploymentRunnerSignals()
        self._server_id = server_id
        self._force_napcat_update = force_napcat_update
        self._force_linuxqq_reinstall = force_linuxqq_reinstall
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        from creart import it

        from src.core.remote import ServerManager

        manager = it(ServerManager)
        try:
            manager.deploy_server(
                self._server_id,
                force_napcat_update=self._force_napcat_update,
                force_linuxqq_reinstall=self._force_linuxqq_reinstall,
            )
        except Exception as exc:  # noqa: BLE001
            # ServerManager 已经 emit 过 deployment_finished, 此处仅记录日志
            logger.warning(
                f"DeploymentRunner 捕获部署异常: server_id={self._server_id}, exc={exc}",
                LogType.NETWORK,
                LogSource.UI,
            )
        finally:
            self.signals.finished.emit(self._server_id)


class RedetectRunnerSignals(QObject):
    """[`RedetectRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体。"""

    # (server_id, ok, napcat_version, qq_version, error_msg)
    finished = Signal(str, bool, object, object, str)


class RedetectRunner(QRunnable):
    """后台执行 [`ServerManager.redetect_versions`](src/core/remote/server_manager.py)。

    相比 [`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py) 仅探测版本,
    不重新执行安装脚本; 用于"刷新"按钮等轻量场景。
    """

    def __init__(self, server_id: str) -> None:
        super().__init__()
        self.signals = RedetectRunnerSignals()
        self._server_id = server_id
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        from creart import it

        from src.core.remote import ServerManager

        manager = it(ServerManager)
        try:
            napcat_version, qq_version = manager.redetect_versions(self._server_id)
            self.signals.finished.emit(self._server_id, True, napcat_version, qq_version, "")
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"RedetectRunner 捕获异常: server_id={self._server_id}, exc={exc}",
                LogType.NETWORK,
                LogSource.UI,
            )
            self.signals.finished.emit(self._server_id, False, None, None, str(exc))


class RollbackRunnerSignals(QObject):
    """[`RollbackRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体。"""

    # (server_id, ok, message)
    finished = Signal(str, bool, str)


class RollbackRunner(QRunnable):
    """后台执行 [`ServerManager.rollback_server`](src/core/remote/server_manager.py)。

    仅供开发者模式 (设置→开发者→远程部署调试) 使用。
    """

    def __init__(self, server_id: str, *, include_qq: bool = True) -> None:
        super().__init__()
        self.signals = RollbackRunnerSignals()
        self._server_id = server_id
        self._include_qq = include_qq
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - 实现 QRunnable.run
        from creart import it

        from src.core.remote import ServerManager

        manager = it(ServerManager)
        try:
            manager.rollback_server(self._server_id, include_qq=self._include_qq)
            self.signals.finished.emit(self._server_id, True, "回滚完成")
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"RollbackRunner 捕获回滚异常: server_id={self._server_id}, exc={exc}",
                LogType.NETWORK,
                LogSource.UI,
            )
            self.signals.finished.emit(self._server_id, False, str(exc))
