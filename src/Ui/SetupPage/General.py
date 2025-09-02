# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import (
    PushButton,
    ScrollArea,
    TitleLabel,
    ExpandLayout,
    MessageBoxBase,
    SettingCardGroup,
    OptionsSettingCard,
)
from PySide6.QtCore import Qt, QSize, QObject
from PySide6.QtWidgets import QWidget, QGridLayout, QHBoxLayout

# 项目内模块导入
from src.Core.Config import cfg
from src.Core.Utils.email import EncryptionType, test_email
from src.Ui.common.info_bar import success_bar, warning_bar
from src.Ui.common.input_card.generic_card import (
    ShowDialogCard,
    SwitchConfigCard,
    ComboBoxConfigCard,
    LineEditConfigCard,
    JsonTemplateEditConfigCard,
)

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
            content=self.tr("设置机器人离线邮件通知"),
            parent=self.eventGroup,
        )
        self.botOfflineWebHookCard = ShowDialogCard(
            dialog=BotOfflineWebHookDialog,
            icon=FI.CHAT,
            title=self.tr("机器人离线通知[Webhook]"),
            content=self.tr("设置机器人离线Webhook通知"),
            parent=self.eventGroup,
        )

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.actionGroup.addSettingCard(self.closeBtnCard)
        self.eventGroup.addSettingCard(self.botOfflineEmailCard)
        self.eventGroup.addSettingCard(self.botOfflineWebHookCard)

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
        self.stmpServerPortCard = LineEditConfigCard(FI.ALBUM, self.tr("SMTP服务器端口"), "465")
        self.encryptionCard = ComboBoxConfigCard(FI.VPN, self.tr("加密方式"), EncryptionType.get_values())
        self.testEmailButton = PushButton(self.tr("发送测试邮件"))

        # 填充配置
        self.fill_config()

        # 布局
        self.gridLayout = QGridLayout()
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 4)
        self.gridLayout.addWidget(self.receiversCard, 1, 0, 1, 4)
        self.gridLayout.addWidget(self.senderCard, 2, 0, 1, 4)
        self.gridLayout.addWidget(self.tokenCard, 3, 0, 1, 4)
        self.gridLayout.addWidget(self.stmpServerCard, 4, 0, 1, 2)
        self.gridLayout.addWidget(self.stmpServerPortCard, 4, 2, 1, 2)
        self.gridLayout.addWidget(self.encryptionCard, 5, 0, 1, 4)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.setSpacing(8)
        self.buttonLayout.addWidget(self.testEmailButton, 1)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)
        self.widget.setMinimumSize(500, 400)

        # 链接信号
        self.testEmailButton.clicked.connect(lambda: (self.save_config(), test_email()))

    def fill_config(self) -> None:
        # 填充配置
        self.enableCard.fillValue(cfg.get(cfg.titleTabBar))
        self.receiversCard.fillValue(cfg.get(cfg.emailReceiver))
        self.senderCard.fillValue(cfg.get(cfg.emailSender))
        self.tokenCard.fillValue(cfg.get(cfg.emailToken))
        self.stmpServerCard.fillValue(cfg.get(cfg.emailStmpServer))
        self.stmpServerPortCard.fillValue(str(cfg.get(cfg.emailStmpPort)))
        self.encryptionCard.fillValue(cfg.get(cfg.emailEncryption))

    def save_config(self) -> None:
        """保存配置"""
        try:
            cfg.set(cfg.titleTabBar, self.enableCard.getValue())
            cfg.set(cfg.emailReceiver, self.receiversCard.getValue())
            cfg.set(cfg.emailSender, self.senderCard.getValue())
            cfg.set(cfg.emailToken, self.tokenCard.getValue())
            cfg.set(cfg.emailStmpServer, self.stmpServerCard.getValue())
            cfg.set(cfg.emailStmpPort, int(self.stmpServerPortCard.getValue()))
            cfg.set(cfg.emailEncryption, self.encryptionCard.getValue())
            success_bar(self.tr("配置已保存"))
            self.fill_config()
        except ValueError:
            warning_bar(self.tr("配置保存失败，请检查输入是否正确"))

    def accept(self) -> None:
        """接受按钮"""
        self.save_config()
        super().accept()


class BotOfflineWebHookDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("机器人离线通知[Webhook]"), self)
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用Webhook通知"))
        self.webhookUrlCard = LineEditConfigCard(FI.ROBOT, self.tr("Webhook地址"), "https://example.com/webhook")
        self.webhookSecretCard = LineEditConfigCard(FI.VPN, self.tr("Webhook 密钥"), "Secret")
        self.jsonCard = JsonTemplateEditConfigCard(FI.CODE, self.tr("Webhook JSON"))
        self.testButtonn = PushButton(self.tr("发送测试请求"), self)

        # 布局
        self.gridLayout = QGridLayout()
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 4)
        self.gridLayout.addWidget(self.webhookUrlCard, 1, 0, 1, 4)
        self.gridLayout.addWidget(self.jsonCard, 2, 0, 1, 4)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.setSpacing(8)

        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)

        self.buttonLayout.addWidget(self.testButtonn, 1)

        # 设置
        self.widget.setMinimumSize(650, 400)

        # 填充配置
        self.fill_config()

    def fill_config(self) -> None:
        # 填充配置
        self.enableCard.fillValue(cfg.get(cfg.botOfflineWebHookNotice))
        self.webhookUrlCard.fillValue(cfg.get(cfg.webHookUrl))
        self.webhookSecretCard.fillValue(cfg.get(cfg.webHookSecret))
        self.jsonCard.fillValue(cfg.get(cfg.webHookJson))

    def save_config(self) -> None:
        """保存配置"""
        try:
            cfg.set(cfg.botOfflineWebHookNotice, self.enableCard.getValue())
            cfg.set(cfg.webHookUrl, self.webhookUrlCard.getValue())
            cfg.set(cfg.webHookSecret, self.webhookSecretCard.getValue())
            cfg.set(cfg.webHookJson, self.jsonCard.getValue())
            self.fill_config()
        except ValueError:
            warning_bar(self.tr("配置保存失败，请检查输入是否正确"))

    def accept(self) -> None:
        """接受按钮"""

        if self.jsonCard.jsonTextEdit.checkJson(False) is False:
            return

        self.save_config()
        super().accept()
