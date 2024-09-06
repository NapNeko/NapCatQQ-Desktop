# -*- coding: utf-8 -*-
"""
## 消息条创建函数
    本模块的目的是为了统一程序内消息条的创建，从而保持代码简洁。

### 消息条位置
    - 右下角: `info_bar`, `success_bar`
    - 右上角: `warning_bar`, `error_bar`
"""

from creart import it
from PySide6.QtCore import Qt
from qfluentwidgets import InfoBar

from src.Ui.common.managers import NCDInfoBarPosition


def info_bar(title: str, content: str, duration: int = 5_000) -> None:
    """
    ## info 信息消息条, 仅用于展示一些提示, 故显示时间不会太长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 5 秒
    """
    from src.Ui.MainWindow.Window import MainWindow

    InfoBar.info(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.BOTTOM_RIGHT,
        parent=it(MainWindow),
    )


def success_bar(title: str, content: str, duration: int = 5_000) -> None:
    """
    ## success 信息消息条, 仅用于展示一些成功提示, 故显示时间不会太长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 5 秒
    """
    from src.Ui.MainWindow.Window import MainWindow

    InfoBar.success(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.BOTTOM_RIGHT,
        parent=it(MainWindow),
    )


def warning_bar(title: str, content: str, duration: int = 10_000) -> None:
    """
    ## warning 信息消息条, 仅用于展示一些警告提示, 故显示时间稍长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 10 秒
    """
    from src.Ui.MainWindow.Window import MainWindow

    InfoBar.warning(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.TOP_RIGHT,
        parent=it(MainWindow),
    )


def error_bar(title: str, content: str, duration: int = 0) -> None:
    """
    ## error 信息消息条, 仅用于展示一些警告提示, 故显示时间稍长

    ## 参数
        - title: 消息条的标题, 不建议太长
        - content: 消息条内容
        - duration: 消息条显示时间, 默认 ∞ 秒
    """
    from src.Ui.MainWindow.Window import MainWindow

    InfoBar.error(
        title=title,
        content=content,
        orient=Qt.Orientation.Vertical,
        duration=duration,
        position=NCDInfoBarPosition.TOP_RIGHT,
        parent=it(MainWindow),
    )
