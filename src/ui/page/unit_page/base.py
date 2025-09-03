# -*- coding: utf-8 -*-
"""
## Unit 内部使用的一些基类
"""
# 标准库导入
import re
from enum import Enum

# 第三方库导入
from markdown import markdown
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    ImageLabel,
    PushButton,
    ScrollArea,
    TitleLabel,
    ProgressRing,
    HyperlinkLabel,
    HeaderCardWidget,
    SimpleCardWidget,
    PrimaryPushButton,
    TransparentToolButton,
    IndeterminateProgressRing,
    setFont,
)
from PySide6.QtGui import QFont, QImage, QPixmap, QDesktopServices
from PySide6.QtCore import Qt, QUrl, QSize
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.ui.UnitPage.status import StatusLabel, ButtonStatus, ProgressRingStatus
from src.ui.common.code_editor import UpdateLogExhibit


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

        self.getVersion = parent.getVersion


class DisplayCard(SimpleCardWidget):
    """
    ## 用于左侧的介绍卡片
    """

    # 按钮显示标识符
    button_visibility: dict = {
        ButtonStatus.INSTALL: {"install": False, "update": False, "openFolder": True},
        ButtonStatus.UNINSTALLED: {"install": True, "update": False, "openFolder": False},
        ButtonStatus.UPDATE: {"install": False, "update": True, "openFolder": False},
        ButtonStatus.NONE: {"install": False, "update": False, "openFolder": False},
    }

    # 进度条显示标识符
    progress_ring_visibility: dict = {
        ProgressRingStatus.INDETERMINATE: {"indeterminate": True, "determinate": False},
        ProgressRingStatus.DETERMINATE: {"indeterminate": False, "determinate": True},
        ProgressRingStatus.NONE: {"indeterminate": False, "determinate": False},
    }

    # 状态标签显示标识符
    status_label_visibility: dict = {StatusLabel.SHOW: True, StatusLabel.HIDE: False}

    def __init__(self, parent) -> None:
        super().__init__(parent)

        # 创建控件
        self.iconLabel = ImageLabel(":/Global/logo.png", self)
        self.nameLabel = TitleLabel("Unknown", self)
        self.hyperLabel = HyperlinkLabel("Unknown", self)
        self.statusLabel = BodyLabel("Unknown", self)

        self.installButton = PrimaryPushButton(self.tr("安装"), self)
        self.updateButton = PrimaryPushButton(self.tr("更新"), self)
        self.openFolderButton = PushButton(self.tr("打开文件夹"), self)

        self.indeterminateProgressRing = IndeterminateProgressRing(self)
        self.progressRing = ProgressRing(self)

        self.vBoxLayout = QVBoxLayout()

        # 设置控件
        self.setMaximumWidth(400)
        self.setMinimumWidth(230)

        self.iconLabel.setBorderRadius(8, 8, 8, 8)
        self.iconLabel.scaledToHeight(128)

        self.installButton.setMinimumWidth(140)
        self.updateButton.setMinimumWidth(140)
        self.openFolderButton.setMinimumWidth(140)

        self.indeterminateProgressRing.setFixedSize(QSize(72, 72))
        self.progressRing.setFixedSize(QSize(72, 72))
        self.progressRing.setTextVisible(True)

        setFont(self.nameLabel, 22, QFont.Weight.Bold)
        setFont(self.statusLabel, 13, QFont.Weight.Normal)

        # 隐藏控件
        self.installButton.hide()
        self.updateButton.hide()
        self.statusLabel.hide()
        self.indeterminateProgressRing.hide()
        self.progressRing.hide()

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
        self.vBoxLayout.addWidget(self.indeterminateProgressRing, 2, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.progressRing, 2, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addSpacing(20)
        self.vBoxLayout.addWidget(self.statusLabel, 0, Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(3)

        self.setLayout(self.vBoxLayout)

    def setIcon(self, icon: str | QPixmap | QImage) -> None:
        self.iconLabel.setImage(icon)

    def setName(self, name: str) -> None:
        self.nameLabel.setText(name)

    def setHyperLabelName(self, name: str) -> None:
        self.hyperLabel.setText(name)

    def setHyperLabelUrl(self, url: str | QUrl) -> None:
        self.hyperLabel.setUrl(url)

    def setStatusText(self, text: str) -> None:
        self.statusLabel.setText(text)

    def setProgressRingValue(self, value: int) -> None:
        self.progressRing.setValue(value)

    def setVisibility(self, visible_buttons: dict, visible_progress_rings: dict, visible_status_label: dict) -> None:
        """
        ## 设置按钮和进度环以及状态标签的可见性

        ## 参数
            - visible_buttons: dict, 按钮可见性
            - visible_progress_rings: dict, 进度环可见性
            - visible_status_label: dict, 状态标签可见性
        """
        self.installButton.setVisible(visible_buttons.get("install", False))
        self.updateButton.setVisible(visible_buttons.get("update", False))
        self.openFolderButton.setVisible(visible_buttons.get("openFolder", False))

        self.indeterminateProgressRing.setVisible(visible_progress_rings.get("indeterminate", False))
        self.progressRing.setVisible(visible_progress_rings.get("determinate", False))

        self.statusLabel.setVisible(visible_status_label)

    def switchButton(self, status: ButtonStatus) -> None:
        """
        ## 切换按钮, 如果需要显示按钮, 则隐藏所有进度条
        """
        self.setVisibility(
            self.button_visibility[status],
            self.progress_ring_visibility[ProgressRingStatus.NONE],
            self.status_label_visibility[StatusLabel.HIDE],
        )

    def switchProgressRing(self, status: ProgressRingStatus) -> None:
        """
        ## 切换进度环, 如果需要显示进度条, 则隐藏所有按钮, 并显示 statusLabel
        """
        self.setVisibility(
            self.button_visibility[ButtonStatus.NONE],
            self.progress_ring_visibility[status],
            self.status_label_visibility[StatusLabel.SHOW],
        )


class UpdateLogCard(HeaderCardWidget):
    """
    ## 用于右侧的更新日志
    """

    def __init__(self, parent) -> None:
        super().__init__(parent)
        # 创建属性
        self.url = QUrl()

        # 创建控件
        self.logEdit = UpdateLogExhibit()
        self.urlButton = TransparentToolButton(FluentIcon.GLOBE)

        # 设置属性
        self.setTitle(self.tr("更新日志"))
        self.urlButton.clicked.connect(lambda: QDesktopServices.openUrl(self.url))

        # 添加到布局
        self.headerLayout.addWidget(self.urlButton, 0, Qt.AlignmentFlag.AlignRight)
        self.viewLayout.addWidget(self.logEdit)
        self.viewLayout.setContentsMargins(8, 4, 8, 4)

    def setLog(self, text: str) -> None:
        """
        ## 设置 Log
        """
        css_style = "<style>pre { white-space: pre-wrap; }</style>"
        self.logEdit.setHtml(css_style + markdown(re.sub(r"\r\n", "\n", text), extensions=["nl2br"]))

    def setUrl(self, url: str) -> None:
        """
        ## 设置 Url
        """
        self.url.setUrl(url)
