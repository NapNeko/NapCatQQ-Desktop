# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from creart import it
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    MessageBox,
    PushButton,
    TitleLabel,
    CaptionLabel,
    SegmentedWidget,
    PrimaryPushButton,
)

from src.Ui.common.info_bar import error_bar, success_bar
from src.Core.Config.OperateConfig import update_config, check_duplicate_bot

if TYPE_CHECKING:
    from src.Ui.AddPage import AddWidget


class AddTopCard(QWidget):
    """
    ## AddWidget 顶部展示的信息, 操作面板
        用于展示切换 view 的 SegmentedWidget
        包括清空配置项的按钮, 添加到列表的按钮, 创建脚本的按钮

    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._createLabel()
        self._createButton()
        self._setLayout()
        self._connectSignal()

    def _createLabel(self) -> None:
        """构建 Label 并配置"""
        self.titleLabel = TitleLabel(self.tr("Add bot"), self)
        self.subtitleLabel = CaptionLabel(self.tr("Before adding a robot, you need to do some configuration"), self)

    def _createButton(self) -> None:
        """构建 Button 并配置"""
        self.clearConfigButton = PushButton(icon=FluentIcon.DELETE, text=self.tr("Clear config"))
        self.psPushButton = PrimaryPushButton(icon=FluentIcon.ADD, text=self.tr("Add to bot list"))

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        # 对 Label 区域进行布局
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)
        self.labelLayout.addSpacing(4)
        self.labelLayout.addWidget(self.pivot)

        # 对 Button 区域进行布局
        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.clearConfigButton),
        self.buttonLayout.addSpacing(2)
        self.buttonLayout.addWidget(self.psPushButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)

        # 添加到总布局
        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.setContentsMargins(1, 0, 1, 5)

        # 设置页面布局
        self.setLayout(self.hBoxLayout)

    def _connectSignal(self) -> None:
        """
        ## 链接所需信号
        """
        self.clearConfigButton.clicked.connect(self._clearBtnSlot)
        self.psPushButton.clicked.connect(self._addBotListBtnSlot)

    @Slot()
    def _addBotListBtnSlot(self) -> None:
        """
        ## 添加到机器人列表
        """
        from src.Ui.AddPage.AddWidget import AddWidget
        from src.Core.Config.ConfigModel import Config
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        # 读取配置文件并追加, 判断是否存在相同的 QQID
        config = Config(**it(AddWidget).getConfig())
        if check_duplicate_bot(config):
            # 检查是否已存在相同的机器人配置
            error_bar(
                self.tr("Bots can't be added"),
                self.tr(f"{config.bot.QQID} it already exists, please do not add it repeatedly"),
            )
            return

        if update_config(config):
            # 更新配置文件, 如果返回为 True 则代表更新成功
            # 执行刷新
            it(BotListWidget).botList.updateList()
            success_bar(
                self.tr("Bot addition success!"),
                self.tr(f"Bot({config.bot.QQID}) it has been successfully added, you can view it in BotList"),
            )
        else:
            # 更新失败则提示查看日志
            error_bar(
                self.tr("Failed"),
                self.tr(
                    "An error is thrown when updating the configuration file, "
                    "please check the detailed error in the Setup >Log and take "
                    "a screenshot to someone who has the ability to solve it for help"
                ),
            )

    @Slot()
    def _clearBtnSlot(self) -> None:
        """
        ## 清理按钮的槽函数
            用于提示用户是否确认清空(还原)所有已有配置项
        """
        from src.Ui.AddPage import AddWidget

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
