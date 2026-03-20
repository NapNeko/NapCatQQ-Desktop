# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import BodyLabel, FluentIcon, FluentIconBase, LineEdit, SwitchButton
from qfluentwidgets.components.settings.expand_setting_card import GroupSeparator
from PySide6.QtCore import QEasingCurve, Qt
from PySide6.QtGui import QIcon, QWheelEvent
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QWidget
from src.ui.common.card_surface import OpaqueExpandSettingCard


class ItemBase(QWidget):

    def __init__(self, title: str, parent: QWidget | None = None) -> None:
        """初始化

        Args:
            title (str): item的标题
            parent (QWidget | None): 父控件，可为 None。
        """
        super().__init__(parent=parent)
        self.label = BodyLabel(title, self)
        self.setFixedHeight(65)
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Fixed)
        self.hBoxLayout = QHBoxLayout(self)

    def _set_layout(self, widget: LineEdit | SwitchButton) -> None:
        """设置布局

        Args:
            widget (LineEdit | SwitchButton): item右侧的控件
        """

        self.hBoxLayout.setContentsMargins(48, 0, 60, 0)
        self.hBoxLayout.addWidget(self.label, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(widget, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)


class GroupCardBase(OpaqueExpandSettingCard):

    def __init__(
        self, icon: FluentIcon | FluentIconBase, title: str, content: str, parent: QWidget | None = None
    ) -> None:
        """初始化

        Args:
            icon (FluentIcon | FluentIconBase): 图标
            title (str): 标题
            content (str): 内容
            parent (QWidget | None): 父控件，可为 None。
        """
        card_icon = icon if isinstance(icon, FluentIcon) else QIcon(icon.path())
        super().__init__(card_icon, title, content, parent)
        self.itemList: list[ItemBase] = []

        # 调用方法
        self._setup_ui()

    def _setup_ui(self) -> None:
        """
        设置卡片内部控件
        """
        self.expandAni.setDuration(100)
        self.expandAni.setEasingCurve(QEasingCurve.Type.InQuint)

        # 初始化布局
        self.viewLayout.setSpacing(0)
        self.viewLayout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)

    def add_item(self, item: ItemBase) -> None:
        """添加 Item"""
        self.itemList.append(item)
        self.viewLayout.addWidget(item)

        if self.viewLayout.count() >= 1:
            self.viewLayout.addWidget(GroupSeparator(self.view))

        item.show()
        self._adjustViewSize()

    def wheelEvent(self, e: QWheelEvent) -> None:
        """重写滚轮事件"""
        # 不知道为什么ExpandGroupSettingCard把wheelEvent屏蔽了
        # 检查是否在展开状态下，如果是，则传递滚轮事件给父级窗体
        if self.isExpand:
            parent = self.parentWidget()
            if parent is not None:
                parent.wheelEvent(e)
                return

        super().wheelEvent(e)
