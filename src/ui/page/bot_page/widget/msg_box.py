# -*- coding: utf-8 -*-

# 第三方库导入
import qrcode
from abc import ABC
from pydantic import ValidationError
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import (
    MessageBoxBase,
    RadioButton,
    SimpleCardWidget,
    TitleLabel,
    ImageLabel,
    PipsPager,
    PipsScrollButtonDisplayMode,
)
from PySide6.QtCore import QObject, Qt, Slot
from PySide6.QtWidgets import QButtonGroup, QGridLayout, QVBoxLayout, QWidget, QHBoxLayout
from PySide6.QtGui import QPixmap

# 项目内模块导入
from src.core.config.config_model import (
    AutoRestartScheduleConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.components.input_card.generic_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard
from src.ui.components.input_card.time_card import IntervalTimeConfigCard
from src.ui.page.bot_page.utils.enum import ConnectType
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from src.ui.components.stacked_widget import TransparentStackedWidget
from io import BytesIO


class ChooseConfigCard(SimpleCardWidget):
    """选择配置类型的卡片控件"""

    def __init__(self, button: RadioButton, content: str, parent: "ChooseConfigTypeDialog") -> None:
        """初始化选择配置卡片

        Args:
            button: 单选框按钮
            content: 内容描述文本
            parent: 父级对话框
        """
        super().__init__(parent)

        # 创建控件
        self.button = button
        self.content_label = BodyLabel(content, self)
        self.v_box_layout = QVBoxLayout(self)

        # 设置属性
        self.content_label.setWordWrap(True)
        self.setMinimumSize(175, 128)

        # 设置布局
        self.v_box_layout.setContentsMargins(16, 16, 16, 16)
        self.v_box_layout.setSpacing(4)
        self.v_box_layout.addWidget(self.button, alignment=Qt.AlignmentFlag.AlignTop)
        self.v_box_layout.addWidget(self.content_label, alignment=Qt.AlignmentFlag.AlignTop)

        # 连接信号
        self.clicked.connect(lambda: self.button.setChecked(True))


class ChooseConfigTypeDialog(MessageBoxBase):
    """选择配置类型的对话框"""

    def __init__(self, parent: QObject) -> None:
        """初始化配置类型选择对话框

        Args:
            parent: 父级对象
        """
        super().__init__(parent=parent)

        # 配置类型说明内容
        contents = [
            self.tr("「由NapCat建立」的HTTP服务器, 可「用框架连接」或「手动发送请求」。NapCat会提供配置的地址供连接。"),
            self.tr("「由NapCat建立」的HTTP SSE服务器, 可「用框架连接」或「手动发送请求」。NapCat会提供配置的地址。"),
            self.tr("「由框架或自建」的HTTP客户端, 用于接收NapCat的请求。通常是框架提供的地址, NapCat会主动连接。"),
            self.tr("「由NapCat建立」的WebSocket服务器, 你的框架需要连接其提供的地址。"),
            self.tr("「由框架提供」的WebSocket地址, NapCat会主动连接。"),
        ]

        # 创建控件
        self.title_label = TitleLabel(self.tr("请选择配置类型"), self)
        self.button_group = QButtonGroup(self)
        self.http_server_config_button = RadioButton(self.tr("HTTP 服务器"))
        self.http_sse_server_config_button = RadioButton(self.tr("HTTP SSE 服务器"))
        self.http_client_config_button = RadioButton(self.tr("HTTP 客户端"))
        self.web_socket_server_config_button = RadioButton(self.tr("WebSocket 服务器"))
        self.web_socket_client_config_button = RadioButton(self.tr("WebSocket 客户端"))

        self.http_server_card = ChooseConfigCard(self.http_server_config_button, contents[0], self)
        self.http_sse_server_card = ChooseConfigCard(self.http_sse_server_config_button, contents[1], self)
        self.http_client_card = ChooseConfigCard(self.http_client_config_button, contents[2], self)
        self.web_socket_server_card = ChooseConfigCard(self.web_socket_server_config_button, contents[3], self)
        self.web_socket_client_card = ChooseConfigCard(self.web_socket_client_config_button, contents[4], self)

        # 设置属性
        self.button_group.setExclusive(True)
        self.button_group.addButton(self.http_server_config_button, 1)
        self.button_group.addButton(self.http_sse_server_config_button, 2)
        self.button_group.addButton(self.http_client_config_button, 3)
        self.button_group.addButton(self.web_socket_server_config_button, 4)
        self.button_group.addButton(self.web_socket_client_config_button, 5)
        self.widget.setMinimumSize(850, 400)

        # 设置布局
        self.card_layout = QGridLayout()
        self.card_layout.setSpacing(10)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.addWidget(self.http_server_card, 0, 0)
        self.card_layout.addWidget(self.http_sse_server_card, 0, 1)
        self.card_layout.addWidget(self.http_client_card, 0, 2)
        self.card_layout.addWidget(self.web_socket_server_card, 1, 0)
        self.card_layout.addWidget(self.web_socket_client_card, 1, 1)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.card_layout, stretch=1)

    def get_value(self) -> ConnectType:
        """获取选择的值

        Returns:
            ConnectType: 连接类型枚举
        """
        if (checked_id := self.button_group.checkedId()) != -1:
            # 将选中的 id 映射为 ConnectType。使用 ConnectType(checked_id) 比
            # list(ConnectType)[checked_id] 更稳健（避免枚举顺序/索引不一致的问题）。
            return ConnectType(checked_id)

        return ConnectType.NO_TYPE


class ConfigDialogBase(MessageBoxBase):
    """配置对话框基类，提供通用的配置界面和验证功能"""

    def __init__(self, parent: QObject, config: NetworkBaseConfig) -> None:
        """初始化配置对话框基类

        Args:
            parent: 父级对象
            config: 网络基础配置对象
        """
        super().__init__(parent)
        # 属性
        self.config = config

        # 创建控件
        self.title_label = TitleLabel(self)
        self.enable_card = SwitchConfigCard(FI.IOT, self.tr("启用"))
        self.name_card = LineEditConfigCard(FI.TAG, self.tr("名称*"), "输入配置名称", self.tr("设置配置名称"))
        self.msg_format_card = ComboBoxConfigCard(
            FI.MESSAGE, self.tr("消息格式"), ["array", "string"], self.tr("设置消息格式")
        )
        self.token_card = LineEditConfigCard(FI.VPN, self.tr("Token"), content=self.tr("设置连接Token"))
        self.debug_card = SwitchConfigCard(FI.DEVELOPER_TOOLS, self.tr("调试"))

        # 布局
        self.grid_layout = QGridLayout()
        self.grid_layout.setSpacing(8)
        self.grid_layout.setContentsMargins(0, 0, 0, 0)

        # 设置布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.grid_layout)

    def fill_config(self) -> None:
        """填充配置数据到界面控件

        如果传入了配置对象，则将配置值填充到对应的控件中
        """
        if self.config is None:
            return

        self.enable_card.fill_value(self.config.enable)
        self.debug_card.fill_value(self.config.debug)
        self.name_card.fill_value(self.config.name)
        self.msg_format_card.fill_value(self.config.messagePostFormat)
        self.token_card.fill_value(self.config.token)

        # 禁用名字卡片（编辑模式下名称不可修改）
        self.name_card.setEnabled(False)

    @Slot()
    def accept(self) -> None:
        """重写接受方法，验证配置有效性

        在点击确定按钮时验证配置，如果验证失败显示错误信息
        """
        try:
            # 验证配置
            self.get_config()
            # 关闭对话框
            super().accept()
        except ValidationError as e:
            # 显示错误信息
            if "配置错误请重试" in self.title_label.text():
                return
            self.title_label.setText(self.title_label.text() + f" - 配置错误请重试")

    def get_config(self) -> NetworkBaseConfig:
        """获取配置数据

        Returns:
            NetworkBaseConfig: 网络基础配置对象
        """
        return NetworkBaseConfig(
            **{
                "enable": self.enable_card.get_value(),
                "name": self.name_card.get_value(),
                "messagePostFormat": self.msg_format_card.get_value().lower(),
                "token": self.token_card.get_value(),
                "debug": self.debug_card.get_value(),
            }
        )


class HttpServerConfigDialog(ConfigDialogBase):
    """HTTP 服务器配置对话框"""

    config: HttpServersConfig | None

    def __init__(self, parent: QObject, config: HttpServersConfig | None = None) -> None:
        """初始化 HTTP 服务器配置对话框

        Args:
            parent: 父级对象
            config: HTTP 服务器配置对象，可选
        """
        super().__init__(parent, config)

        # 创建控件
        self.host_card = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.port_card = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.cors_card = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocket_card = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))

        # 设置属性
        self.title_label.setText("HTTP Server")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 1)
        self.grid_layout.addWidget(self.debug_card, 0, 1, 1, 1)
        self.grid_layout.addWidget(self.name_card, 2, 0, 1, 2)
        self.grid_layout.addWidget(self.host_card, 3, 0, 1, 2)
        self.grid_layout.addWidget(self.port_card, 4, 0, 1, 2)
        self.grid_layout.addWidget(self.cors_card, 5, 0, 1, 1)
        self.grid_layout.addWidget(self.websocket_card, 5, 1, 1, 1)
        self.grid_layout.addWidget(self.msg_format_card, 6, 0, 1, 2)
        self.grid_layout.addWidget(self.token_card, 7, 0, 1, 2)

        # 填充配置数据
        self.fill_config()

    def fill_config(self) -> None:
        """填充 HTTP 服务器配置数据"""
        if self.config is None:
            return

        super().fill_config()
        self.host_card.fill_value(self.config.host)
        self.port_card.fill_value(self.config.port)
        self.cors_card.fill_value(self.config.enableCors)
        self.websocket_card.fill_value(self.config.enableWebsocket)

    def get_config(self) -> HttpServersConfig:
        """获取 HTTP 服务器配置

        Returns:
            HttpServersConfig: HTTP 服务器配置对象
        """
        return HttpServersConfig(
            **{
                "host": self.host_card.get_value(),
                "port": self.port_card.get_value(),
                "enableCors": self.cors_card.get_value(),
                "enableWebsocket": self.websocket_card.get_value(),
                **super().get_config().model_dump(),
            }
        )


class HttpSSEServerConfigDialog(ConfigDialogBase):
    """HTTP SSE 服务器配置对话框"""

    config: HttpSseServersConfig | None

    def __init__(self, parent: QObject, config: HttpSseServersConfig | None = None) -> None:
        """初始化 HTTP SSE 服务器配置对话框

        Args:
            parent: 父级对象
            config: HTTP SSE 服务器配置对象，可选
        """
        super().__init__(parent, config)

        # 创建控件
        self.host_card = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.port_card = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.cors_card = SwitchConfigCard(FI.GLOBE, self.tr("CORS"))
        self.websocket_card = SwitchConfigCard(FI.SCROLL, self.tr("WebSocket"))
        self.report_self_msg_card = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))

        # 设置属性
        self.title_label.setText("HTTP SSE Server")
        self.widget.setMinimumWidth(600)

        # 创建布局
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 3)
        self.grid_layout.addWidget(self.debug_card, 0, 3, 1, 3)
        self.grid_layout.addWidget(self.name_card, 2, 0, 1, 6)
        self.grid_layout.addWidget(self.host_card, 3, 0, 1, 6)
        self.grid_layout.addWidget(self.port_card, 4, 0, 1, 6)
        self.grid_layout.addWidget(self.cors_card, 5, 0, 1, 2)
        self.grid_layout.addWidget(self.websocket_card, 5, 2, 1, 2)
        self.grid_layout.addWidget(self.report_self_msg_card, 5, 4, 1, 2)
        self.grid_layout.addWidget(self.msg_format_card, 6, 0, 1, 6)
        self.grid_layout.addWidget(self.token_card, 7, 0, 1, 6)

        # 填充配置数据
        self.fill_config()

    def fill_config(self) -> None:
        """填充 HTTP SSE 服务器配置数据"""
        if self.config is None:
            return

        super().fill_config()
        self.host_card.fill_value(self.config.host)
        self.port_card.fill_value(self.config.port)
        self.cors_card.fill_value(self.config.enableCors)
        self.websocket_card.fill_value(self.config.enableWebsocket)
        self.report_self_msg_card.fill_value(self.config.reportSelfMessage)

    def get_config(self) -> HttpSseServersConfig:
        """获取 HTTP SSE 服务器配置

        Returns:
            HttpSseServersConfig: HTTP SSE 服务器配置对象
        """
        return HttpSseServersConfig(
            **{
                "host": self.host_card.get_value(),
                "port": self.port_card.get_value(),
                "enableCors": self.cors_card.get_value(),
                "enableWebsocket": self.websocket_card.get_value(),
                "reportSelfMessage": self.report_self_msg_card.get_value(),
                **super().get_config().model_dump(),
            }
        )


class HttpClientConfigDialog(ConfigDialogBase):
    """HTTP 客户端配置对话框"""

    config: HttpClientsConfig | None

    def __init__(self, parent: QObject, config: HttpClientsConfig | None = None) -> None:
        """初始化 HTTP 客户端配置对话框

        Args:
            parent: 父级对象
            config: HTTP 客户端配置对象，可选
        """
        super().__init__(parent, config)

        # 创建控件
        self.report_self_msg_card = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.url_card = LineEditConfigCard(FI.LINK, "URL*", "http://localhost:8080", self.tr("设置请求地址"))

        # 设置属性
        self.title_label.setText("HTTP Client")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 2)
        self.grid_layout.addWidget(self.debug_card, 0, 2, 1, 2)
        self.grid_layout.addWidget(self.report_self_msg_card, 0, 4, 1, 2)
        self.grid_layout.addWidget(self.name_card, 1, 0, 1, 6)
        self.grid_layout.addWidget(self.url_card, 2, 0, 1, 6)
        self.grid_layout.addWidget(self.msg_format_card, 3, 0, 1, 6)
        self.grid_layout.addWidget(self.token_card, 4, 0, 1, 6)

        # 填充配置数据
        self.fill_config()

    def fill_config(self) -> None:
        """填充 HTTP 客户端配置数据"""
        if self.config is None:
            return

        super().fill_config()
        self.report_self_msg_card.fill_value(self.config.reportSelfMessage)
        self.url_card.fill_value(str(self.config.url))

    def get_config(self) -> HttpClientsConfig:
        """获取 HTTP 客户端配置

        Returns:
            HttpClientsConfig: HTTP 客户端配置对象
        """
        return HttpClientsConfig(
            **{
                "url": self.url_card.get_value(),
                "reportSelfMessage": self.report_self_msg_card.get_value(),
                **super().get_config().model_dump(),
            }
        )


class WebsocketServerConfigDialog(ConfigDialogBase):
    """WebSocket 服务器配置对话框"""

    config: WebsocketServersConfig | None

    def __init__(self, parent: QObject, config: WebsocketServersConfig | None = None) -> None:
        """初始化 WebSocket 服务器配置对话框

        Args:
            parent: 父级对象
            config: WebSocket 服务器配置对象，可选
        """
        super().__init__(parent, config)

        # 创建控件
        self.force_push_event_card = SwitchConfigCard(FI.MESSAGE, self.tr("强制推送事件"), value=True)
        self.report_self_msg_card = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.host_card = LineEditConfigCard(FI.HOME, self.tr("Host*"), "0.0.0.0", self.tr("设置主机地址"))
        self.port_card = LineEditConfigCard(FI.LINK, self.tr("Port*"), "3000", self.tr("设置端口号"))
        self.heart_interval_card = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))

        # 设置属性
        self.title_label.setText("Websocket Server")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 1)
        self.grid_layout.addWidget(self.debug_card, 0, 1, 1, 1)
        self.grid_layout.addWidget(self.force_push_event_card, 0, 2, 1, 1)
        self.grid_layout.addWidget(self.report_self_msg_card, 0, 3, 1, 1)
        self.grid_layout.addWidget(self.name_card, 2, 0, 1, 4)
        self.grid_layout.addWidget(self.host_card, 3, 0, 1, 2)
        self.grid_layout.addWidget(self.port_card, 3, 2, 1, 2)
        self.grid_layout.addWidget(self.msg_format_card, 4, 0, 1, 4)
        self.grid_layout.addWidget(self.token_card, 5, 0, 1, 2)
        self.grid_layout.addWidget(self.heart_interval_card, 5, 2, 1, 2)

        # 填充配置数据
        self.fill_config()

    def fill_config(self) -> None:
        """填充 WebSocket 服务器配置数据"""
        if self.config is None:
            return

        super().fill_config()
        self.force_push_event_card.fill_value(self.config.enableForcePushEvent)
        self.report_self_msg_card.fill_value(self.config.reportSelfMessage)
        self.host_card.fill_value(self.config.host)
        self.port_card.fill_value(self.config.port)
        self.heart_interval_card.fill_value(self.config.heartInterval)

    def get_config(self) -> WebsocketServersConfig:
        """获取 WebSocket 服务器配置

        Returns:
            WebsocketServersConfig: WebSocket 服务器配置对象
        """
        return WebsocketServersConfig(
            **{
                "host": self.host_card.get_value(),
                "port": self.port_card.get_value(),
                "reportSelfMessage": self.report_self_msg_card.get_value(),
                "enableForcePushEvent": self.force_push_event_card.get_value(),
                "heartInterval": int(self.heart_interval_card.get_value(), 30000),
                **super().get_config().model_dump(),
            }
        )


class WebsocketClientConfigDialog(ConfigDialogBase):
    """WebSocket 客户端配置对话框"""

    config: WebsocketClientsConfig | None

    def __init__(self, parent: QObject, config: WebsocketClientsConfig | None = None) -> None:
        """初始化 WebSocket 客户端配置对话框

        Args:
            parent: 父级对象
            config: WebSocket 客户端配置对象，可选
        """
        super().__init__(parent, config)

        # 创建控件
        self.report_self_msg_card = SwitchConfigCard(FI.MESSAGE, self.tr("上报自身消息"))
        self.url_card = LineEditConfigCard(FI.LINK, "URL*", "ws://localhost:8080", self.tr("设置请求地址"))
        self.heart_interval_card = LineEditConfigCard(FI.HEART, self.tr("心跳间隔"), "30000", self.tr("设置心跳间隔"))
        self.reconnect_interval_card = LineEditConfigCard(
            FI.UPDATE, self.tr("重连间隔"), "30000", self.tr("设置重连间隔")
        )

        # 设置属性
        self.title_label.setText("Websocket Client")
        self.widget.setMinimumWidth(600)

        # 设置布局
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 2)
        self.grid_layout.addWidget(self.debug_card, 0, 2, 1, 2)
        self.grid_layout.addWidget(self.report_self_msg_card, 0, 4, 1, 2)
        self.grid_layout.addWidget(self.name_card, 1, 0, 1, 6)
        self.grid_layout.addWidget(self.url_card, 2, 0, 1, 6)
        self.grid_layout.addWidget(self.msg_format_card, 3, 0, 1, 3)
        self.grid_layout.addWidget(self.token_card, 3, 3, 1, 3)
        self.grid_layout.addWidget(self.heart_interval_card, 4, 0, 1, 3)
        self.grid_layout.addWidget(self.reconnect_interval_card, 4, 3, 1, 3)

        # 填充配置数据
        self.fill_config()

    def fill_config(self) -> None:
        """填充 WebSocket 客户端配置数据"""
        if self.config is None:
            return

        super().fill_config()
        self.report_self_msg_card.fill_value(self.config.reportSelfMessage)
        self.url_card.fill_value(str(self.config.url))
        self.heart_interval_card.fill_value(self.config.heartInterval)
        self.reconnect_interval_card.fill_value(self.config.reconnectInterval)

    def get_config(self) -> WebsocketClientsConfig:
        """获取 WebSocket 客户端配置

        Returns:
            WebsocketClientsConfig: WebSocket 客户端配置对象
        """
        return WebsocketClientsConfig(
            **{
                "url": self.url_card.get_value(),
                "reportSelfMessage": self.report_self_msg_card.get_value(),
                "heartInterval": int(self.heart_interval_card.get_value(), 30000),
                "reconnectInterval": int(self.reconnect_interval_card.get_value(), 30000),
                **super().get_config().model_dump(),
            }
        )


class AutoRestartDialog(MessageBoxBase):
    """自动重启提示对话框"""

    def __init__(self, parent: QWidget) -> None:
        """初始化自动重启提示对话框

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(self.tr("自动重启 Bot"), self)
        self.enable_card = SwitchConfigCard(FI.IOT, self.tr("启用自动重启"), parent=self)
        self.interval_card = IntervalTimeConfigCard(FI.DATE_TIME, self.tr("重启间隔"), parent=self)

        # 布局
        self.grid_layout = QGridLayout()
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 4)
        self.grid_layout.addWidget(self.interval_card, 1, 0, 1, 4)
        self.grid_layout.setContentsMargins(0, 0, 0, 0)
        self.grid_layout.setSpacing(8)

        # 设置布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.grid_layout)
        self.widget.setMinimumWidth(600)

    # ==================== 公共方法 ====================
    def get_config(self) -> AutoRestartScheduleConfig:
        """获取配置"""
        interval_value = self.interval_card.get_value()
        return AutoRestartScheduleConfig(
            **{
                "enable": self.enable_card.get_value(),
                "time_unit": interval_value[0],
                "duration": interval_value[1],
            }
        )

    def fill_config(self, config: AutoRestartScheduleConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.enable_card.fill_value(config.enable)
        self.interval_card.fill_value(config.time_unit, config.duration)

    def clear_config(self) -> None:
        """清空配置"""
        self.enable_card.clear()
        self.interval_card.clear()


class QRCodeDialog(MessageBoxBase):
    """显示二维码的对话框"""

    def __init__(self, qr_code_list: list[dict[str, str]], parent: QWidget) -> None:
        """初始化二维码对话框

        Args:
            parent: 父控件
            qr_code_list: 二维码列表
        """
        super().__init__(parent=parent)
        # 设置属性
        self._qr_code_list = qr_code_list

        # 创建控件
        self.title_label = TitleLabel(self.tr("QQ 登录二维码"), self)
        self.view = TransparentStackedWidget(self)
        self.pips_pager = PipsPager(self)

        # 设置属性
        self.widget.setMinimumSize(530, 470)
        self.pips_pager.setPageNumber(len(self._qr_code_list))
        self.pips_pager.setVisibleNumber(len(self._qr_code_list))
        self.pips_pager.setNextButtonDisplayMode(PipsScrollButtonDisplayMode.ON_HOVER)
        self.pips_pager.setPreviousButtonDisplayMode(PipsScrollButtonDisplayMode.ON_HOVER)

        # 设置布局
        self.viewLayout.addWidget(self.title_label, alignment=Qt.AlignmentFlag.AlignLeft)
        self.viewLayout.addWidget(self.view, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.viewLayout.addWidget(self.pips_pager, alignment=Qt.AlignmentFlag.AlignHCenter)

        # 连接信号
        self.pips_pager.currentIndexChanged.connect(self.view.setCurrentIndex)
        self.view.currentChanged.connect(self.slot_current_index_changed)

        # 调用方法
        self.add_qr_codes()

    # ==================== 公共方法 ====================
    def update_qr_codes(self, qr_code_list: list[dict[str, str]]) -> None:
        """更新二维码列表

        Args:
            qr_code_list (list[dict[str, str]]): 二维码列表
        """
        # 判断是否有变化
        if self._qr_code_list == qr_code_list:
            return

        self._qr_code_list = qr_code_list

        # 移除 view 中的所有子控件
        while self.view.count() > 0:
            widget_to_remove = self.view.widget(0)
            self.view.removeWidget(widget_to_remove)
            widget_to_remove.deleteLater()

        # 更新分页控件
        self.pips_pager.setPageNumber(len(self._qr_code_list))
        self.pips_pager.setVisibleNumber(len(self._qr_code_list))

        # 重新添加二维码图片
        self.add_qr_codes()

    def add_qr_codes(self) -> None:
        """添加二维码图片到视图"""
        for qr_code_info in self._qr_code_list:
            self.add_qr_code(qr_code_info["qq_id"], qr_code_info["qrcode_url"])

    def add_qr_code(self, qq_id: str, qrcode_url: str) -> None:
        """添加二维码图片到视图

        Args:
            qrcode_url (str): 二维码 URL
        """
        # 生成二维码图片并编码为 PNG 格式的字节
        pil_img = qrcode.make(qrcode_url).get_image()
        buffer = BytesIO()
        pil_img.save(buffer, format="PNG")
        qr_code_image_bytes = buffer.getvalue()

        # 使用 QPixmap 加载图片数据并创建 ImageLabel
        pixmap = QPixmap()
        pixmap.loadFromData(qr_code_image_bytes)
        image_label = ImageLabel(pixmap, self.view)
        image_label.setObjectName(qq_id)

        # 调整属性
        image_label.scaledToHeight(260)

        # 添加到视图堆栈
        self.view.addWidget(image_label)

    # ==================== 槽函数 ====================
    def slot_current_index_changed(self, index: int) -> None:
        """当前索引改变槽函数

        Args:
            index (int): 当前索引
        """
        if index == -1:
            return
        self.title_label.setText(self.tr(f"QQ 登录二维码 - {self.view.widget(index).objectName()}"))


class QRCodeDialogFactory(QObject):
    """显示二维码的对话框的工厂类"""

    def __init__(self) -> None:
        """初始化二维码对话框工厂类"""
        super().__init__()
        # 创建控件
        self.dialog = None

        # 二维码列表, 存储二维码的相关信息, 字典结构为{"qq_id": str, "qrcode_url": str}
        self.qr_code_list: list[dict[str, str]] = []

    # ==================== 公共方法 ====================
    def add_qr_code(self, qq_id: str, qrcode_url: str) -> None:
        """添加二维码信息到列表

        Args:
            qq_id (str): QQ 号
            qrcode_url (str): 二维码 URL
        """
        # 如果列表中已存在相同 QQ 号的二维码信息, 则更新其 URL
        for qr_code_info in self.qr_code_list:
            if qr_code_info["qq_id"] == qq_id:
                qr_code_info["qrcode_url"] = qrcode_url
                break
        else:
            self.qr_code_list.append({"qq_id": qq_id, "qrcode_url": qrcode_url})

        self.show()

    def remove_qr_code(self, qq_id: str) -> None:
        """从列表中移除二维码信息

        Args:
            qq_id (str): QQ 号
        """
        self.qr_code_list = [info for info in self.qr_code_list if info["qq_id"] != qq_id]

        # 如果没有二维码信息了, 则关闭对话框
        if not self.qr_code_list and self.dialog is not None:
            self.dialog.done(MessageBoxBase.DialogCode.Rejected)
            self.dialog = None
        elif self.dialog is not None:
            self.dialog.update_qr_codes(self.qr_code_list)

    def show(self) -> None:
        """显示二维码对话框"""
        # 判断二维码列表是否为空
        if not self.qr_code_list:
            return

        # 判断是否已经exec()过, 如果是则更新内容
        if self.dialog is not None:
            self.dialog.update_qr_codes(self.qr_code_list)

        else:
            from src.ui.window.main_window import MainWindow

            self.dialog = QRCodeDialog(self.qr_code_list, it(MainWindow))

            # 清理对话框引用
            if (
                code := self.dialog.exec()
            ) == MessageBoxBase.DialogCode.Accepted or code == MessageBoxBase.DialogCode.Rejected:
                self.dialog = None


class QRCodeDialogFactoryCreator(AbstractCreator, ABC):
    """QRCodeDialogFactory 的创建器"""

    targets = (CreateTargetInfo("src.ui.page.bot_page.widget.msg_box", "QRCodeDialogFactory"),)

    @staticmethod
    def available() -> bool:
        """检查是否可用"""
        return exists_module("src.ui.page.bot_page.widget.msg_box")

    @classmethod
    def create(cls, create_type: type[QRCodeDialogFactory]) -> QRCodeDialogFactory:
        """创建 QRCodeDialogFactory 实例"""
        return create_type()


add_creator(QRCodeDialogFactoryCreator)
