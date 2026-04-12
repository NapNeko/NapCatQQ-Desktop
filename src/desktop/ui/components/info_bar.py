# -*- coding: utf-8 -*-
"""
消息条工具模块

提供统一的消息条创建函数, 确保程序内消息提示风格一致且代码简洁。

消息条位置规则：
- 右下角: 信息提示 (`info_bar`), 成功提示 (`success_bar`)
- 右上角: 警告提示 (`warning_bar`), 错误提示 (`error_bar`)
"""

# 第三方库导入
from qfluentwidgets import InfoBar
from PySide6.QtCore import QObject, Qt
from PySide6.QtWidgets import QWidget
from typing import Any, cast

# 项目内模块导入
from src.ui.components.managers import NCDInfoBarPosition


def _position(position: NCDInfoBarPosition) -> Any:
    """将自定义 InfoBar 位置枚举转换为 qfluentwidgets 可接受的类型。"""
    return cast(object, position)


def _resolve_parent(parent: QObject | None) -> QWidget:
    """统一将消息条父级收敛到顶层窗口，避免子页面和主窗口各自堆叠。"""
    if isinstance(parent, QWidget):
        window = parent.window()
        if isinstance(window, QWidget):
            return window
        return parent

    from src.ui.window.main_window.window import MainWindow
    from creart import it

    return it(MainWindow)


def info_bar(content: str, title: str = "Tips✨", duration: int = 5000, parent: QObject | None = None) -> None:
    """
    创建信息提示消息条

    用于展示一般性提示信息, 显示时间较短。

    Args:
        title: 消息条标题, 建议简洁明了
        content: 消息条内容文本
        duration: 消息条显示时长(毫秒), 默认5000毫秒(5秒)
        parent: 父级组件, 如为None则使用主窗口作为父级
    """
    # 延迟导入以避免循环依赖
    # 项目内模块导入
    InfoBar.info(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=_position(NCDInfoBarPosition.BOTTOM_RIGHT),
        parent=_resolve_parent(parent),
    )


def success_bar(content: str, title: str = "Success✅", duration: int = 5000, parent: QObject | None = None) -> None:
    """
    创建成功提示消息条

    用于展示操作成功提示, 显示时间较短。

    Args:
        title: 消息条标题, 建议简洁明了
        content: 消息条内容文本
        duration: 消息条显示时长(毫秒), 默认5000毫秒(5秒)
        parent: 父级组件, 如为None则使用主窗口作为父级
    """
    # 项目内模块导入
    InfoBar.success(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=_position(NCDInfoBarPosition.BOTTOM_RIGHT),
        parent=_resolve_parent(parent),
    )


def warning_bar(content: str, title: str = "Warning⚠️", duration: int = 10000, parent: QObject | None = None) -> None:
    """
    创建警告提示消息条

    用于展示警告信息, 显示时间较长以便用户注意。

    Args:
        title: 消息条标题, 建议简洁明了
        content: 消息条内容文本
        duration: 消息条显示时长(毫秒), 默认10000毫秒(10秒)
        parent: 父级组件, 如为None则使用主窗口作为父级
    """
    # 项目内模块导入
    InfoBar.warning(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=_position(NCDInfoBarPosition.TOP_RIGHT),
        parent=_resolve_parent(parent),
    )


def error_bar(content: str, title: str = "Failed❌", duration: int = -1, parent: QObject | None = None) -> None:
    """
    创建错误提示消息条

    用于展示错误信息, 默认持续显示直到用户手动关闭。

    Args:
        title: 消息条标题, 建议简洁明了
        content: 消息条内容文本
        duration: 消息条显示时长(毫秒), 默认-1(持续显示)
        parent: 父级组件, 如为None则使用主窗口作为父级
    """
    # 项目内模块导入
    InfoBar.error(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=_position(NCDInfoBarPosition.TOP_RIGHT),
        parent=_resolve_parent(parent),
    )
