# -*- coding: utf-8 -*-
"""
## 弹出消息框创建
    本模块的目的是为了统一程序内消息框的创建, 从而保持代码的简洁

"""
from typing import TYPE_CHECKING, Dict, List

from PySide6.QtGui import QImage, QPixmap
from PySide6.QtCore import Qt, QUrl
from qfluentwidgets import (
    LineEdit,
    FluentIcon,
    ImageLabel,
    CaptionLabel,
    SubtitleLabel,
    MessageBoxBase,
    HyperlinkButton,
)
from PySide6.QtWidgets import QVBoxLayout

if TYPE_CHECKING:
    from src.Ui.MainWindow.Window import MainWindow


class TextInputBox(MessageBoxBase):
    """文本输入框"""

    def __init__(self, parent: "MainWindow", placeholder_text: str = "Enter...") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - placeholder_text LineEdit的占位符
        """
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = SubtitleLabel(self.tr("请输入..."), self)
        self.urlLineEdit = LineEdit(self)

        # 设置属性
        self.urlLineEdit.setPlaceholderText(placeholder_text)
        self.urlLineEdit.setClearButtonEnabled(True)
        self.widget.setMinimumSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.urlLineEdit)


class AskBox(MessageBoxBase):
    """
    ##询问用户是否确认提示框
    
    ## 参数
        - title: 消息框标题
        - content: 消息框内容
        - parent: 消息框父类
    """

    def __init__(self, title: str, content: str, parent: "MainWindow") -> None:
        """初始化类, 创建必要控件"""
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = SubtitleLabel(title, self)
        self.contentLabel = CaptionLabel(content, self)

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)


class ImageBox(MessageBoxBase):
    """图片展示框"""

    def __init__(self, title: str, image: str | QImage | QPixmap, parent: "MainWindow") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - title: 消息框标题
            - parent: 消息框父类
            - image: 显示的图片内容
        """
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = SubtitleLabel(title, self)
        self.imageLabel = ImageLabel(image, self)

        # 设置属性
        self.imageLabel.setMinimumSize(100, 100)
        self.widget.setMinimumSize(420, 230)
        self.yesButton.setText(self.tr("刷新"))

    def setImage(self, image: str | QImage | QPixmap) -> None:
        """
        ## 设置图像

        ## 参数
            - image: 图像
        """
        self.imageLabel.setImage(image)


class HyperlinkBox(MessageBoxBase):
    """超链接消息框"""

    def __init__(self, title: str, content: str, hyperlinks: List[Dict[str, QUrl]], parent: "MainWindow") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - title: 消息框标题
            - content: 消息框内容
            - hyperlinks: 显示的超链接内容
            - parent: 消息框父类
        """
        super().__init__(parent=parent)

        # 创建属性
        self.hyperlinkLabels = []

        # 创建控件
        self.hyperlinkLayout = QVBoxLayout()
        self.titleLabel = SubtitleLabel(title, self)
        self.contentLabel = CaptionLabel(content, self)

        for link in hyperlinks:
            # 遍历列表添加到 hyperlinkLabels
            self.hyperlinkLabels.append(
                HyperlinkButton(link['url'].toString(), link['name'], self, FluentIcon.LINK)
            )

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)
        for label in self.hyperlinkLabels:
            self.hyperlinkLayout.addWidget(label, 1, Qt.AlignmentFlag.AlignLeft)
        self.viewLayout.addLayout(self.hyperlinkLayout)
