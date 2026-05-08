# -*- coding: utf-8 -*-
"""[`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py): 后台执行远端部署. 

仿 [`ConnectionTester`](src/ui/page/remote_page/connection_tester.py) 的模式,
通过 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 把 [`ServerManager.deploy_server`](src/core/remote/server_manager.py)
推到 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 里执行. 

注意: 本 runner 仅负责调度, **不**重复 emit `progress` / `finished` 信号:
- 进度实时性: 由 [`ServerManager.deployment_progress`](src/core/remote/server_manager.py) 直接 emit, UI 端订阅该信号即可
- 终结回调: 同上, 由 ServerManager 在 deploy_server 内部 emit ``deployment_finished``
- runner 自带的 ``finished`` 仅用于 UI 端复位临时按钮态(无差错语义)
"""

from __future__ import annotations

from contextlib import contextmanager
from typing import Iterator

from PySide6.QtCore import QObject, QRunnable, Signal

from src.core.logging import LogSource, LogType, logger


class _TaskTracker:
    """BackgroundTaskCenter 报到把手, 支持显式调 :meth:`success` / :meth:`fail`.

    runnable 在 ``with _tracked(...)`` 块中可以根据自己的结果调用对应方法,
    桥接到 [`ProgressInfoBar`](src/ui/components/progress_info_bar_bridge.py) 的 ✅/❌
    + 完成文案. 块结束时若都没调过, 默认按"成功 + 无文案"处理.
    """

    def __init__(self, task_id: str) -> None:
        self.task_id = task_id
        self._resolved = False
        self._success = True
        self._message = ""

    def success(self, message: str = "") -> None:
        if self._resolved:
            return
        self._resolved = True
        self._success = True
        self._message = message

    def fail(self, message: str = "") -> None:
        if self._resolved:
            return
        self._resolved = True
        self._success = False
        self._message = message

    def is_resolved(self) -> bool:
        return self._resolved

    def report_to(self, center) -> None:  # noqa: ANN001 - center 是可选 BackgroundTaskCenter
        if center is None:
            return
        try:
            if self._success:
                center.end(self.task_id, success=True, message=self._message)
            else:
                center.fail(self.task_id, self._message)
        except Exception:  # noqa: BLE001
            pass


@contextmanager
def _tracked(task_id: str, label: str, *, content: str = "") -> Iterator[_TaskTracker]:
    """BackgroundTaskCenter 透明上报包装; center 不可用时静默直通.

    P3 perf: 许多远端运维 runnable 共享 [包裹 try/finally 报到 Center] 的需求,
    集中在这里实现; runnable 内部 ``with _tracked(...) as tracker:``, 完成时调
    ``tracker.success("...")`` / ``tracker.fail("...")`` 即可获得 ProgressInfoBar 反馈.
    """
    center = None
    try:
        from creart import it
        from src.core.runtime.background_tasks import BackgroundTaskCenter

        center = it(BackgroundTaskCenter)
        center.begin(task_id, label, content=content)
    except Exception:  # noqa: BLE001
        center = None

    tracker = _TaskTracker(task_id)
    try:
        yield tracker
    except BaseException as exc:  # noqa: BLE001 - 异常路径标记失败再上抛
        tracker.fail(f"{type(exc).__name__}: {exc}")
        raise
    finally:
        tracker.report_to(center)


class DeploymentRunnerSignals(QObject):
    """[`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体. """

    finished = Signal(str)  # (server_id)


class DeploymentRunner(QRunnable):
    """后台执行 [`ServerManager.deploy_server`](src/core/remote/server_manager.py). 

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
        if self._force_napcat_update:
            label = f"强制更新 NapCat ({self._server_id})"
            success_msg = "远端 NapCat 强制更新完成"
        elif self._force_linuxqq_reinstall:
            label = f"强制重装 LinuxQQ ({self._server_id})"
            success_msg = "远端 LinuxQQ 强制重装完成"
        else:
            label = f"部署远端 NapCat ({self._server_id})"
            success_msg = "远端部署完成"
        with _tracked(
            f"deploy-{self._server_id}",
            label,
            content="正在通过 SSH 执行部署脚本…",
        ) as tracker:
            try:
                manager.deploy_server(
                    self._server_id,
                    force_napcat_update=self._force_napcat_update,
                    force_linuxqq_reinstall=self._force_linuxqq_reinstall,
                )
                tracker.success(success_msg)
            except Exception as exc:  # noqa: BLE001
                # ServerManager 已经 emit 过 deployment_finished, 此处仅记录日志
                logger.warning(
                    f"DeploymentRunner 捕获部署异常: server_id={self._server_id}, exc={exc}",
                    LogType.NETWORK,
                    LogSource.UI,
                )
                tracker.fail(f"{type(exc).__name__}: {exc}")
            finally:
                self.signals.finished.emit(self._server_id)


class RedetectRunnerSignals(QObject):
    """[`RedetectRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体. """

    # (server_id, ok, napcat_version, qq_version, error_msg)
    finished = Signal(str, bool, object, object, str)


class RedetectRunner(QRunnable):
    """后台执行 [`ServerManager.redetect_versions`](src/core/remote/server_manager.py). 

    相比 [`DeploymentRunner`](src/ui/page/remote_page/deployment_runner.py) 仅探测版本,
    不重新执行安装脚本; 用于"刷新"按钮等轻量场景. 
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
        with _tracked(
            f"redetect-{self._server_id}",
            f"检测远端版本 ({self._server_id})",
            content="正在与远端交互, 读取 NapCat / LinuxQQ 版本信息…",
        ) as tracker:
            try:
                napcat_version, qq_version = manager.redetect_versions(self._server_id)
                self.signals.finished.emit(self._server_id, True, napcat_version, qq_version, "")
                tracker.success(
                    f"NapCat={napcat_version or '未探测到'}, "
                    f"QQ={qq_version or '未探测到'}"
                )
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"RedetectRunner 捕获异常: server_id={self._server_id}, exc={exc}",
                    LogType.NETWORK,
                    LogSource.UI,
                )
                self.signals.finished.emit(self._server_id, False, None, None, str(exc))
                tracker.fail(f"{type(exc).__name__}: {exc}")


class RollbackRunnerSignals(QObject):
    """[`RollbackRunner`](src/ui/page/remote_page/deployment_runner.py) 信号载体. """

    # (server_id, ok, message)
    finished = Signal(str, bool, str)


class RollbackRunner(QRunnable):
    """后台执行 [`ServerManager.rollback_server`](src/core/remote/server_manager.py). 

    仅供开发者模式 (设置→开发者→远程部署调试) 使用. 
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
        with _tracked(
            f"rollback-{self._server_id}",
            f"回滚远端部署 ({self._server_id})",
            content="正在清理远端 NapCat / LinuxQQ 产物…",
        ) as tracker:
            try:
                manager.rollback_server(self._server_id, include_qq=self._include_qq)
                self.signals.finished.emit(self._server_id, True, "回滚完成")
                tracker.success("回滚完成")
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"RollbackRunner 捕获回滚异常: server_id={self._server_id}, exc={exc}",
                    LogType.NETWORK,
                    LogSource.UI,
                )
                self.signals.finished.emit(self._server_id, False, str(exc))
                tracker.fail(f"{type(exc).__name__}: {exc}")
