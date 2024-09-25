# -*- coding: utf-8 -*-
"""
## Unit 内部使用的一些基类
"""
import re
from enum import Enum
from tkinter import Image

from PySide6.QtGui import QFont, QDesktopServices
from PySide6.QtCore import Qt, QUrl
from PySide6.QtWidgets import QVBoxLayout, QHBoxLayout
from qfluentwidgets import (
    ScrollArea,
    SimpleCardWidget,
    ImageLabel,
    TitleLabel,
    HyperlinkLabel,
    SubtitleLabel,
    BodyLabel,
    PrimaryPushButton,
    FluentIcon,
    PushButton,
    setFont,
    HeaderCardWidget,
    TextEdit, TransparentToolButton
)

from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.CodeEditor import UpdateLogEdit
from markdown import markdown


class Status(Enum):
    """
    ## 状态枚举类
    """

    # 安装状态
    INSTALL = 0
    UNINSTALLED = 1

    # 更新状态
    UPDATE = 2
    NOUPDATE = 3


class PageBase(ScrollArea):
    """
    ## Page 的基类
    """

    def __init__(self, parent) -> None:
        super().__init__(parent)

        # 创建控件
        self.appCard = DisplayCard(self)
        self.logCard = UpdateLogCard(self)

        # 进行布局
        self.hBoxLayout = QHBoxLayout()
        self.hBoxLayout.addWidget(self.appCard, 1)
        self.hBoxLayout.addWidget(self.logCard, 2)

        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.hBoxLayout)


class DisplayCard(SimpleCardWidget):
    """
    ## 用于左侧的介绍卡片
    """

    def __init__(self, parent) -> None:
        super().__init__(parent)

        # 创建控件
        self.iconLabel = ImageLabel(":/Global/logo.png", self)
        self.nameLabel = TitleLabel("NapCatQQ", self)
        self.hyperLabel = HyperlinkLabel("仓库地址", self)

        self.installButton = PrimaryPushButton(self.tr("安装"), self)
        self.updateButton = PrimaryPushButton(self.tr("更新"), self)
        self.openFolderButton = PushButton(self.tr("打开文件夹"), self)

        self.vBoxLayout = QVBoxLayout()

        # 设置控件
        self.setMaximumWidth(400)
        self.setMinimumWidth(230)

        self.iconLabel.setBorderRadius(8, 8, 8, 8)
        self.iconLabel.scaledToWidth(100)

        self.installButton.setMinimumWidth(140)
        self.updateButton.setMinimumWidth(140)
        self.openFolderButton.setMinimumWidth(140)

        setFont(self.nameLabel, 22, QFont.Weight.Bold)

        # 隐藏控件
        self.updateButton.hide()
        self.openFolderButton.hide()

        # 添加到布局
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addStretch(3)
        self.vBoxLayout.addWidget(self.iconLabel, 1, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addSpacing(20)
        self.vBoxLayout.addWidget(self.nameLabel, 1, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addSpacing(5)
        self.vBoxLayout.addWidget(self.hyperLabel, 1, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(3)
        self.vBoxLayout.addWidget(self.installButton, 2, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.updateButton, 2, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.openFolderButton, 2, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(3)

        self.setLayout(self.vBoxLayout)


class UpdateLogCard(HeaderCardWidget):
    """
    ## 用于右侧的更新日志
    """

    def __init__(self, parent) -> None:
        super().__init__(parent)
        # 创建属性
        self.url = QUrl()

        # 创建控件
        self.logEdit = UpdateLogEdit()
        self.urlButton = TransparentToolButton(FluentIcon.GLOBE)

        # 设置属性
        self.setTitle(self.tr("更新日志"))
        self.urlButton.clicked.connect(lambda: QDesktopServices.openUrl(self.url))

        # 添加到布局
        self.headerLayout.addWidget(self.urlButton, 0, Qt.AlignmentFlag.AlignRight)
        self.viewLayout.addWidget(self.logEdit)
        self.viewLayout.setContentsMargins(8, 4, 8, 4)

    def setLog(self, text) -> None:
        """
        ## 设置 Log
        """
        css_style = '<style>pre { white-space: pre-wrap; }</style>'
        self.logEdit.setHtml(css_style + markdown(re.sub(r'\r\n', '\n', text), extensions=['nl2br']))

    def setUrl(self, url) -> None:
        """
        ## 设置 Url
        """
        self.url.setUrl(url)
