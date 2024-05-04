# -*- coding: utf-8 -*-
from typing import List

from creart import it
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QWidget
from qfluentwidgets import (
    BodyLabel,
    ExpandSettingCard,
    FluentIcon,
    FluentIconBase,
    InfoBar,
    InfoBarPosition,
    LineEdit,
    MessageBox,
    MessageBoxBase,
    TitleLabel,
    TransparentPushButton,
    TransparentToolButton,
)


class UrlItem(QWidget):
    """
    ## Url Item
    """

    removed = Signal(QWidget)

    def __init__(self, url: str, parent=None) -> None:
        """
        ## 初始化 item

        ### 参数
            - url: 传入的 url 字符串
            - parent: 父组件

        """
        super().__init__(parent=parent)
        self.url = url
        self.hBoxLayout = QHBoxLayout(self)
        self.urlLabel = BodyLabel(url, self)
        self.removeButton = TransparentToolButton(FluentIcon.CLOSE, self)

        self.setFixedHeight(55)
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Fixed)
        self.hBoxLayout.setContentsMargins(48, 0, 60, 0)
        self.hBoxLayout.addWidget(self.urlLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addSpacing(16)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.removeButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        self.removeButton.clicked.connect(lambda: self.removed.emit(self))


class UrlCard(ExpandSettingCard):
    """
    ## 上报 Url 列表卡片
    """
    # 成功添加 url 的信号
    addSignal = Signal()
    # 当用户删除了所有 url 的信号
    emptiedSignal = Signal()

    def __init__(
        self, icon: FluentIconBase, title: str, content: str, parent=None
    ) -> None:
        """
        ## 初始化卡片

        ### 参数
            - icon: 卡片图标
            - title: 卡片标题
            - content: 卡片内容
            - parent: 父组件
        """
        super().__init__(icon, title, content, parent)
        self.urls: List[str] = []
        self.urlItemList: List[UrlItem] = []

        # 创建所需控件
        self.addUrlButton = TransparentPushButton(FluentIcon.ADD, self.tr("Add URL"))

        # 调用方法
        self._initWidget()

    def getValue(self) -> List[str]:
        return self.urls

    def clear(self) -> None:
        """
        清空卡片内容
        """
        for item in self.urlItemList:
            self._removeUrl(item)
        self.urls.clear()

    def _initWidget(self):
        """
        设置卡片内部控件
        """
        self.addWidget(self.addUrlButton)
        self.card.expandButton.setEnabled(False)

        # 初始化布局
        self.viewLayout.setSpacing(0)
        self.viewLayout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)

        # 连接信号
        self.addUrlButton.clicked.connect(self.__showUrlInputBox)

    def __showUrlInputBox(self):
        """
        显示 URL 输入框
        """
        from src.Ui.AddPage.Add import AddWidget

        box = UrlInputBox(it(AddWidget))
        if not box.exec() or not box.urlLineEdit.text():
            # 如果用户取消或输入空字符,则退出函数
            return

        if box.urlLineEdit.text() in self.urls:
            # 如果用户输入的值已经存在则弹出提示并退出函数
            InfoBar.error(
                title=self.tr("The URL already exists"),
                content=self.tr("Please enter a new URL"),
                orient=Qt.Orientation.Vertical,
                duration=3000,
                position=InfoBarPosition.BOTTOM_RIGHT,
                parent=it(AddWidget),
            )
            return

        self.urls.append(box.urlLineEdit.text())
        self._addUrlItem(box.urlLineEdit.text())
        self.setExpand(True)
        self.addSignal.emit()

    def _addUrlItem(self, url: str) -> None:
        """
        添加 Url Item
        """
        if not self.card.expandButton.isEnabled():
            self.card.expandButton.setEnabled(True)
        item = UrlItem(url, self.view)
        item.removed.connect(self._showConfirmDialog)
        self.urlItemList.append(item)
        self.viewLayout.addWidget(item)
        item.show()
        self._adjustViewSize()

    def _showConfirmDialog(self, item: UrlItem) -> None:
        """
        显示确认对话框
        """
        from src.Ui.AddPage.Add import AddWidget

        box = MessageBox(
            title=self.tr("Confirm"),
            content=self.tr(
                f"Are you sure you want to delete the following URLs?\n\n{item.url}"
            ),
            parent=it(AddWidget),
        )
        box.yesSignal.connect(lambda: self._removeUrl(item))
        box.exec()

    def _removeUrl(self, item: UrlItem):
        """
        移除 Url Item
        """
        if item.url not in self.urls:
            return

        self.urls.remove(item.url)
        self.viewLayout.removeWidget(item)
        item.deleteLater()
        self._adjustViewSize()

        if not len(self.urls):
            self.card.expandButton.clicked.emit()
            self.card.expandButton.setEnabled(False)
            self.emptiedSignal.emit()

    def wheelEvent(self, event) -> None:
        if self.isExpand:
            # 如果是展开状态则将滚轮事件向上传递
            self.parent().wheelEvent(event)


class UrlInputBox(MessageBoxBase):

    def __init__(self, parent=None):
        super().__init__(parent)
        self.titleLabel = TitleLabel(self.tr("Enter the URL"))
        self.urlLineEdit = LineEdit()

        self.urlLineEdit.setPlaceholderText(self.tr("Enter the URL..."))
        self.urlLineEdit.setClearButtonEnabled(True)

        # 将组件添加到布局中
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.urlLineEdit)

        # 设置对话框最小宽度
        self.widget.setMinimumWidth(350)
