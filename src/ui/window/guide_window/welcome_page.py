# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import FluentIcon, HyperlinkButton, ImageLabel, PrimaryPushButton, SubtitleLabel, TitleLabel
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.network.urls import Urls
from src.ui.common.icon import StaticIcon

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.guide_window.guide_window import GuideWindow


class WelcomePage(QWidget):
    """欢迎页面"""

    def __init__(self, parent: "GuideWindow") -> None:
        """初始化

        创建控件, 设置属性, 布局, 信号连接

        Args:
            parent (FramelessWindow): 父窗体
        """
        super().__init__(parent)

        # 创建控件
        self.logo_label = ImageLabel(StaticIcon.LOGO.path(), self)
        self.title_label = TitleLabel("NapCatQQ", self)
        self.subtitle_label = SubtitleLabel(self.tr("现代化的基于 NTQQ\n的 Bot 协议端实现"), self)
        self.install_button = PrimaryPushButton(self.tr("开始安装"), self)
        self.github_link_button = HyperlinkButton(self)

        # 设置属性
        self.logo_label.scaledToHeight(256)
        self.github_link_button.setText("GitHub")
        self.github_link_button.setIcon(FluentIcon.GITHUB)
        self.github_link_button.setUrl(Urls.NAPCATQQ_REPO.value)

        # 布局
        self.logo_label.move(310, 100)
        self.title_label.move(80, 160)
        self.subtitle_label.move(80, 210)
        self.install_button.move(80, 290)
        self.github_link_button.move(180, 290)

        # 信号连接
        self.install_button.clicked.connect(self.on_click)

    def on_click(self) -> None:
        """点击事件"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        it(GuideWindow).on_next_page()
