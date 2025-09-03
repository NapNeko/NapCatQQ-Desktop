# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BodyLabel
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import (
    PushButton,
    TeachingTip,
    PillPushButton,
    TeachingTipView,
    HeaderCardWidget,
    TransparentToolButton,
    TeachingTipTailPosition,
)
from PySide6.QtGui import QFont
from PySide6.QtCore import Qt, QObject
from PySide6.QtWidgets import QWidget, QGridLayout

# 项目内模块导入
from src.ui.page.add_page.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.core.config.config_model import (
    BaseModel,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)


class EnableTag(PillPushButton):
    """显示是否启用的标签"""

    def __init__(self, status: bool, parent: QObject) -> None:
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 8))
        self.update_status(status)

    def update_status(self, status: bool) -> None:
        """更新状态"""
        if status:
            self.setText(self.tr("启用"))
        else:
            self.setText(self.tr("禁用"))


class FormateTag(PillPushButton):
    """消息格式标签"""

    def __init__(self, format: str, parent: QObject) -> None:
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 8))
        self.update_format(format)

    def update_format(self, format: str) -> None:
        """更新格式"""
        self.setText(format)


class ConfigCardBase(HeaderCardWidget):

    def __init__(self, config: BaseModel, parent: QObject) -> None:
        super().__init__(parent)
        # 属性
        self.config = config

        # 创建控件
        self.removeButton = TransparentToolButton(FI.DELETE, self)
        self.editButton = TransparentToolButton(FI.EDIT, self)
        self.configView = QWidget(self)

        # 设置属性
        self.setTitle(self.config.name)
        self.setFixedSize(335, 170)

        # 设置布局
        self.headerLayout.addStretch(1)
        self.headerLayout.addWidget(self.removeButton)
        self.headerLayout.addWidget(self.editButton)
        self.viewLayout.addWidget(self.configView)
        self.viewLayout.setContentsMargins(16, 16, 16, 16)

        self.configViewLayout = QGridLayout(self.configView)
        self.configViewLayout.setContentsMargins(0, 0, 0, 0)
        self.configViewLayout.setVerticalSpacing(10)

        # 信号连接
        self.removeButton.clicked.connect(self._onRemoveButtonClicked)
        self.editButton.clicked.connect(self._onEditButtonClicked)

    def _onRemoveButtonClicked(self) -> None:
        """删除按钮被点击"""
        # 项目内模块导入
        from src.ui.page.add_page.signal_bus import addPageSingalBus

        view = TeachingTipView(
            title=self.tr("删除配置"),
            content=self.tr("确定要删除该配置吗?这个操作不可逆!"),
            isClosable=False,
            tailPosition=TeachingTipTailPosition.TOP,
        )
        button = PushButton(self.tr("删除"), self)

        view.addWidget(button, align=Qt.AlignmentFlag.AlignRight)

        widget = TeachingTip.make(
            target=self.removeButton,
            view=view,
            duration=2000,
            tailPosition=TeachingTipTailPosition.BOTTOM,
            parent=self,
        )
        view.closed.connect(widget.close)
        button.clicked.connect(lambda: addPageSingalBus.removeCard.emit(self))
        button.clicked.connect(widget.close)

    def _onEditButtonClicked(self) -> None:
        """编辑按钮点击事件"""
        ...


class HttpServerConfigCard(ConfigCardBase):
    config: HttpServersConfig

    def __init__(self, config: HttpServersConfig, parent: QObject):
        super().__init__(config, parent)

        # 创建控件
        self.hostLabel = BodyLabel(self.tr("主机"), self)
        self.hostConfigLabel = BodyLabel(self.config.host, self)

        self.portLabel = BodyLabel(self.tr("端口"), self)
        self.portConfigLabel = BodyLabel(str(self.config.port), self)

        self.corsLabel = BodyLabel(self.tr("CORS"), self)
        self.corsConfigLabel = EnableTag(self.config.enableCors, self)

        self.websocketLabel = BodyLabel(self.tr("WS"), self)
        self.websocketConfigLabel = EnableTag(self.config.enableWebsocket, self)

        self.msgPostFormatLabel = BodyLabel(self.tr("格式"), self)
        self.msgPostFormatConfigLabel = FormateTag(self.config.messagePostFormat, self)

        # 布局
        self.configViewLayout.addWidget(self.hostLabel, 0, 0, 1, 1)
        self.configViewLayout.addWidget(self.hostConfigLabel, 0, 1, 1, 2)

        self.configViewLayout.addWidget(self.portLabel, 0, 3, 1, 1)
        self.configViewLayout.addWidget(self.portConfigLabel, 0, 4, 1, 2)

        self.configViewLayout.addWidget(self.corsLabel, 1, 0, 1, 1)
        self.configViewLayout.addWidget(self.corsConfigLabel, 1, 1, 1, 2)

        self.configViewLayout.addWidget(self.websocketLabel, 1, 3, 1, 1)
        self.configViewLayout.addWidget(self.websocketConfigLabel, 1, 4, 1, 2)

        self.configViewLayout.addWidget(self.msgPostFormatLabel, 2, 0, 1, 1)
        self.configViewLayout.addWidget(self.msgPostFormatConfigLabel, 2, 1, 1, 5)

    def fillValue(self) -> None:
        """填充需要展示的配置"""
        self.hostConfigLabel.setText(self.config.host)
        self.portConfigLabel.setText(str(self.config.port))
        self.corsConfigLabel.update_status(self.config.enableCors)
        self.websocketConfigLabel.update_status(self.config.enableWebsocket)
        self.msgPostFormatConfigLabel.update_format(self.config.messagePostFormat)

    def getValue(self) -> HttpServersConfig:
        return self.config

    def _onEditButtonClicked(self) -> None:

        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if (dialog := HttpServerConfigDialog(MainWindow(), self.config)).exec():
            self.config = dialog.getConfig()
            self.fillValue()


class HttpSSEConfigCard(ConfigCardBase):
    config: HttpSseServersConfig

    def __init__(self, config: HttpSseServersConfig, parent: QObject):
        super().__init__(config, parent)

        # 创建控件
        self.hostLabel = BodyLabel(self.tr("主机"), self)
        self.hostConfigLabel = BodyLabel(self.config.host, self)

        self.portLabel = BodyLabel(self.tr("端口"), self)
        self.portConfigLabel = BodyLabel(str(self.config.port), self)

        self.corsLabel = BodyLabel(self.tr("CORS"), self)
        self.corsConfigLabel = EnableTag(self.config.enableCors, self)

        self.websocketLabel = BodyLabel(self.tr("WS"), self)
        self.websocketConfigLabel = EnableTag(self.config.enableWebsocket, self)

        self.msgPostFormatLabel = BodyLabel(self.tr("格式"), self)
        self.msgPostFormatConfigLabel = FormateTag(self.config.messagePostFormat, self)

        self.reportSelfMessageLabel = BodyLabel(self.tr("上报自身消息"), self)
        self.reportSelfMessageConfigLabel = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.configViewLayout.addWidget(self.hostLabel, 0, 0, 1, 1)
        self.configViewLayout.addWidget(self.hostConfigLabel, 0, 1, 1, 2)

        self.configViewLayout.addWidget(self.portLabel, 0, 3, 1, 1)
        self.configViewLayout.addWidget(self.portConfigLabel, 0, 4, 1, 2)

        self.configViewLayout.addWidget(self.corsLabel, 1, 0, 1, 1)
        self.configViewLayout.addWidget(self.corsConfigLabel, 1, 1, 1, 2)

        self.configViewLayout.addWidget(self.websocketLabel, 1, 3, 1, 1)
        self.configViewLayout.addWidget(self.websocketConfigLabel, 1, 4, 1, 2)

        self.configViewLayout.addWidget(self.msgPostFormatLabel, 2, 0, 1, 1)
        self.configViewLayout.addWidget(self.msgPostFormatConfigLabel, 2, 1, 1, 2)

        self.configViewLayout.addWidget(self.reportSelfMessageLabel, 2, 3, 1, 1)
        self.configViewLayout.addWidget(self.reportSelfMessageConfigLabel, 2, 4, 1, 2)

    def fillValue(self) -> None:
        """填充需要展示的配置"""
        self.hostConfigLabel.setText(self.config.host)
        self.portConfigLabel.setText(str(self.config.port))
        self.corsConfigLabel.update_status(self.config.enableCors)
        self.websocketConfigLabel.update_status(self.config.enableWebsocket)
        self.msgPostFormatConfigLabel.update_format(self.config.messagePostFormat)
        self.reportSelfMessageConfigLabel.update_status(self.config.reportSelfMessage)

    def getValue(self) -> HttpSseServersConfig:
        return self.config

    def _onEditButtonClicked(self) -> None:

        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if (dialog := HttpSSEServerConfigDialog(MainWindow(), self.config)).exec():
            self.config = dialog.getConfig()
            self.fillValue()


class HttpClientConfigCard(ConfigCardBase):
    config: HttpClientsConfig

    def __init__(self, config: HttpClientsConfig, parent: QObject):
        super().__init__(config, parent)

        # 创建控件
        urlLabel = BodyLabel(self.tr("URL"), self)
        urlConfigLabel = BodyLabel(str(self.config.url), self)

        formatLabel = BodyLabel(self.tr("格式"), self)
        formatConfigLabel = FormateTag(self.config.messagePostFormat, self)

        reportSelfMessageLabel = BodyLabel(self.tr("上报自身消息"), self)
        reportSelfMessageConfigLabel = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.configViewLayout.addWidget(urlLabel, 0, 0, 1, 1)
        self.configViewLayout.addWidget(urlConfigLabel, 0, 1, 1, 6)

        self.configViewLayout.addWidget(formatLabel, 1, 0, 1, 1)
        self.configViewLayout.addWidget(formatConfigLabel, 1, 1, 1, 1)

        self.configViewLayout.addWidget(reportSelfMessageLabel, 1, 4, 1, 1)
        self.configViewLayout.addWidget(reportSelfMessageConfigLabel, 1, 5, 1, 1)

    def fillValue(self) -> None:
        """填充需要展示的配置"""
        self.urlConfigLabel.setText(str(self.config.url))
        self.formatConfigLabel.update_format(self.config.messagePostFormat)
        self.reportSelfMessageConfigLabel.update_status(self.config.reportSelfMessage)

    def getValue(self) -> HttpClientsConfig:
        return self.config

    def _onEditButtonClicked(self) -> None:

        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if (dialog := HttpClientConfigDialog(MainWindow(), self.config)).exec():
            self.config = dialog.getConfig()
            self.fillValue()


class WebsocketServersConfigCard(ConfigCardBase):
    config: WebsocketServersConfig

    def __init__(self, config: WebsocketServersConfig, parent: QObject):
        super().__init__(config, parent)

        # 创建控件
        self.hostLabel = BodyLabel(self.tr("主机"), self)
        self.hostConfigLabel = BodyLabel(self.config.host, self)

        self.portLabel = BodyLabel(self.tr("端口"), self)
        self.portConfigLabel = BodyLabel(str(self.config.port), self)

        self.heartIntervalLabel = BodyLabel(self.tr("心跳间隔"), self)
        self.heartIntervalConfigLabel = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.msgPostFormatLabel = BodyLabel(self.tr("格式"), self)
        self.msgPostFormatConfigLabel = FormateTag(self.config.messagePostFormat, self)

        self.reportSelfMessageLabel = BodyLabel(self.tr("上报自身消息"), self)
        self.reportSelfMessageConfigLabel = EnableTag(self.config.reportSelfMessage, self)

        self.enableForcePushEventLabel = BodyLabel(self.tr("强制推送事件"), self)
        self.enableForcePushEventConfigLabel = EnableTag(self.config.enableForcePushEvent, self)

        # 布局
        self.configViewLayout.addWidget(self.hostLabel, 0, 0, 1, 1)
        self.configViewLayout.addWidget(self.hostConfigLabel, 0, 1, 1, 2)

        self.configViewLayout.addWidget(self.portLabel, 0, 3, 1, 1)
        self.configViewLayout.addWidget(self.portConfigLabel, 0, 4, 1, 2)

        self.configViewLayout.addWidget(self.heartIntervalLabel, 1, 0, 1, 1)
        self.configViewLayout.addWidget(self.heartIntervalConfigLabel, 1, 1, 1, 2)

        self.configViewLayout.addWidget(self.msgPostFormatLabel, 1, 3, 1, 1)
        self.configViewLayout.addWidget(self.msgPostFormatConfigLabel, 1, 4, 1, 2)

        self.configViewLayout.addWidget(self.reportSelfMessageLabel, 2, 0, 1, 1)
        self.configViewLayout.addWidget(self.reportSelfMessageConfigLabel, 2, 1, 1, 2)

        self.configViewLayout.addWidget(self.enableForcePushEventLabel, 2, 3, 1, 1)
        self.configViewLayout.addWidget(self.enableForcePushEventConfigLabel, 2, 4, 1, 2)

    def fillValue(self) -> None:
        """填充需要展示的配置"""
        self.hostConfigLabel.setText(self.config.host)
        self.portConfigLabel.setText(str(self.config.port))
        self.heartIntervalConfigLabel.setText(str(self.config.heartInterval) + "ms")
        self.msgPostFormatConfigLabel.update_format(self.config.messagePostFormat)
        self.reportSelfMessageConfigLabel.update_status(self.config.reportSelfMessage)
        self.enableForcePushEventConfigLabel.update_status(self.config.enableForcePushEvent)

    def getValue(self) -> WebsocketServersConfig:
        return self.config

    def _onEditButtonClicked(self) -> None:

        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if (dialog := WebsocketServerConfigDialog(MainWindow(), self.config)).exec():
            self.config = dialog.getConfig()
            self.fillValue()


class WebsocketClientConfigCard(ConfigCardBase):
    config: WebsocketClientsConfig

    def __init__(self, config: WebsocketClientsConfig, parent: QObject):
        super().__init__(config, parent)

        # 创建控件
        self.urlLabel = BodyLabel(self.tr("URL"), self)
        self.urlConfigLabel = BodyLabel(str(self.config.url), self)

        self.reconnectIntervalLabel = BodyLabel(self.tr("重连间隔"), self)
        self.reconnectIntervalConfigLabel = BodyLabel(str(self.config.reconnectInterval) + "ms", self)

        self.heartIntervalLabel = BodyLabel(self.tr("心跳间隔"), self)
        self.heartIntervalConfigLabel = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.formatLabel = BodyLabel(self.tr("格式"), self)
        self.formatConfigLabel = FormateTag(self.config.messagePostFormat, self)

        self.reportSelfMessageLabel = BodyLabel(self.tr("上报自身消息"), self)
        self.reportSelfMessageConfigLabel = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.configViewLayout.addWidget(self.urlLabel, 0, 0, 1, 1)
        self.configViewLayout.addWidget(self.urlConfigLabel, 0, 1, 1, 6)

        self.configViewLayout.addWidget(self.reconnectIntervalLabel, 1, 0, 1, 1)
        self.configViewLayout.addWidget(self.reconnectIntervalConfigLabel, 1, 1, 1, 2)

        self.configViewLayout.addWidget(self.heartIntervalLabel, 1, 3, 1, 1)
        self.configViewLayout.addWidget(self.heartIntervalConfigLabel, 1, 4, 1, 2)

        self.configViewLayout.addWidget(self.formatLabel, 2, 0, 1, 1)
        self.configViewLayout.addWidget(self.formatConfigLabel, 2, 1, 1, 2)

        self.configViewLayout.addWidget(self.reportSelfMessageLabel, 2, 3, 1, 1)
        self.configViewLayout.addWidget(self.reportSelfMessageConfigLabel, 2, 4, 1, 2)

    def fillValue(self) -> None:
        """填充需要展示的配置"""
        self.urlConfigLabel.setText(str(self.config.url))
        self.reconnectIntervalConfigLabel.setText(str(self.config.reconnectInterval) + "ms")
        self.heartIntervalConfigLabel.setText(str(self.config.heartInterval) + "ms")
        self.formatConfigLabel.update_format(self.config.messagePostFormat)
        self.reportSelfMessageConfigLabel.update_status(self.config.reportSelfMessage)

    def getValue(self) -> WebsocketClientsConfig:
        return self.config

    def _onEditButtonClicked(self) -> None:

        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if (dialog := WebsocketClientConfigDialog(MainWindow(), self.config)).exec():
            self.config = dialog.getConfig()
            self.fillValue()
