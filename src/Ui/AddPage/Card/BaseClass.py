# -*- coding: utf-8 -*-
from PySide6.QtCore import QEasingCurve
from PySide6.QtWidgets import QWidget, QHBoxLayout
from qfluentwidgets.common import FluentIconBase
from qfluentwidgets.components.settings import ExpandGroupSettingCard


class GroupCardBase(ExpandGroupSettingCard):

    def __init__(self, icon: FluentIconBase, title, content, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.expandAni.setDuration(100)
        self.expandAni.setEasingCurve(QEasingCurve.Type.InQuint)

    def add(self, label, widget) -> None:
        view = QWidget()

        layout = QHBoxLayout(view)
        layout.setContentsMargins(48, 12, 48, 12)

        layout.addWidget(label)
        layout.addStretch(1)
        layout.addWidget(widget)

        # 添加组件到设置卡
        self.addGroupWidget(view)

    def wheelEvent(self, event) -> None:
        # 不知道为什么ExpandGroupSettingCard把wheelEvent屏蔽了
        # 检查是否在展开状态下，如果是，则传递滚轮事件给父级窗体
        if self.isExpand:
            self.parent().wheelEvent(event)
        else:
            super().wheelEvent(event)

    def setExpand(self, isExpand: bool) -> None:
        """重写方法优化下性能"""
        if self.isExpand == isExpand:
            return

        self.isExpand = isExpand
        self.setProperty("isExpand", isExpand)
        self.setUpdatesEnabled(False)

        if isExpand:
            h = self.viewLayout.sizeHint().height()
            self.verticalScrollBar().setValue(h)
            self.expandAni.setStartValue(h)
            self.expandAni.setEndValue(0)
        else:
            self.expandAni.setStartValue(0)
            self.expandAni.setEndValue(self.verticalScrollBar().maximum())

        self.expandAni.start()

    def _onExpandValueChanged(self) -> None:
        vh = self.viewLayout.sizeHint().height()
        h = self.viewportMargins().top()
        self.setFixedHeight(max(h + vh - self.verticalScrollBar().value(), h))
        self.setUpdatesEnabled(True)
