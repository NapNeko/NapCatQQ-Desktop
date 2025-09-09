# -*- coding: utf-8 -*-
"""
UI 组件位置调整模块

提供自定义的 InfoBar 位置管理器，用于控制消息条在界面中的显示位置。
包含六种不同的位置枚举和对应的管理器实现。

## 注意
- 方法 `_slideStartPos` 保持驼峰命名，因为它是重写父类的方法，需要保持方法签名一致。
- 其他命名遵循项目的 snake_case 规范。
"""

# 标准库导入
from enum import Enum

# 第三方库导入
from qfluentwidgets import InfoBar, InfoBarManager
from PySide6.QtCore import QPoint, QSize


class NCDInfoBarPosition(Enum):
    """InfoBar 位置枚举"""

    TOP = 0
    BOTTOM = 1
    TOP_LEFT = 2
    TOP_RIGHT = 3
    BOTTOM_LEFT = 4
    BOTTOM_RIGHT = 5
    NONE = 6


@InfoBarManager.register(NCDInfoBarPosition.TOP_LEFT)
class TopLeftInfoBarManager(InfoBarManager):
    """消息条左上方位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = self.margin + 64
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件左侧之外）
        """
        return QPoint(-info_bar.width(), self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.TOP)
class TopInfoBarManager(InfoBarManager):
    """消息条顶部居中位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = (parent_size.width() - info_bar.width() + 40) // 2
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（略微向上偏移）
        """
        pos = self._pos(info_bar)
        return QPoint(pos.x(), pos.y() - 16)


@InfoBarManager.register(NCDInfoBarPosition.TOP_RIGHT)
class TopRightInfoBarManager(InfoBarManager):
    """消息条右上方位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = parent_size.width() - info_bar.width() - self.margin
        y = self.margin + 42

        # 累加之前所有信息栏的高度和间距
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件右侧之外）
        """
        return QPoint(info_bar.parent().width(), self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_LEFT)
class BottomLeftInfoBarManager(InfoBarManager):
    """消息条左下方位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = self.margin + 64
        y = parent_size.height() - info_bar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置
        """
        return QPoint(self.margin + 64, self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM)
class BottomInfoBarManager(InfoBarManager):
    """消息条底部居中位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = (parent_size.width() - info_bar.width() + 40) // 2
        y = parent_size.height() - info_bar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（略微向下偏移）
        """
        pos = self._pos(info_bar)
        return QPoint(pos.x(), pos.y() + 16)


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_RIGHT)
class BottomRightInfoBarManager(InfoBarManager):
    """消息条右下方位置管理器"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            info_bar: 消息条实例
            parent_size: 消息条父组件大小，默认为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        x = parent_size.width() - info_bar.width() - self.margin
        y = parent_size.height() - info_bar.height() - self.margin

        # 累减之前所有信息栏的高度和间距（从底部向上堆叠）
        for bar in self.info_bars[parent][: self.info_bars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            info_bar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置（父组件右侧之外）
        """
        return QPoint(info_bar.parent().width(), self._pos(info_bar).y())
