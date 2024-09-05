# -*- coding: utf-8 -*-
from typing import List

from creart import it
from PySide6.QtCore import Qt, Signal
from qfluentwidgets import (
    InfoBar,
    LineEdit,
    BodyLabel,
    FluentIcon,
    MessageBox,
    TitleLabel,
    FluentIconBase,
    MessageBoxBase,
    InfoBarPosition,
    ExpandSettingCard,
    TransparentPushButton,
    TransparentToolButton,
)
from PySide6.QtWidgets import QWidget, QHBoxLayout, QSizePolicy


class TextItem(QWidget):
    """
    ## Url Item
    """

    removed = Signal(QWidget)

    def __init__(self, text: str, parent=None) -> None:
        """
        ## 初始化 item

        ### 参数
            - text: 传入的 text 字符串
            - parent: 父组件

        """
        super().__init__(parent=parent)
        self.text = text
        self.hBoxLayout = QHBoxLayout(self)
        self.urlLabel = BodyLabel(text, self)
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


class TextCard(ExpandSettingCard):
    """
    ## 上报 Text 列表卡片
    """

    # 成功添加 url 的信号
    addSignal = Signal()
    # 当用户删除了所有 url 的信号
    emptiedSignal = Signal()

    def __init__(
        self, identifier: str, icon: FluentIcon | FluentIconBase, title: str, content: str, parent=None
    ) -> None:
        """
        ## 初始化卡片

        ### 参数
            - identifier: 标识符
            - icon: 卡片图标
            - title: 卡片标题
            - content: 卡片内容
            - parent: 父组件
        """
        super().__init__(icon, title, content, parent)
        self.identifier = identifier
        self.texts: List[str] = []
        self.textItemList: List[TextItem] = []

        # 创建所需控件
        self.addUrlButton = TransparentPushButton(FluentIcon.ADD, self.tr("Add"))

        # 调用方法
        self._initWidget()

    def fillValue(self, values: List[str]) -> None:
        self.texts = values
        [self._addUrlItem(url) for url in self.texts]

    def getValue(self) -> List[str]:
        return self.texts

    def clear(self) -> None:
        """
        清空卡片内容
        """
        for item in self.textItemList:
            self._removeUrl(item)
        self.texts.clear()

    def _initWidget(self) -> None:
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
        self.addUrlButton.clicked.connect(self._showTextInputBox)

    def _showTextInputBox(self) -> None:
        """
        显示 Text 输入框
        """
        from src.Ui.AddPage import AddWidget
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        box = TextInputBox(it(AddWidget)) if self.identifier == "Add" else TextInputBox(it(BotListWidget))
        if not box.exec() or not box.urlLineEdit.text():
            # 如果用户取消或输入空字符,则退出函数
            return

        if box.urlLineEdit.text() in self.texts:
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

        self.texts.append(box.urlLineEdit.text())
        self._addUrlItem(box.urlLineEdit.text())
        self.setExpand(True)
        self.addSignal.emit()

    def _addUrlItem(self, url: str) -> None:
        """
        添加 Url Item
        """
        if not self.card.expandButton.isEnabled():
            self.card.expandButton.setEnabled(True)
        item = TextItem(url, self.view)
        item.removed.connect(self._showConfirmDialog)
        self.textItemList.append(item)
        self.viewLayout.addWidget(item)
        item.show()
        self._adjustViewSize()

    def _showConfirmDialog(self, item: TextItem) -> None:
        """
        显示确认对话框
        """
        from src.Ui.AddPage import AddWidget

        box = MessageBox(
            title=self.tr("Confirm"),
            content=self.tr(f"Are you sure you want to delete the following URLs?\n\n{item.text}"),
            parent=it(AddWidget),
        )
        box.yesSignal.connect(lambda: self._removeUrl(item))
        box.exec()

    def _removeUrl(self, item: TextItem):
        """
        移除 Url Item
        """
        if item.text not in self.texts:
            return

        self.texts.remove(item.text)
        self.viewLayout.removeWidget(item)
        item.deleteLater()
        self._adjustViewSize()

        if not len(self.texts):
            self.card.expandButton.clicked.emit()
            self.card.expandButton.setEnabled(False)
            self.emptiedSignal.emit()

    def wheelEvent(self, event) -> None:
        if self.isExpand:
            # 如果是展开状态则将滚轮事件向上传递
            self.parent().wheelEvent(event)


class TextInputBox(MessageBoxBase):

    def __init__(self, parent=None):
        super().__init__(parent)
        self.titleLabel = TitleLabel(self.tr("Enter"), self)
        self.urlLineEdit = LineEdit()

        self.urlLineEdit.setPlaceholderText(self.tr("Enter..."))
        self.urlLineEdit.setClearButtonEnabled(True)

        # 将组件添加到布局中
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.urlLineEdit)

        # 设置对话框最小宽度
        self.widget.setMinimumWidth(350)
