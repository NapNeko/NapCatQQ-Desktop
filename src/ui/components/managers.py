# -*- coding: utf-8 -*-
"""
UI 组件位置调整模块

提供自定义的 InfoBar 位置管理器, 用于控制消息条在界面中的显示位置. 
包含六种不同的位置枚举和对应的管理器实现. 

## 注意
- 方法 `_slideStartPos` 保持驼峰命名, 因为它是重写父类的方法, 需要保持方法签名一致. 
- 属性 `infoBars` 保持驼峰命名, 因为它是继承自父类的属性, 需要保持一致. 
- 其他命名遵循项目的 snake_case 规范. 
"""

# 标准库导入
from enum import Enum
from typing import cast

# 第三方库导入
from qfluentwidgets import InfoBar, InfoBarManager
from PySide6.QtCore import QObject, QPoint, QSize
from PySide6.QtWidgets import QWidget


# ==================== Monkey-patch InfoBarManager 单例 bug ====================
# qfluentwidgets.InfoBarManager 把 ``_instance`` 写成 class-level 单例:
#
#     class InfoBarManager(QObject):
#         _instance = None
#         def __new__(cls, *args, **kwargs):
#             if cls._instance is None:
#                 cls._instance = super().__new__(cls, *args, **kwargs)
#             return cls._instance
#
# 由于 ``cls._instance`` 是 **class attribute**, 第一个被实例化的子类会"锁定"
# 后续所有 ``InfoBarManager.make(position)`` 的返回值: 即使传入 BOTTOM_RIGHT,
# 拿到的还是首次实例化时的 (例如 TopRight) 实例 - 于是 chip 全部按 TOP_RIGHT 算法
# 摆放, BOTTOM_RIGHT / BOTTOM 等设定形同虚设.
#
# 修复: 改为**按子类分别缓存**单例; 每个子类首次 ``make`` 时创建独立实例.
def _per_subclass_new(cls, *args, **kwargs):
    holder = InfoBarManager.__dict__.get("_subcls_instances")
    if holder is None:
        holder = {}
        InfoBarManager._subcls_instances = holder
    inst = holder.get(cls)
    if inst is None:
        inst = QObject.__new__(cls)
        holder[cls] = inst
        # 让原 ``__init__`` 的 ``_InfoBarManager__initialized`` 守卫失效
        # (该属性是 name-mangled 的 ``self.__initialized``).
        inst._InfoBarManager__initialized = False
    return inst


# 仅在尚未 patch 时替换, 避免热重载时重复 patch
if getattr(InfoBarManager.__new__, "__name__", "") != "_per_subclass_new":
    InfoBarManager.__new__ = _per_subclass_new


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
    """将 InfoBar 的父对象收窄为 QWidget. """
    return cast(QWidget, infoBar.parent())


@InfoBarManager.register(NCDInfoBarPosition.TOP_LEFT)
class TopLeftInfoBarManager(InfoBarManager):
    """消息条左上方位置管理器"""

    def _pos(self, infoBar: InfoBar, parentSize: QSize | None = None) -> QPoint:
        """
        计算消息条的最终位置

        Args:
            infoBar: 消息条实例
            parentSize: 消息条父组件大小, 可为 None

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
            QPoint: 信息栏的起始位置 (父组件左侧之外) 
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
            parentSize: 消息条父组件大小, 可为 None

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
            QPoint: 信息栏的起始位置 (略微向上偏移) 
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
            parentSize: 消息条父组件大小, 可为 None

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
            QPoint: 信息栏的起始位置 (父组件右侧之外) 
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
            parentSize: 消息条父组件大小, 可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = self.margin + 64
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距 (从底部向上堆叠) 
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
            parentSize: 消息条父组件大小, 可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = (parentSize.width() - infoBar.width() + 40) // 2
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距 (从底部向上堆叠) 
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置 (略微向下偏移) 
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
            parentSize: 消息条父组件大小, 可为 None

        Returns:
            QPoint: 消息条的位置坐标
        """
        parent = _parent_widget(infoBar)
        parentSize = parentSize or parent.size()

        x = parentSize.width() - infoBar.width() - self.margin
        y = parentSize.height() - infoBar.height() - self.margin

        # 累减之前所有信息栏的高度和间距 (从底部向上堆叠) 
        for bar in self.infoBars[parent][: self.infoBars[parent].index(infoBar)]:
            y -= bar.height() + self.spacing

        return QPoint(x, y)

    def _slideStartPos(self, infoBar: InfoBar) -> QPoint:
        """
        计算信息栏滑动动画的起始位置

        Args:
            infoBar: 要动画显示的信息栏对象

        Returns:
            QPoint: 信息栏的起始位置 (父组件右侧之外) 
        """
        return QPoint(_parent_widget(infoBar).width(), self._pos(infoBar).y())
