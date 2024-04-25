# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt, Signal, QStandardPaths, QEasingCurve
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout, QFileDialog
from creart import it
from qfluentwidgets.common import FluentIconBase, FluentIcon, Action
from qfluentwidgets.components import (
    PushButton, PrimarySplitPushButton, SwitchButton, BodyLabel,
    ComboBox, LineEdit, IndicatorPosition, TitleLabel, CaptionLabel,
    RoundMenu, MessageBox
)
from qfluentwidgets.components.settings import (
    SettingCard, ExpandGroupSettingCard
)

from src.Core.PathFunc import PathFunc

if TYPE_CHECKING:
    from src.Ui.AddPage.Add import AddWidget


class ConfigTopCard(QWidget):
    createPsScript = Signal(bool)
    createBatScript = Signal(bool)

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        self.titleLabel = TitleLabel(self.tr("Add bot"))
        self.subtitleLabel = CaptionLabel(
            self.tr("Before adding a robot, you need to do some configuration")
        )
        self.clearConfigButton = PushButton(
            icon=FluentIcon.DELETE,
            text=self.tr("Clear config")
        )
        self.psPushButton = PrimarySplitPushButton(
            icon=FluentIcon.ADD,
            text=self.tr("Add to bot list")
        )
        self.menu = RoundMenu()
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .ps1 script"),
                triggered=self.createPsScript.emit
            )
        )
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .bat script"),
                triggered=self.createBatScript.emit
            )
        )
        self.psPushButton.setFlyout(self.menu)

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        self.__setLayout()
        self.__connectSignal()

    def __setLayout(self) -> None:
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(1)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.clearConfigButton),
        self.buttonLayout.addSpacing(2)
        self.buttonLayout.addWidget(self.psPushButton)
        self.buttonLayout.setAlignment(
            Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft
        )

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.addSpacing(20)

        self.setLayout(self.hBoxLayout)

    def __connectSignal(self) -> None:
        self.clearConfigButton.clicked.connect(self.clearBtnSlot)

    def clearBtnSlot(self) -> None:
        from src.Ui.AddPage.Add import AddWidget
        msg = MessageBox(
            title=self.tr("Confirm clearing configuration"),
            content=self.tr(
                "After clearing, all configuration items on this page "
                "will be cleared, and this operation cannot be undone"
            ),
            parent=it(AddWidget)
        )

        if msg.exec():
            for card in it(AddWidget).cardList:
                card.clear()


class LineEditConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str, required=True,
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

    def clear(self) -> None:
        self.lineEdit.clear()


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

    def clear(self) -> None:
        self.comboBox.setCurrentIndex(0)


class SwitchConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str,
            content=None, parent=None
    ):
        super().__init__(icon, title, content, parent)
        self.swichButton = SwitchButton(self, IndicatorPosition.RIGHT)

        self.hBoxLayout.addWidget(self.swichButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> bool:
        return self.swichButton.isChecked()

    def clear(self) -> None:
        self.swichButton.setChecked(False)


class FolderConfigCard(SettingCard):

    def __init__(
            self, icon: FluentIconBase, title: str,
            content=None, parent=None
    ):
        super().__init__(icon, title, content, parent)
        self.default = content
        self.chooseFolderButton = PushButton(
            icon=FluentIcon.FOLDER,
            text=self.tr("Choose Folder")
        )
        self.chooseFolderButton.clicked.connect(self.chooseFolder)

        self.hBoxLayout.addWidget(
            self.chooseFolderButton, 0, Qt.AlignmentFlag.AlignRight
        )
        self.hBoxLayout.addSpacing(16)

    def chooseFolder(self):
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Choose folder"),
            dir=QStandardPaths.writableLocation(
                QStandardPaths.StandardLocation.DesktopLocation
            )
        )
        if folder:
            self.contentLabel.setText(folder)

    def getValue(self) -> str:
        return self.contentLabel.text()

    def clear(self) -> None:
        self.contentLabel.setText(self.default)


class GroupCardBase(ExpandGroupSettingCard):

    def __init__(self, icon: FluentIconBase, title, content, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.expandAni.setDuration(100)
        self.expandAni.setEasingCurve(QEasingCurve.Type.InQuint)

    def add(self, label, widget) -> None:
        view = QWidget()

        layout = QHBoxLayout(view)
        layout.setContentsMargins(48, 12, 48, 12)

        layout.addWidget(label)
        layout.addStretch(1)
        layout.addWidget(widget)

        # 添加组件到设置卡
        self.addGroupWidget(view)

    def wheelEvent(self, event):
        # 不知道为什么ExpandGroupSettingCard把wheelEvent屏蔽了
        # 检查是否在展开状态下，如果是，则传递滚轮事件给父级窗体
        if self.isExpand:
            self.parent().wheelEvent(event)
        else:
            super().wheelEvent(event)

    def setExpand(self, isExpand: bool):
        """ 重写方法优化下性能 """
        if self.isExpand == isExpand:
            return

        self.isExpand = isExpand
        self.setProperty('isExpand', isExpand)
        self.setUpdatesEnabled(False)

        if isExpand:
            h = self.viewLayout.sizeHint().height()
            self.verticalScrollBar().setValue(h)
            self.expandAni.setStartValue(h)
            self.expandAni.setEndValue(0)
        else:
            self.expandAni.setStartValue(0)
            self.expandAni.setEndValue(self.verticalScrollBar().maximum())

        self.expandAni.start()

    def _onExpandValueChanged(self):
        vh = self.viewLayout.sizeHint().height()
        h = self.viewportMargins().top()
        self.setFixedHeight(max(h + vh - self.verticalScrollBar().value(), h))
        self.setUpdatesEnabled(True)


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

    def clear(self):
        self.httpServiceButton.setChecked(False)
        self.httpPortLineEidt.clear()


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
        self.httpRpTokenLineEdit = LineEdit()
        self.httpRpTokenLineEdit.setPlaceholderText(self.tr("Optional filling"))

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
        self.add(self.httpRpTokenLabel, self.httpRpTokenLineEdit)
        self.add(self.httpRpIpLabel, self.httpRpIpLineEdit)
        self.add(self.httpRpPortLabel, self.httpRpPortLineEdit)
        self.add(self.httpRpPathLabel, self.httpRpPathLineEdit)

    def clear(self):
        self.httpRpButton.setChecked(False)
        self.httpRpHeartButton.setChecked(False)
        self.httpRpTokenLineEdit.clear()
        self.httpRpIpLineEdit.clear()
        self.httpRpPortLineEdit.clear()
        self.httpRpPathLineEdit.clear()


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

    def clear(self):
        self.wsServiceButton.setChecked(False),
        self.wsPortLineEdit.clear()


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
        self.wsRePortLineEdit = LineEdit()
        self.wsRePortLineEdit.setPlaceholderText("8080")

        # 反向 Ws 地址
        self.wsRePathLable = BodyLabel(self.tr("Set WebSocketReverse path"))
        self.wsRePathLineEidt = LineEdit()
        self.wsRePathLineEidt.setPlaceholderText("/onebot/v11/ws")

        # 添加到设置卡
        self.add(self.wsReServiceLable, self.wsReServiceButton)
        self.add(self.wsReIpLabel, self.wsReIpLineEdit)
        self.add(self.wsRePortLable, self.wsRePortLineEdit)
        self.add(self.wsRePathLable, self.wsRePathLineEidt)

    def clear(self):
        self.wsReServiceButton.setChecked(False)
        self.wsReIpLineEdit.clear()
        self.wsRePortLineEdit.clear()
        self.wsReIpLineEdit.clear()
