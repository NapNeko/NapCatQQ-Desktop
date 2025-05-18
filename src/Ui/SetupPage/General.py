# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import (
    FluentIcon,
    ScrollArea,
    ExpandLayout,
    RangeSettingCard,
    SettingCardGroup,
    OptionsSettingCard,
    CustomColorSettingCard,
    ExpandGroupSettingCard,
    setTheme,
    setThemeColor,
)
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.common.info_bar import success_bar
from src.Ui.SetupPage.ExpandGroupSettingItem import FileItem, RangeItem, SwitchItem, ComboBoxItem, LineEditItem

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
        self._connect_signal()
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
            icon=FluentIcon.CLOSE,
            title=self.tr("关闭按钮"),
            content=self.tr("选择点击关闭按钮时的行为"),
            texts=[self.tr("关闭程序"), self.tr("最小化隐藏到托盘")],
            parent=self.actionGroup,
        )

        # 创建组 - 事件
        self.eventGroup = SettingCardGroup(title=self.tr("事件"), parent=self.view)
        # 创建项
        self.botOfflineEventCard = BotOfflineEventCard(self.eventGroup)

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组
        self.actionGroup.addSettingCard(self.closeBtnCard)
        self.eventGroup.addSettingCard(self.botOfflineEventCard)

        # 添加到布局
        self.expand_layout.addWidget(self.actionGroup)
        self.expand_layout.addWidget(self.eventGroup)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

    def _connect_signal(self) -> None:
        """
        信号处理
        """
        # 连接重启提示
        # cfg.appRestartSig.connect(lambda: success_bar(self.tr("配置在重启后生效")))
        # 连接启动相关


class BotOfflineEventCard(ExpandGroupSettingCard):
    """
    ## 机器人离线事件
    """

    def __init__(self, parent) -> None:
        super().__init__(
            icon=FluentIcon.RINGER,
            title=self.tr("机器人离线事件"),
            content=self.tr("设置机器人离线时的事件"),
            parent=parent,
        )

        # 创建项
        self.enabledEmailNoticeItem = SwitchItem(cfg.botOfflineEmailNotice, self.tr("启用邮件通知"), self)
        self.emailReceiversItem = LineEditItem(
            cfg.emailReceiver, self.tr("接收人邮箱"), placeholder_text=self.tr("A@qq.com"), parent=self
        )
        self.emailSenderItem = LineEditItem(
            cfg.emailSender, self.tr("发送人邮箱"), placeholder_text=self.tr("B@qq.com"), parent=self
        )
        self.emailToken = LineEditItem(cfg.emailToken, self.tr("发送人邮箱密钥"), parent=self)
        self.emailStmpServer = LineEditItem(cfg.emailStmpServer, self.tr("SMTP服务器"), parent=self)

        # 调整内部布局
        self.viewLayout.setContentsMargins(0, 0, 1, 0)
        self.viewLayout.setSpacing(2)

        # 添加组件
        self.addGroupWidget(self.enabledEmailNoticeItem)
        self.addGroupWidget(self.emailReceiversItem)
        self.addGroupWidget(self.emailSenderItem)
        self.addGroupWidget(self.emailToken)
        self.addGroupWidget(self.emailStmpServer)

        # 信号连接
        self.enabledEmailNoticeItem.checkedChanged.connect(self.isHide)

        # 调用方法
        self.isHide()

    def isHide(self) -> None:
        """
        ## 判断是否需要隐藏
        """
        for item in [self.enabledEmailNoticeItem]:
            if cfg.get(item.configItem):
                self.showItem(item)
            else:
                self.hideItem(item)

        self._adjustViewSize()

    def showItem(self, item: SwitchItem) -> None:
        """
        ## 显示项
        """
        match item.configItem:
            case cfg.botOfflineEmailNotice:
                self.emailReceiversItem.setEnabled(True)
                self.emailSenderItem.setEnabled(True)
                self.emailToken.setEnabled(True)
                self.emailStmpServer.setEnabled(True)

    def hideItem(self, item: SwitchItem) -> None:
        """
        ## 隐藏项
        """
        match item.configItem:
            case cfg.botOfflineEmailNotice:
                self.emailReceiversItem.setEnabled(False)
                self.emailSenderItem.setEnabled(False)
                self.emailToken.setEnabled(False)
                self.emailStmpServer.setEnabled(False)

    def wheelEvent(self, event) -> None:
        """
        ## 滚动事件上传到父控件
        """
        self.parent().wheelEvent(event)
        super().wheelEvent(event)
