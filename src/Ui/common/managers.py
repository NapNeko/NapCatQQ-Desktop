# -*- coding: utf-8 -*-
"""
## UI 中一些组件位置调整

## InfoBarManager 消息条位置管理器
    - InfoBarPosition 位置枚举
    - TopLeftInfoBarManager 左上方位置
    - TopInfoBarManager 顶部位置
    - TopRightInfoBarManager 右上方位置
    - ButtonLeftInfoBarManager 左下方位置
    - ButtonInfoBarManager 下方位置
    - ButtonRightInfoBarManager 右下方位置
"""
from enum import Enum

from PySide6.QtCore import QSize, QPoint
from qfluentwidgets import InfoBar, InfoBarManager


class NCDInfoBarPosition(Enum):
    """程序 InfoBar 位置"""

    TOP = 0
    BOTTOM = 1
    TOP_LEFT = 2
    TOP_RIGHT = 3
    BOTTOM_LEFT = 4
    BOTTOM_RIGHT = 5
    NONE = 6


@InfoBarManager.register(NCDInfoBarPosition.TOP_LEFT)
class TopLeftInfoBarManager(InfoBarManager):
    """消息条左上方位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = self.margin + 64
        y = self.margin + 42

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        ## 计算信息栏滑动动画的起始位置

        ## 参数
            - info_bar: 要动画显示的信息栏对象

        ## 返回
            - QPoint: 信息栏的起始位置，在父组件右侧之外
        """
        return QPoint(-info_bar.width(), self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.TOP)
class TopInfoBarManager(InfoBarManager):
    """消息条顶部位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = (info_bar.parent().width() - info_bar.width() + 40) // 2
        y = self.margin + 42

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        pos = self._pos(info_bar)
        return QPoint(pos.x(), pos.y() - 16)


@InfoBarManager.register(NCDInfoBarPosition.TOP_RIGHT)
class TopRightInfoBarManager(InfoBarManager):
    """消息条右上方位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = parent_size.width() - info_bar.width() - self.margin
        y = self.margin + 42

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y。
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y += bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        ## 计算信息栏滑动动画的起始位置

        ## 参数
            - info_bar: 要动画显示的信息栏对象

        ## 返回
            - QPoint: 信息栏的起始位置，在父组件右侧之外
        """
        return QPoint(info_bar.parent().width(), self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_LEFT)
class ButtonLeftInfoBarManager(InfoBarManager):
    """消息条左下方位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = self.margin + 64
        y = parent_size.height() - info_bar.height() - self.margin

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        ## 计算信息栏滑动动画的起始位置

        ## 参数
            - info_bar: 要动画显示的信息栏对象

        ## 返回
            - QPoint: 信息栏的起始位置，在父组件右侧之外
        """
        return QPoint(self.margin + 64, self._pos(info_bar).y())


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM)
class ButtonInfoBarManager(InfoBarManager):
    """消息条下方位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = (parent_size.width() - info_bar.width() + 40) // 2
        y = parent_size.height() - info_bar.height() - self.margin

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        ## 计算信息栏滑动动画的起始位置

        ## 参数
            - info_bar: 要动画显示的信息栏对象

        ## 返回
            - QPoint: 信息栏的起始位置，在父组件右侧之外
        """
        pos = self._pos(info_bar)
        return QPoint(pos.x(), pos.y() + 16)


@InfoBarManager.register(NCDInfoBarPosition.BOTTOM_RIGHT)
class ButtonRightInfoBarManager(InfoBarManager):
    """消息条右下方位置"""

    def _pos(self, info_bar: InfoBar, parent_size: QSize = None) -> QPoint:
        """
        ## 调整消息条位置

        ## 参数
            - info_bar: 消息条实例
            - parent_size: 消息条父类大小

        ## 返回
            - QPoint: 位置信息
        """
        # 获取父类以及父类大小
        parent = info_bar.parent()
        parent_size = parent_size or parent.size()

        # 计算 InfoBar 的 x 以及 y 的位置
        x = parent_size.width() - info_bar.width() - self.margin
        y = parent_size.height() - info_bar.height() - self.margin

        # 对于每个先前的信息栏，加上它们的高度和间距累加到 y
        # 这样可以确保每个信息栏都位于前一个信息栏的正下方，并保持一致的间隔
        for bar in self.infoBars[parent][0 : self.infoBars[parent].index(info_bar)]:
            y -= bar.height() + self.spacing

        # 返回位置信息
        return QPoint(x, y)

    def _slideStartPos(self, info_bar: InfoBar) -> QPoint:
        """
        ## 计算信息栏滑动动画的起始位置

        ## 参数
            - info_bar: 要动画显示的信息栏对象

        ## 返回
            - QPoint: 信息栏的起始位置，在父组件右侧之外
        """
        return QPoint(info_bar.parent().width(), self._pos(info_bar).y())
