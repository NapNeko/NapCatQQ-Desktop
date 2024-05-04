# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets.common import Action, FluentIcon
from qfluentwidgets.components import (
    CaptionLabel,
    MessageBox,
    PrimarySplitPushButton,
    PushButton,
    RoundMenu,
    TitleLabel,
    SegmentedWidget,
)

from src.Core.CreateScript import ScriptType

if TYPE_CHECKING:
    from src.Core import CreateScript
    from src.Ui.AddPage.Add import AddWidget


class ConfigTopCard(QWidget):
    """
    ## AddWidget 顶部展示的 Card

    用于展示切换 view 的 SegmentedWidget
    包括清空配置项的按钮, 添加到列表的按钮, 创建脚本的按钮
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        # 创建所需控件
        self.pivot = SegmentedWidget()
        self._createLabel()
        self._createButton()
        self._createMenu()

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._setLayout()
        self._connectSignal()

    def _createLabel(self) -> None:
        # 构建 Label 并配置
        self.titleLabel = TitleLabel(self.tr("Add bot"))
        self.subtitleLabel = CaptionLabel(self.tr("Before adding a robot, you need to do some configuration"))

    def _createButton(self) -> None:
        # 构建 Button 并配置
        self.clearConfigButton = PushButton(icon=FluentIcon.DELETE, text=self.tr("Clear config"))
        self.psPushButton = PrimarySplitPushButton(icon=FluentIcon.ADD, text=self.tr("Add to bot list"))

    def _createMenu(self) -> None:
        # 构建 Menu 并配置
        self.menu = RoundMenu(self)

        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .ps1 script"),
                triggered=self._createPs1ScriptSlot,
            )
        )
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .bat script"),
                triggered=self._createBatScriptSlot,
            )
        )
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .sh script"),
                triggered=self._createShScriptSlot,
            )
        )

        self.psPushButton.setFlyout(self.menu)

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """

        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)
        self.labelLayout.addSpacing(4)
        self.labelLayout.addWidget(self.pivot)

        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.clearConfigButton),
        self.buttonLayout.addSpacing(2)
        self.buttonLayout.addWidget(self.psPushButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(1, 0, 1, 5)

        self.setLayout(self.hBoxLayout)

    def _connectSignal(self) -> None:
        """
        ## 链接所需信号
        """
        self.clearConfigButton.clicked.connect(self.clearBtnSlot)

    def _initCreateScript(self, scriptType) -> "CreateScript":
        """
        ## 初始化 CreateScript 类并返回

        ### 参数
            - scriptType 传入需要创建的脚本类型
        """
        from src.Core.CreateScript import CreateScript
        from src.Ui.AddPage.Add import AddWidget

        return CreateScript(
            config=it(AddWidget).getConfig(),
            scriptType=scriptType,
            infoBarParent=self.parent().parent(),
        )

    def _createPs1ScriptSlot(self) -> None:
        # 创建 ps1 脚本
        create = self._initCreateScript(ScriptType.PS1)
        create.createPs1Script()

    def _createBatScriptSlot(self) -> None:
        # 创建 bat 脚本
        create = self._initCreateScript(ScriptType.BAT)
        create.createBatScript()

    def _createShScriptSlot(self) -> None:
        # 创建 sh 脚本
        create = self._initCreateScript(ScriptType.SH)
        create.createShScript()

    def clearBtnSlot(self) -> None:
        """
        ## 清理按钮的槽函数

        用于提示用户是否确认清空(还原)所有已有配置项
        """
        from src.Ui.AddPage.Add import AddWidget

        box = MessageBox(
            title=self.tr("Confirm clearing configuration"),
            content=self.tr(
                "After clearing, all configuration items on this page "
                "will be cleared, and this operation cannot be undone"
            ),
            parent=it(AddWidget),
        )

        if box.exec():
            it(AddWidget).botWidget.clearValues()
            it(AddWidget).connectWidget.clearValues()
            it(AddWidget).advancedWidget.clearValues()
