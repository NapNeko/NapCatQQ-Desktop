# -*- coding: utf-8 -*-
"""
## 弹出消息框创建
    本模块的目的是为了统一程序内消息框的创建, 从而保持代码的简洁

"""
# 标准库导入
from typing import TYPE_CHECKING, Dict, List

# 第三方库导入
from qfluentwidgets import (
    LineEdit,
    BodyLabel,
    FluentIcon,
    ImageLabel,
    TitleLabel,
    CaptionLabel,
    SubtitleLabel,
    MessageBoxBase,
    HyperlinkButton,
    TransparentToolButton,
)
from PySide6.QtGui import QImage, QPixmap
from PySide6.QtCore import Qt, QDir, QUrl, Slot
from PySide6.QtWidgets import QFileDialog, QHBoxLayout, QVBoxLayout

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window.window import MainWindow


class TextInputBox(MessageBoxBase):
    """文本输入框"""

    def __init__(self, parent: "main_window", placeholder_text: str = "Enter...") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - placeholder_text LineEdit的占位符
        """
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = TitleLabel(self.tr("请输入..."), self)
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

    def __init__(self, title: str, content: str, parent: "main_window") -> None:
        """初始化类, 创建必要控件"""
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = TitleLabel(title, self)
        self.contentLabel = BodyLabel(content, self)

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)


class ImageBox(MessageBoxBase):
    """图片展示框"""

    def __init__(self, title: str, image: str | QImage | QPixmap, parent: "main_window") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - title: 消息框标题
            - parent: 消息框父类
            - image: 显示的图片内容
        """
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = TitleLabel(title, self)
        self.imageLabel = ImageLabel(image, self)

        # 设置属性
        self.imageLabel.scaledToWidth(120)
        self.widget.setMinimumSize(420, 230)

        # 添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.imageLabel, 0, Qt.AlignmentFlag.AlignCenter)

    def setImage(self, image: str | QImage | QPixmap) -> None:
        """
        ## 设置图像

        ## 参数
            - image: 图像
        """
        self.imageLabel.setImage(image)
        self.setImageScaledToWidth(int(self.width() * 0.3))

    def setImageBorderRadius(self, topLeft: int, topRight: int, bottomLeft: int, bottomRight: int) -> None:
        """
        ## 设置图片圆角

        ## 参数
            - radius: 圆角半径
        """
        self.imageLabel.setBorderRadius(topLeft, topRight, bottomLeft, bottomRight)

    def setImageScaledToWidth(self, width: int) -> None:
        """
        ## 设置图片宽度

        ## 参数
            - width: 图片宽度
        """
        self.imageLabel.scaledToWidth(width)

    def setImageScaledToHeight(self, height: int) -> None:
        """
        ## 设置图片高度

        ## 参数
            - height: 图片高度
        """
        self.imageLabel.scaledToHeight(height)


class HyperlinkBox(MessageBoxBase):
    """超链接消息框"""

    def __init__(self, title: str, content: str, hyperlinks: List[Dict[str, QUrl]], parent: "main_window") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - title: 消息框标题
            - content: 消息框内容
            - hyperlinks: 显示的超链接内容
            - parent: 消息框父类
        """
        super().__init__(parent=parent)

        # 创建控件
        self.hyperlinkLayout = QVBoxLayout()
        self.titleLabel = SubtitleLabel(title, self)
        self.contentLabel = CaptionLabel(content, self)

        for link in hyperlinks:
            # 遍历列表添加到 hyperlinkLabels
            self.hyperlinkLayout.addWidget(
                HyperlinkButton(link["url"].toString(), link["name"], self, FluentIcon.LINK),
                1,
                Qt.AlignmentFlag.AlignLeft,
            )

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)
        self.viewLayout.addLayout(self.hyperlinkLayout)


class FolderBox(MessageBoxBase):
    """文件夹消息框"""

    def __init__(self, title: str, parent: "main_window") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - title: 消息框标题
            - content: 消息框内容
            - folder_path: 文件夹路径
            - parent: 消息框父类
        """
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = TitleLabel(title, self)
        self.folderPathEdit = LineEdit(self)
        self.selectFolderButton = TransparentToolButton(FluentIcon.FOLDER_ADD, self)
        self.selectFolderLayout = QHBoxLayout()

        # 设置属性
        self.widget.setMinimumSize(420, 230)
        self.selectFolderButton.clicked.connect(self.selectFolderSlot)

        # 添加到布局
        self.selectFolderLayout.addWidget(self.folderPathEdit)
        self.selectFolderLayout.addWidget(self.selectFolderButton)

        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.selectFolderLayout)

    def getValue(self) -> str:
        """
        ## 获取文件夹路径
        """
        return self.folderPathEdit.text()

    @Slot()
    def selectFolderSlot(self):
        """
        ## 获取文件夹
        """
        folder = QFileDialog.getExistingDirectory(
            self, self.tr("选择文件夹"), QDir.homePath(), QFileDialog.ShowDirsOnly | QFileDialog.DontResolveSymlinks
        )
        if folder:
            self.folderPathEdit.setText(folder)
        else:
            self.folderPathEdit.clear()
