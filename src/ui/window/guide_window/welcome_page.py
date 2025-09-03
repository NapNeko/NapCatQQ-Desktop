# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import ImageLabel, TitleLabel, SubtitleLabel, HyperlinkButton, PrimaryPushButton
from qframelesswindow import FramelessWindow
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.ui.common.icon import StaticIcon
from src.core.network.urls import Urls


class WelcomePage(QWidget):
    """欢迎页面"""

    def __init__(self, parent: FramelessWindow) -> None:
        super().__init__(parent)

        # 创建控件
        self.logoLabel = ImageLabel(StaticIcon.LOGO.path(), self)
        self.titleLabel = TitleLabel("NapCatQQ", self)
        self.subTitleLabel = SubtitleLabel(self.tr("现代化的基于 NTQQ\n的 Bot 协议端实现"), self)
        self.installButton = PrimaryPushButton(self.tr("开始安装"), self)
        self.githubLinkButton = HyperlinkButton(self)

        # 设置属性
        self.logoLabel.scaledToHeight(256)
        self.githubLinkButton.setText("GitHub")
        self.githubLinkButton.setIcon(FI.GITHUB)
        self.githubLinkButton.setUrl(Urls.NAPCATQQ_REPO.value)

        # 布局
        self.logoLabel.move(310, 100)
        self.titleLabel.move(80, 160)
        self.subTitleLabel.move(80, 210)
        self.installButton.move(80, 290)
        self.githubLinkButton.move(180, 290)

        # 信号连接
        self.installButton.clicked.connect(self.parent().on_next_page)
