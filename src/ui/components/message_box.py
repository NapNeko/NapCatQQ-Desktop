# -*- coding: utf-8 -*-
"""
## 弹出消息框创建
    本模块的目的是为了统一程序内消息框的创建, 从而保持代码的简洁

"""
# 标准库导入
from typing import TYPE_CHECKING, Dict, List

# 第三方库导入
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    HyperlinkButton,
    ImageLabel,
    LineEdit,
    MessageBoxBase,
    SubtitleLabel,
    TitleLabel,
    TransparentToolButton,
)
from PySide6.QtCore import QDir, Qt, QUrl
from PySide6.QtGui import QImage, QPixmap
from PySide6.QtWidgets import QFileDialog, QHBoxLayout, QVBoxLayout

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window.window import MainWindow


class TextInputBox(MessageBoxBase):
    """文本输入框弹窗，用于获取用户输入的文本信息."""

    def __init__(self, parent: MainWindow, placeholder_text: str = "Enter...") -> None:
        """
        初始化文本输入框弹窗.

        Args:
            parent: 父级窗口，通常为 main_window.
            placeholder_text: 输入框的占位提示文本，默认为 "Enter...".
        """
        super().__init__(parent=parent)
        # 创建控件
        self.title_label = TitleLabel(self.tr("请输入..."), self)
        self.input_line_edit = LineEdit(self)

        # 设置属性
        self.input_line_edit.setPlaceholderText(placeholder_text)
        self.input_line_edit.setClearButtonEnabled(True)
        self.widget.setMinimumSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.input_line_edit)


class AskBox(MessageBoxBase):
    """询问用户是否确认的提示框，通常用于危险操作前的二次确认."""

    def __init__(self, title: str, content: str, parent: MainWindow) -> None:
        """
        初始化确认提示框.

        Args:
            title: 弹窗标题.
            content: 提示内容文本.
            parent: 父级窗口.
        """
        super().__init__(parent=parent)
        # 创建控件
        self.title_label = TitleLabel(title, self)
        self.content_label = BodyLabel(content, self)

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)


class ImageBox(MessageBoxBase):
    """图片展示弹窗，用于在弹窗中展示一张图片."""

    def __init__(self, title: str, image: str | QImage | QPixmap, parent: MainWindow) -> None:
        """
        初始化图片展示弹窗。

        Args:
            title: 弹窗标题。
            image: 图片数据，可以是路径 str、QImage 或 QPixmap。
            parent: 父级窗口。
        """
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(title, self)
        self.image_label = ImageLabel(image, self)

        # 设置属性
        self.image_label.scaledToWidth(120)
        self.widget.setMinimumSize(420, 230)

        # 添加到布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.image_label, 0, Qt.AlignmentFlag.AlignCenter)

    def set_image(self, image: str | QImage | QPixmap) -> None:
        """设置图片

        Args:
            image (str | QImage | QPixmap): 图片数据，可以是路径 str、QImage 或 QPixmap
        """
        self.image_label.setImage(image)
        self.set_image_scaled_to_width(int(self.width() * 0.3))

    def set_image_border_radius(self, top_left: int, top_right: int, bottom_left: int, bottom_right: int) -> None:
        """设置图片圆角

        Args:
            top_left (int): 左上角圆角
            top_right (int): 右上角圆角
            bottom_left (int): 左下角圆角
            bottom_right (int): 右下角圆角
        """
        self.image_label.setBorderRadius(top_left, top_right, bottom_left, bottom_right)

    def set_image_scaled_to_width(self, width: int) -> None:
        """设置图片宽度

        Args:
            width (int): 图片宽度
        """
        self.image_label.scaledToWidth(width)

    def set_image_scaled_to_height(self, height: int) -> None:
        """设置图片高度

        Args:
            height (int): 图片高度
        """
        self.image_label.scaledToHeight(height)


class HyperlinkBox(MessageBoxBase):
    """超链接消息框"""

    def __init__(self, title: str, content: str, hyperlinks: List[Dict[str, QUrl]], parent: MainWindow) -> None:
        """初始化

        Args:
            title (str): 消息框标题
            content (str): 消息框内容
            hyperlinks (List[Dict[str, QUrl]]): 超链接列表, 每个字典包含 'name' 和 'url' 键
            parent (MainWindow): 消息框父类
        """

        super().__init__(parent=parent)

        # 创建控件
        self.hyperlink_layout = QVBoxLayout()
        self.title_label = SubtitleLabel(title, self)
        self.content_label = CaptionLabel(content, self)

        for link in hyperlinks:
            # 遍历列表添加到 hyperlinkLabels
            self.hyperlink_layout.addWidget(
                HyperlinkButton(link["url"].toString(), link["name"], self, FluentIcon.LINK),
                1,
                Qt.AlignmentFlag.AlignLeft,
            )

        # 设置属性
        self.widget.setMinimumSize(420, 230)

        # 添加到布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)
        self.viewLayout.addLayout(self.hyperlink_layout)


class FolderBox(MessageBoxBase):
    """文件夹消息框"""

    def __init__(self, title: str, parent: MainWindow) -> None:
        """初始化

        Args:
            title (str): 消息框标题
            parent (MainWindow): 消息框父类
        """
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(title, self)
        self.folder_path_edit = LineEdit(self)
        self.select_folder_button = TransparentToolButton(FluentIcon.FOLDER_ADD, self)
        self.select_folder_layout = QHBoxLayout()

        # 设置属性
        self.widget.setMinimumSize(420, 230)
        self.select_folder_button.clicked.connect(self.on_select_folder)

        # 添加到布局
        self.select_folder_layout.addWidget(self.folder_path_edit)
        self.select_folder_layout.addWidget(self.select_folder_button)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.select_folder_layout)

    def get_value(self) -> str:
        """获取文件夹路径"""
        return self.folder_path_edit.text()

    def on_select_folder(self):
        """获取文件夹"""
        folder = QFileDialog.getExistingDirectory(
            self,
            self.tr("选择文件夹"),
            QDir.homePath(),
            QFileDialog.ShowDirsOnly | QFileDialog.DontResolveSymlinks,
        )
        if folder:
            self.folder_path_edit.setText(folder)
        else:
            self.folder_path_edit.clear()
