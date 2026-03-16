# -*- coding: utf-8 -*-
"""
UI 组件位置调整模块

提供自定义的 InfoBar 位置管理器，用于控制消息条在界面中的显示位置。
包含六种不同的位置枚举和对应的管理器实现。

## 注意
- 方法 `_slideStartPos` 保持驼峰命名，因为它是重写父类的方法，需要保持方法签名一致。
- 属性 `infoBars` 保持驼峰命名，因为它是继承自父类的属性，需要保持一致。
- 其他命名遵循项目的 snake_case 规范。
"""

# 标准库导入
from enum import Enum
from typing import cast

# 第三方库导入
from qfluentwidgets import InfoBar, InfoBarManager
from PySide6.QtCore import QPoint, QSize
from PySide6.QtWidgets import QWidget


class NCDInfoBarPosition(Enum):
    """InfoBar 位置枚举"""

    TOP = 0
    BOTTOM = 1
    TOP_LEFT = 2
    TOP_RIGHT = 3
    BOTTOM_LEFT = 4
    BOTTOM_RIGHT = 5
    NONE = 6


def _parent_widget(infoBar: InfoBar) -> QWidget:
    """将 InfoBar 的父对象收窄为 QWidget。"""
    return cast(QWidget, infoBar.parent())


@InfoBarManager.register(NCDInfoBarPosition.TOP_LEFT)
class TopLeftInfoBarManager(InfoBarManager):
    """消息条左上方位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = self.margin + 64
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件左侧之外）
        """
        return QPoint(-infoBar.width(), self._pos(infoBar).y())


@InfoBarManager.register(NCDInfoBarPosition.TOP)
class TopInfoBarManager(InfoBarManager):
    """消息条顶部居中位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = (parentSize.width() - infoBar.width() + 40) // 2
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（略微向上偏移）
        """
        pos = self._pos(infoBar)
        return QPoint(pos.x(), pos.y() - 16)


@InfoBarManager.register(NCDInfoBarPosition.TOP_RIGHT)
class TopRightInfoBarManager(InfoBarManager):
    """消息条右上方位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = parentSize.width() - infoBar.width() - self.margin
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件右侧之外）
        """
        return QPoint(_parent_widget(infoBar).width(), self._pos(infoBar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_LEFT)
class BottomLeftInfoBarManager(InfoBarManager):
    """消息条左下方位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = self.margin + 64
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置
        """
        return QPoint(self.margin + 64, self._pos(infoBar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM)
class BottomInfoBarManager(InfoBarManager):
    """消息条底部居中位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = (parentSize.width() - infoBar.width() + 40) // 2
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（略微向下偏移）
        """
        pos = self._pos(infoBar)
        return QPoint(pos.x(), pos.y() + 16)


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_RIGHT)
class BottomRightInfoBarManager(InfoBarManager):
    """消息条右下方位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小，可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = parentSize.width() - infoBar.width() - self.margin
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件右侧之外）
        """
        return QPoint(_parent_widget(infoBar).width(), self._pos(infoBar).y())
