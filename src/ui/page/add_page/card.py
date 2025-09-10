# -*- coding: utf-8 -*-

# 标准库导入
from typing import Optional, cast

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import (
    HeaderCardWidget,
    PillPushButton,
    PushButton,
    TeachingTip,
    TeachingTipTailPosition,
    TeachingTipView,
    TransparentToolButton,
)
from PySide6.QtCore import QObject, Qt
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QGridLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import (
    BaseModel,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.page.add_page.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)


class EnableTag(PillPushButton):
    """显示启用/禁用状态的标签控件"""

    def __init__(self, status: bool, parent: Optional[QObject] = None) -> None:
        """初始化启用标签

        Args:
            status: 初始状态，True 为启用，False 为禁用
            parent: 父控件
        """
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 8))
        self.update_status(status)

    def update_status(self, status: bool) -> None:
        """更新标签显示状态

        Args:
            status: 新的状态值，True 为启用，False 为禁用
        """
        if status:
            self.setText(self.tr("启用"))
        else:
            self.setText(self.tr("禁用"))


class FormateTag(PillPushButton):
    """消息格式显示标签控件"""

    def __init__(self, format_str: str, parent: Optional[QObject] = None) -> None:
        """初始化格式标签

        Args:
            format_str: 消息格式字符串
            parent: 父控件
        """
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 8))
        self.update_format(format_str)

    def update_format(self, format_str: str) -> None:
        """更新格式显示

        Args:
            format_str: 新的格式字符串
        """
        self.setText(format_str)


class ConfigCardBase(HeaderCardWidget):
    """配置卡片基类，提供通用的配置显示和操作功能"""

    def __init__(self, config: BaseModel, parent: Optional[QObject] = None) -> None:
        """初始化配置卡片基类

        Args:
            config: 配置数据模型
            parent: 父控件
        """
        super().__init__(parent)

        # 属性
        self.config = config
        self.config_view: Optional[QWidget] = None
        self.config_view_layout: Optional[QGridLayout] = None
        self.remove_button: Optional[TransparentToolButton] = None
        self.edit_button: Optional[TransparentToolButton] = None

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()

    def _create_widgets(self) -> None:
        """创建子控件"""
        self.remove_button = TransparentToolButton(FI.DELETE, self)
        self.edit_button = TransparentToolButton(FI.EDIT, self)
        self.config_view = QWidget(self)

    def _setup_layout(self) -> None:
        """设置控件布局"""
        self.setTitle(self.config.name)
        self.setFixedSize(335, 170)

        # 设置布局
        self.headerLayout.addStretch(1)
        self.headerLayout.addWidget(self.remove_button)
        self.headerLayout.addWidget(self.edit_button)
        self.viewLayout.addWidget(self.config_view)
        self.viewLayout.setContentsMargins(16, 16, 16, 16)

        self.config_view_layout = QGridLayout(self.config_view)
        self.config_view_layout.setContentsMargins(0, 0, 0, 0)
        self.config_view_layout.setVerticalSpacing(10)

    def _connect_signals(self) -> None:
        """连接信号与槽"""
        self.remove_button.clicked.connect(self._on_remove_button_clicked)
        self.edit_button.clicked.connect(self._on_edit_button_clicked)

    # ==================== 公共方法 ====================
    def fill_value(self) -> None:
        """填充配置数据显示 - 由子类实现"""
        raise NotImplementedError("子类必须实现 fill_value 方法")

    def get_value(self) -> BaseModel:
        """获取配置数据

        Returns:
            BaseModel: 配置数据模型
        """
        return self.config

    # ==================== 槽函数 ====================
    def _on_remove_button_clicked(self) -> None:
        """处理删除按钮点击事件"""
        # 项目内模块导入
        from src.ui.page.add_page.signal_bus import add_page_singal_bus

        view = TeachingTipView(
            title=self.tr("删除配置"),
            content=self.tr("确定要删除该配置吗?这个操作不可逆!"),
            isClosable=False,
            tailPosition=TeachingTipTailPosition.TOP,
        )
        button = PushButton(self.tr("删除"), self)

        view.addWidget(button, align=Qt.AlignmentFlag.AlignRight)

        widget = TeachingTip.make(
            target=self.remove_button,
            view=view,
            duration=2000,
            tailPosition=TeachingTipTailPosition.BOTTOM,
            parent=self,
        )
        view.closed.connect(widget.close)
        button.clicked.connect(lambda: add_page_singal_bus.remove_card_signal.emit(self))
        button.clicked.connect(widget.close)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件 - 由子类实现"""
        raise NotImplementedError("子类必须实现 _on_edit_button_clicked 方法")


class HttpServerConfigCard(ConfigCardBase):
    """HTTP 服务器配置卡片"""

    def __init__(self, config: HttpServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP 服务器配置卡片

        Args:
            config: HTTP 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.cors_label = BodyLabel(self.tr("CORS"), self)
        self.cors_config_label = EnableTag(self.config.enableCors, self)

        self.websocket_label = BodyLabel(self.tr("WS"), self)
        self.websocket_config_label = EnableTag(self.config.enableWebsocket, self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.cors_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.cors_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.websocket_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.websocket_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 2, 1, 1, 5)

    def fill_value(self) -> None:
        """填充 HTTP 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.cors_config_label.update_status(self.config.enableCors)
        self.websocket_config_label.update_status(self.config.enableWebsocket)
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)

    def get_value(self) -> HttpServersConfig:
        """获取 HTTP 服务器配置数据

        Returns:
            HttpServersConfig: HTTP 服务器配置
        """
        return cast(HttpServersConfig, self.config)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpServerConfigDialog(MainWindow(), cast(HttpServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_value()


class HttpSSEConfigCard(ConfigCardBase):
    """HTTP SSE 服务器配置卡片"""

    def __init__(self, config: HttpSseServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP SSE 服务器配置卡片

        Args:
            config: HTTP SSE 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.cors_label = BodyLabel(self.tr("CORS"), self)
        self.cors_config_label = EnableTag(self.config.enableCors, self)

        self.websocket_label = BodyLabel(self.tr("WS"), self)
        self.websocket_config_label = EnableTag(self.config.enableWebsocket, self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.cors_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.cors_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.websocket_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.websocket_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 4, 1, 2)

    def fill_value(self) -> None:
        """填充 HTTP SSE 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.cors_config_label.update_status(self.config.enableCors)
        self.websocket_config_label.update_status(self.config.enableWebsocket)
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_value(self) -> HttpSseServersConfig:
        """获取 HTTP SSE 服务器配置数据

        Returns:
            HttpSseServersConfig: HTTP SSE 服务器配置
        """
        return cast(HttpSseServersConfig, self.config)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpSSEServerConfigDialog(MainWindow(), cast(HttpSseServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_value()


class HttpClientConfigCard(ConfigCardBase):
    """HTTP 客户端配置卡片"""

    def __init__(self, config: HttpClientsConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP 客户端配置卡片

        Args:
            config: HTTP 客户端配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.url_label = BodyLabel(self.tr("URL"), self)
        self.url_config_label = BodyLabel(str(self.config.url), self)

        self.format_label = BodyLabel(self.tr("格式"), self)
        self.format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.url_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.url_config_label, 0, 1, 1, 6)

        self.config_view_layout.addWidget(self.format_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.format_config_label, 1, 1, 1, 1)

        self.config_view_layout.addWidget(self.report_self_message_label, 1, 4, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 1, 5, 1, 1)

    def fill_value(self) -> None:
        """填充 HTTP 客户端配置数据"""
        self.url_config_label.setText(str(self.config.url))
        self.format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_value(self) -> HttpClientsConfig:
        """获取 HTTP 客户端配置数据

        Returns:
            HttpClientsConfig: HTTP 客户端配置
        """
        return cast(HttpClientsConfig, self.config)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpClientConfigDialog(MainWindow(), cast(HttpClientsConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_value()


class WebsocketServersConfigCard(ConfigCardBase):
    """WebSocket 服务器配置卡片"""

    def __init__(self, config: WebsocketServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 WebSocket 服务器配置卡片

        Args:
            config: WebSocket 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.heart_interval_label = BodyLabel(self.tr("心跳间隔"), self)
        self.heart_interval_config_label = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        self.enable_force_push_event_label = BodyLabel(self.tr("强制推送事件"), self)
        self.enable_force_push_event_config_label = EnableTag(self.config.enableForcePushEvent, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.heart_interval_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.heart_interval_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.enable_force_push_event_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.enable_force_push_event_config_label, 2, 4, 1, 2)

    def fill_value(self) -> None:
        """填充 WebSocket 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.heart_interval_config_label.setText(str(self.config.heartInterval) + "ms")
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)
        self.enable_force_push_event_config_label.update_status(self.config.enableForcePushEvent)

    def get_value(self) -> WebsocketServersConfig:
        """获取 WebSocket 服务器配置数据

        Returns:
            WebsocketServersConfig: WebSocket 服务器配置
        """
        return cast(WebsocketServersConfig, self.config)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = WebsocketServerConfigDialog(MainWindow(), cast(WebsocketServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_value()


class WebsocketClientConfigCard(ConfigCardBase):
    """WebSocket 客户端配置卡片"""

    def __init__(self, config: WebsocketClientsConfig, parent: Optional[QObject] = None) -> None:
        """初始化 WebSocket 客户端配置卡片

        Args:
            config: WebSocket 客户端配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.url_label = BodyLabel(self.tr("URL"), self)
        self.url_config_label = BodyLabel(str(self.config.url), self)

        self.reconnect_interval_label = BodyLabel(self.tr("重连间隔"), self)
        self.reconnect_interval_config_label = BodyLabel(str(self.config.reconnectInterval) + "ms", self)

        self.heart_interval_label = BodyLabel(self.tr("心跳间隔"), self)
        self.heart_interval_config_label = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.format_label = BodyLabel(self.tr("格式"), self)
        self.format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.url_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.url_config_label, 0, 1, 1, 6)

        self.config_view_layout.addWidget(self.reconnect_interval_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.reconnect_interval_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.heart_interval_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.heart_interval_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.format_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 4, 1, 2)

    def fill_value(self) -> None:
        """填充 WebSocket 客户端配置数据"""
        self.url_config_label.setText(str(self.config.url))
        self.reconnect_interval_config_label.setText(str(self.config.reconnectInterval) + "ms")
        self.heart_interval_config_label.setText(str(self.config.heartInterval) + "ms")
        self.format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_value(self) -> WebsocketClientsConfig:
        """获取 WebSocket 客户端配置数据

        Returns:
            WebsocketClientsConfig: WebSocket 客户端配置
        """
        return cast(WebsocketClientsConfig, self.config)

    def _on_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = WebsocketClientConfigDialog(MainWindow(), cast(WebsocketClientsConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_value()
