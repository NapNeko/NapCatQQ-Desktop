# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout
from qfluentwidgets.common import FluentIconBase, FluentIcon
from qfluentwidgets.components import (
    SwitchButton, BodyLabel, ComboBox, LineEdit, IndicatorPosition
)
from qfluentwidgets.components.settings import (
    SettingCard, ExpandGroupSettingCard
)


class LineEditConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str,
            placeholder_text="", content=None, parent=None
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setClearButtonEnabled(True)
        self.lineEdit.setPlaceholderText(placeholder_text)

        self.hBoxLayout.addWidget(self.lineEdit, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> str:
        return self.lineEdit.text()


class ComboBoxConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str,
            texts=None, content=None, parent=None
    ):
        super().__init__(icon, title, content, parent)
        self.texts = texts or []
        self.comboBox = ComboBox(self)
        self.comboBox.addItems(self.texts)
        self.hBoxLayout.addWidget(self.comboBox, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> str:
        return self.comboBox.currentText()


class SwitchConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str, 
            content=None, parent=None
    ):
        super().__init__(icon, title, content, parent)
        self.swichButton = SwitchButton(self, IndicatorPosition.RIGHT)

        self.hBoxLayout.addWidget(self.swichButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self):
        return self.swichButton.isChecked()


class GroupCardBase(ExpandGroupSettingCard):

    def __init__(self, icon: FluentIconBase, title, content, parent=None):
        super().__init__(icon, title, content, parent)

    def add(self, label, widget):
        view = QWidget()

        layout = QHBoxLayout(view)
        layout.setContentsMargins(48, 12, 48, 12)

        layout.addWidget(label)
        layout.addStretch(1)
        layout.addWidget(widget)

        # 添加组件到设置卡
        self.addGroupWidget(view)


class HttpConfigCard(GroupCardBase):

    def __init__(self, parent=None):
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP service"),
            content=self.tr("Configure HTTP related services"),
            parent=parent
        )
        # http服务开关
        self.httpServiceLabel = BodyLabel(self.tr("Enable HTTP service"))
        self.httpServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http服务端口
        self.httpPortLabel = BodyLabel(self.tr("HTTP service port"))
        self.httpPortLineEidt = LineEdit()
        self.httpPortLineEidt.setPlaceholderText("8080")

        # 添加到设置卡
        self.add(self.httpServiceLabel, self.httpServiceButton)
        self.add(self.httpPortLabel, self.httpPortLineEidt)


class HttpReportConfigCard(GroupCardBase):

    def __init__(self, parent=None):
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP report"),
            content=self.tr("Configure HTTP reporting related services"),
            parent=parent
        )

        # http 上报服务开关
        self.httpRpLabel = BodyLabel(self.tr("Enable HTTP reporting service"))
        self.httpRpButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http 上报心跳开关
        self.httpRpHeartLabel = BodyLabel(self.tr("Enable HTTP reporting heart"))
        self.httpRpHeartButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http 上报 token
        self.httpRpTokenLabel = BodyLabel(self.tr("Set HTTP reporting token"))
        self.httpRpTokenButton = LineEdit()
        self.httpRpTokenButton.setPlaceholderText(self.tr("Optional filling"))

        # http上报 ip
        self.httpRpIpLabel = BodyLabel(self.tr("Set HTTP reporting IP"))
        self.httpRpIpLineEdit = LineEdit()
        self.httpRpIpLineEdit.setPlaceholderText("127.0.0.1")

        # http 上报 端口
        self.httpRpPortLabel = BodyLabel(self.tr("Set HTTP reporting port"))
        self.httpRpPortLineEdit = LineEdit()
        self.httpRpPortLineEdit.setPlaceholderText("8080")

        # http 上报 地址
        self.httpRpPathLabel = BodyLabel(self.tr("Set HTTP reporting path"))
        self.httpRpPathLineEdit = LineEdit()
        self.httpRpPathLineEdit.setPlaceholderText("/onebot/v11/http")

        # 添加到设置卡
        self.add(self.httpRpLabel, self.httpRpButton)
        self.add(self.httpRpHeartLabel, self.httpRpHeartButton)
        self.add(self.httpRpTokenLabel, self.httpRpTokenButton)
        self.add(self.httpRpIpLabel, self.httpRpIpLineEdit)
        self.add(self.httpRpPortLabel, self.httpRpPortLineEdit)
        self.add(self.httpRpPathLabel, self.httpRpPathLineEdit)


class WsConfigCard(GroupCardBase):

    def __init__(self, parent=None):
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocket service"),
            content=self.tr("Configure WebSocket service"),
            parent=parent
        )

        # 正向 Ws 服务开关
        self.wsServiceLabel = BodyLabel(self.tr("Enable WebSocket service"))
        self.wsServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # 正向 Ws 服务 端口
        self.wsPortLabel = BodyLabel(self.tr("Set WebSocket Port"))
        self.wsPortLineEdit = LineEdit()
        self.wsPortLineEdit.setPlaceholderText("3001")

        # 添加到设置卡
        self.add(self.wsServiceLabel, self.wsServiceButton)
        self.add(self.wsPortLabel, self.wsPortLineEdit)


class WsReverseConfigCard(GroupCardBase):

    def __init__(self, parent=None):
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocketReverse service"),
            content=self.tr("Configure WebSocketReverse service"),
            parent=parent
        )

        # 反向 Ws 开关
        self.wsReServiceLable = BodyLabel(
            self.tr("Enable WebSocketReverse service")
        )
        self.wsReServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # 反向 Ws Ip
        self.wsReIpLabel = BodyLabel(self.tr("Set WebSocketReverse Ip"))
        self.wsReIpLineEdit = LineEdit()
        self.wsReIpLineEdit.setPlaceholderText("127.0.0.1")

        # 反向 Ws 端口
        self.wsRePortLable = BodyLabel(self.tr("Set WebSocketReverse port"))
        self.wsRePortButton = LineEdit()
        self.wsRePortButton.setPlaceholderText("8080")

        # 反向 Ws 地址
        self.wsRePathLable = BodyLabel(self.tr("Set WebSocketReverse path"))
        self.wsRePathButton = LineEdit()
        self.wsRePathButton.setPlaceholderText("/onebot/v11/ws")

        # 添加到设置卡
        self.add(self.wsReServiceLable, self.wsReServiceButton)
        self.add(self.wsReIpLabel, self.wsReIpLineEdit)
        self.add(self.wsRePortLable, self.wsRePortButton)
        self.add(self.wsRePathLable, self.wsRePathButton)

