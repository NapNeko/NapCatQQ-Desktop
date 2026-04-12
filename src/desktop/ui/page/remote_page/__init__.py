# -*- coding: utf-8 -*-
"""远程管理页面。

主路径仅保留 Agent/Daemon 管理，SSH 仅作为一次性部署向导出现。
"""
from __future__ import annotations

from abc import ABC
import contextlib
import warnings
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QObject, QRunnable, Qt, QThreadPool, Signal, Slot
from PySide6.QtWidgets import QFrame, QHBoxLayout, QSplitter, QVBoxLayout, QWidget
from qfluentwidgets import TitleLabel

from src.desktop.core.logging import LogSource, LogType, logger
from src.desktop.core.remote import AgentConnectionConfig
from src.desktop.ui.common.style_sheet import PageStyleSheet
from src.desktop.ui.components.info_bar import error_bar, success_bar

from .agent_handler import AgentHandler
from .agent_panel import AgentConfigPanel
from .connection_base import ConnectionInfo, ConnectionState, ServerStatus
from .status_panel import StatusPanel

if TYPE_CHECKING:
    from src.desktop.ui.window.main_window import MainWindow


class RemoteActionTask(QObject, QRunnable):
    """远程操作异步任务。"""

    success_signal = Signal(str)
    error_signal = Signal(str)

    def __init__(self, name: str, handler, *args, **kwargs) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._name = name
        self._handler = handler
        self._args = args
        self._kwargs = kwargs

    def run(self) -> None:
        try:
            result = self._handler(*self._args, **self._kwargs)
            if isinstance(result, tuple):
                success, message = result
                if success:
                    self.success_signal.emit(message)
                else:
                    self.error_signal.emit(message)
            else:
                self.success_signal.emit(str(result))
        except Exception as exc:  # pragma: no cover - UI 异步异常统一反馈
            logger.exception(
                f"远程操作执行异常: {self._name}",
                exc,
                log_type=LogType.NETWORK,
                log_source=LogSource.UI,
            )
            self.error_signal.emit(f"{type(exc).__name__}: {exc}")


class RemotePage(QWidget):
    """远程管理页面 - 单主路径 Agent/Daemon 实现。"""

    def __init__(self) -> None:
        super().__init__()
        self._current_handler: AgentHandler | None = None
        self._agent_handler: AgentHandler | None = None

    def initialize(self, parent: "MainWindow") -> "RemotePage":
        """初始化页面。"""
        self.setParent(parent)
        self.setObjectName("RemotePage")

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self._load_saved_config()

        PageStyleSheet.REMOTE.apply(self)
        return self

    def _create_widgets(self) -> None:
        """创建控件。"""
        self.title_label = TitleLabel("远程 Daemon 管理", self)

        self.splitter = QSplitter(Qt.Orientation.Horizontal, self)
        self.splitter.setChildrenCollapsible(False)
        self.splitter.setHandleWidth(1)

        self.left_panel = QFrame(self.splitter)
        self.left_panel.setObjectName("leftPanel")
        self.config_container = QWidget(self.left_panel)
        self.agent_panel = AgentConfigPanel(self.config_container)

        self.right_panel = StatusPanel(self.splitter)
        self.right_panel.set_deploy_visible(False)

        self.splitter.addWidget(self.left_panel)
        self.splitter.addWidget(self.right_panel)
        self.splitter.setSizes([450, 550])

    def _setup_layout(self) -> None:
        """设置布局。"""
        main_layout = QVBoxLayout(self)
        main_layout.setContentsMargins(24, 20, 24, 20)
        main_layout.setSpacing(16)

        header_layout = QHBoxLayout()
        header_layout.setSpacing(16)
        header_layout.addWidget(self.title_label)
        header_layout.addStretch()
        main_layout.addLayout(header_layout)

        left_layout = QVBoxLayout(self.left_panel)
        left_layout.setContentsMargins(0, 0, 16, 0)
        left_layout.setSpacing(0)

        config_layout = QVBoxLayout(self.config_container)
        config_layout.setContentsMargins(0, 0, 0, 0)
        config_layout.addWidget(self.agent_panel)
        left_layout.addWidget(self.config_container, 1)

        main_layout.addWidget(self.splitter, 1)
        self.setLayout(main_layout)

    def _connect_signals(self) -> None:
        """连接信号。"""
        self.agent_panel.test_btn.clicked.connect(self._on_agent_test)
        self.agent_panel.save_btn.clicked.connect(self.agent_panel.save_config)

        self.right_panel.connect_btn.clicked.connect(self._on_connect)
        self.right_panel.disconnect_btn.clicked.connect(self._on_disconnect)
        self.right_panel.start_btn.clicked.connect(self._on_start)
        self.right_panel.stop_btn.clicked.connect(self._on_stop)
        self.right_panel.restart_btn.clicked.connect(self._on_restart)
        self.right_panel.clear_log_btn.clicked.connect(self.right_panel.clear_log)

    def _load_saved_config(self) -> None:
        """加载保存的配置。"""
        self._current_handler = self._get_agent_handler()
        self._connect_handler_signals()
        self.right_panel.set_connection_state(ConnectionState.DISCONNECTED)

    def _get_agent_handler(self) -> AgentHandler:
        """获取 Agent 处理器。"""
        if not self._agent_handler:
            self._agent_handler = AgentHandler(self)
        return self._agent_handler

    def _connect_handler_signals(self) -> None:
        """连接处理器信号。"""
        if not self._current_handler:
            return

        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)
            with contextlib.suppress(RuntimeError, TypeError):
                self._current_handler.state_changed.disconnect(self._on_state_changed)
            with contextlib.suppress(RuntimeError, TypeError):
                self._current_handler.status_updated.disconnect(self._on_status_updated)
            with contextlib.suppress(RuntimeError, TypeError):
                self._current_handler.log_received.disconnect(self._on_log_received)
            with contextlib.suppress(RuntimeError, TypeError):
                self._current_handler.error_occurred.disconnect(self._on_error)

        self._current_handler.state_changed.connect(self._on_state_changed)
        self._current_handler.status_updated.connect(self._on_status_updated)
        self._current_handler.log_received.connect(self._on_log_received)
        self._current_handler.error_occurred.connect(self._on_error)

    @Slot(object)
    def _on_state_changed(self, state: ConnectionState) -> None:
        """连接状态变化。"""
        self.right_panel.set_connection_state(state)

    @Slot(object)
    def _on_status_updated(self, status: ServerStatus) -> None:
        """服务器状态更新。"""
        self.right_panel.update_server_status(status)

    @Slot(str)
    def _on_log_received(self, message: str) -> None:
        """收到日志。"""
        self.right_panel.append_log(message)

    @Slot(str)
    def _on_error(self, message: str) -> None:
        """错误发生。"""
        error_bar(message, parent=self)
        self.right_panel.append_log(f"[错误] {message}")

    def _run_task(self, name: str, handler, *args, **kwargs) -> None:
        """运行异步任务。"""
        task = RemoteActionTask(name, handler, *args, **kwargs)
        task.success_signal.connect(lambda m: success_bar(m, parent=self))
        task.error_signal.connect(lambda m: error_bar(m, parent=self))
        QThreadPool.globalInstance().start(task)

    def _on_agent_test(self) -> None:
        """测试 Agent 连接。"""
        handler = self._get_agent_handler()
        host, port, token = self.agent_panel.get_connection_info()

        if not host or not token:
            error_bar("请填写主机和 Token", parent=self)
            return

        def test():
            handler._config = AgentConnectionConfig(host=host, port=port, token=token, use_ssl=True)
            return handler.test_connection()

        self._run_task("测试 Agent", test)

    def _on_connect(self) -> None:
        """连接服务器。"""
        if not self._current_handler:
            return

        host, port, token = self.agent_panel.get_connection_info()
        if not host or not token:
            error_bar("请填写主机和 Token", parent=self)
            return

        info = ConnectionInfo(host=host, port=port, mode="agent")
        self._current_handler.connect(info, token=token)

    def _on_disconnect(self) -> None:
        """断开连接。"""
        if self._current_handler:
            self._current_handler.disconnect()

    def _on_start(self) -> None:
        """启动 NapCat。"""
        if self._current_handler:
            self._run_task("启动 NapCat", self._current_handler.start_napcat)

    def _on_stop(self) -> None:
        """停止 NapCat。"""
        if self._current_handler:
            self._run_task("停止 NapCat", self._current_handler.stop_napcat)

    def _on_restart(self) -> None:
        """重启 NapCat。"""
        if self._current_handler:
            self._run_task("重启 NapCat", self._current_handler.restart_napcat)


class RemotePageCreator(AbstractCreator, ABC):
    """远程页面创建器。"""

    targets = (CreateTargetInfo("src.desktop.ui.page.remote_page", "RemotePage"),)

    @staticmethod
    def available() -> bool:
        return exists_module("src.desktop.ui.page.remote_page")

    def create(self, *args, **kwargs) -> RemotePage:
        return RemotePage(*args, **kwargs)


add_creator(RemotePageCreator)
