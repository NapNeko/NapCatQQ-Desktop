# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import ScrollArea, TitleLabel, ExpandLayout, MessageBoxBase, SettingCardGroup, OptionsSettingCard
from PySide6.QtCore import Qt, Slot, QObject
from PySide6.QtWidgets import QWidget, QGridLayout

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.common.input_card.generic_card import ShowDialogCard, SwitchConfigCard, LineEditConfigCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class General(ScrollArea):

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupGeneralWidget")

        # 调用方法
        self._createConfigCards()
        self._setLayout()

    def _createConfigCards(self) -> None:
        """
        创建配置项卡片
        """
        # 创建组 - 行为
        self.actionGroup = SettingCardGroup(title=self.tr("行为"), parent=self.view)
        # 创建项
        self.closeBtnCard = OptionsSettingCard(
            configItem=cfg.closeBtnAction,
            icon=FI.CLOSE,
            title=self.tr("关闭按钮"),
            content=self.tr("选择点击关闭按钮时的行为"),
            texts=[self.tr("关闭程序"), self.tr("最小化隐藏到托盘")],
            parent=self.actionGroup,
        )

        # 创建组 - 事件
        self.eventGroup = SettingCardGroup(title=self.tr("事件"), parent=self.view)
        # 创建项
        self.botOfflineEmailCard = ShowDialogCard(
            dialog=BotOfflineEmailDialog,
            icon=FI.CHAT,
            title=self.tr("机器人离线通知[邮件]"),
            content=self.tr("设置机器人离线邮件通知, 目前仅测试过QQ邮箱"),
            parent=self.eventGroup,
        )

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.actionGroup.addSettingCard(self.closeBtnCard)
        self.eventGroup.addSettingCard(self.botOfflineEmailCard)

        # 添加到布局
        self.expand_layout.addWidget(self.actionGroup)
        self.expand_layout.addWidget(self.eventGroup)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)


class BotOfflineEmailDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("机器人离线通知[邮件]"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用邮箱通知"))
        self.receiversCard = LineEditConfigCard(FI.ROBOT, self.tr("收件人邮箱"), "Receivers@qq.com")
        self.senderCard = LineEditConfigCard(FI.ROBOT, self.tr("发件人邮箱"), "Sender@qq.com")
        self.tokenCard = LineEditConfigCard(FI.VPN, self.tr("发件人邮箱密钥"), "Token")
        self.stmpServerCard = LineEditConfigCard(FI.ALBUM, self.tr("SMTP服务器"), "smtp.qq.com")

        # 布局
        self.gridLayout = QGridLayout()
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 4)
        self.gridLayout.addWidget(self.receiversCard, 1, 0, 1, 2)
        self.gridLayout.addWidget(self.senderCard, 1, 2, 1, 2)
        self.gridLayout.addWidget(self.tokenCard, 2, 0, 1, 4)
        self.gridLayout.addWidget(self.stmpServerCard, 3, 0, 1, 4)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.setSpacing(8)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)
        self.widget.setMinimumSize(500, 400)

        # 填充配置
        self.enableCard.fillValue(cfg.get(cfg.titleTabBar))
        self.receiversCard.fillValue(cfg.get(cfg.emailReceiver))
        self.senderCard.fillValue(cfg.get(cfg.emailSender))
        self.tokenCard.fillValue(cfg.get(cfg.emailToken))
        self.stmpServerCard.fillValue(cfg.get(cfg.emailStmpServer))

    def accept(self) -> None:
        """接受按钮"""
        cfg.set(cfg.titleTabBar, self.enableCard.getValue())
        cfg.set(cfg.emailReceiver, self.receiversCard.getValue())
        cfg.set(cfg.emailSender, self.senderCard.getValue())
        cfg.set(cfg.emailToken, self.tokenCard.getValue())
        cfg.set(cfg.emailStmpServer, self.stmpServerCard.getValue())
        super().accept()
