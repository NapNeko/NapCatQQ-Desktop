# -*- coding: utf-8 -*-
from typing import List

from PySide6.QtCore import Qt, QEasingCurve
from qfluentwidgets import LineEdit, BodyLabel, FluentIcon, SwitchButton, FluentIconBase, ExpandSettingCard
from PySide6.QtWidgets import QWidget, QHBoxLayout, QSizePolicy
from qfluentwidgets.components.settings.expand_setting_card import GroupSeparator


class ItemBase(QWidget):

    def __init__(self, title: str, parent=None) -> None:
        """
        ## 初始化 item

        ### 参数
            - parent: 父组件

        """
        super().__init__(parent=parent)
        self.label = BodyLabel(title, self)
        self.setFixedHeight(65)
        self.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Fixed)
        self.hBoxLayout = QHBoxLayout(self)

    def _setLayout(self, widget: LineEdit | SwitchButton) -> None:
        """
        ## 布局控件

        ### 参数
            - label item的内容
            - widget item的可操作控件,如输入框,开关等
        """

        self.hBoxLayout.setContentsMargins(48, 0, 60, 0)
        self.hBoxLayout.addWidget(self.label, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(widget, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)


class GroupCardBase(ExpandSettingCard):

    def __init__(self, icon: FluentIcon | FluentIconBase, title: str, content: str, parent=None) -> None:
        """
        ## 初始化卡片

        ### 参数
            - icon: 卡片图标
            - title: 卡片标题
            - content: 卡片内容
            - parent: 父组件
        """
        super().__init__(icon, title, content, parent)
        self.itemList: List[ItemBase] = []

        # 调用方法
        self._initWidget()

    def _initWidget(self) -> None:
        """
        设置卡片内部控件
        """
        self.expandAni.setDuration(100)
        self.expandAni.setEasingCurve(QEasingCurve.Type.InQuint)

        # 初始化布局
        self.viewLayout.setSpacing(0)
        self.viewLayout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)

    def addItem(self, item: ItemBase) -> None:
        """
        添加 Item
        """
        self.itemList.append(item)
        self.viewLayout.addWidget(item)

        if self.viewLayout.count() >= 1:
            self.viewLayout.addWidget(GroupSeparator(self.view))

        item.show()
        self._adjustViewSize()

    def wheelEvent(self, event) -> None:
        # 不知道为什么ExpandGroupSettingCard把wheelEvent屏蔽了
        # 检查是否在展开状态下，如果是，则传递滚轮事件给父级窗体
        if self.isExpand:
            self.parent().wheelEvent(event)
