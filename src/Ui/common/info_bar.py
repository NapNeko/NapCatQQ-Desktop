# -*- coding: utf-8 -*-
"""
## 消息条创建函数
    本模块的目的是为了统一程序内消息条的创建，从而保持代码简洁。

### 消息条位置
    - 右下角: `info_bar`, `success_bar`
    - 右上角: `warning_bar`, `error_bar`
"""

# 第三方库导入
from qfluentwidgets import InfoBar
from PySide6.QtCore import Qt

# 项目内模块导入
from src.ui.common.managers import NCDInfoBarPosition


def info_bar(content: str, title: str = "Tips✨", duration: int = 5_000) -> None:
    """
    ## info 信息消息条, 仅用于展示一些提示, 故显示时间不会太长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 5 秒
    """
    # 项目内模块导入
    from src.ui.main_window.window import MainWindow

    InfoBar.info(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.BOTTOM_RIGHT,
        parent=MainWindow(),
    )


def success_bar(content: str, title: str = "Success✅", duration: int = 5_000) -> None:
    """
    ## success 信息消息条, 仅用于展示一些成功提示, 故显示时间不会太长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 5 秒
    """
    # 项目内模块导入
    from src.ui.main_window.window import MainWindow

    InfoBar.success(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.BOTTOM_RIGHT,
        parent=MainWindow(),
    )


def warning_bar(content: str, title: str = "Warning⚠️", duration: int = 10_000) -> None:
    """
    ## warning 信息消息条, 仅用于展示一些警告提示, 故显示时间稍长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 10 秒
    """
    # 项目内模块导入
    from src.ui.main_window.window import MainWindow

    InfoBar.warning(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.TOP_RIGHT,
        parent=MainWindow(),
    )


def error_bar(content: str, title: str = "Failed❌", duration: int = -1) -> None:
    """
    ## error 信息消息条, 仅用于展示一些警告提示, 故显示时间稍长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 ∞ 秒
    """
    # 项目内模块导入
    from src.ui.main_window.window import MainWindow

    InfoBar.error(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.TOP_RIGHT,
        parent=MainWindow(),
    )
