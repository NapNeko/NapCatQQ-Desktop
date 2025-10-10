# -*- coding: utf-8 -*-

"""
Bot 卡片
"""

# 第三方库导入
import httpx
from qfluentwidgets import HeaderCardWidget, ImageLabel
from PySide6.QtCore import (
    QEasingCurve,
    QEvent,
    QObject,
    QPoint,
    QPropertyAnimation,
    QRunnable,
    Qt,
    QThreadPool,
    QUrlQuery,
    Signal,
)
from PySide6.QtGui import QEnterEvent, QPixmap
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.network.urls import Urls
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import error_bar


class BotCard(HeaderCardWidget):
    """Bot 卡片 Widget"""

    config: Config

    def __init__(self, bot_config: Config, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(parent)

        # 设置属性
        self.config = bot_config

        # 创建控件
        self.avatar_widget = BotAvatarWidget(int(self.config.bot.QQID), self)

        # 设置控件
        self.setTitle(self.config.bot.name)
        self.setFixedSize(500, 240)

        # 设置布局
        self.viewLayout.addWidget(self.avatar_widget, 1)
        self.viewLayout.addWidget(QWidget(), 2)


class BotAvatarWidget(QWidget):
    """Bot 头像展示控件

    封装了获取头像的功能, 便于维护
    """

    class GetAvatarWoker(QObject, QRunnable):
        """使用 QRunnable 异步获取头像"""

        avatar_pixmap_signal = Signal(QPixmap)

        def __init__(self, qq_id: int) -> None:
            QObject.__init__(self)
            QRunnable.__init__(self)
            # 解析出对应的头像 URL
            url = Urls.QQ_AVATAR.value
            query = QUrlQuery()
            query.addQueryItem("spec", "640")
            query.addQueryItem("dst_uin", str(qq_id))
            url.setQuery(query)

            # 设置属性
            self._qq_id = qq_id
            self._url = url

        def run(self) -> None:
            """通过 httpx 获取头像数据, 然后包装成 QPixmap"""
            try:
                pixmap = QPixmap()
                pixmap.loadFromData(httpx.get(self._url.toString()).content)
                self.avatar_pixmap_signal.emit(pixmap)

            except httpx.HTTPStatusError | httpx.RequestError | httpx.TimeoutException as e:
                error_bar(
                    f"请求头像时发生错误!\n"
                    f"  - QQ号: {self._qq_id}\n"
                    f"  - 错误类型: {e.__class__.__name__}\n"
                    f"  - 错误信息: {e}"
                )

    def __init__(self, qq_id: int, parent: BotCard) -> None:
        super().__init__(parent)
        # 创建控件
        self.imageLabel = ImageLabel(self)

        # 设置控件
        self.imageLabel.setImage(StaticIcon.LOGO.path())
        self.imageLabel.scaledToWidth(128)
        self.imageLabel.setBorderRadius(8, 8, 8, 8)
        self.raise_()

        # 设置属性
        self.qq_id = qq_id

        # 调用方法
        self.init_animation()

    # ==================== 动画方法 ====================
    def init_animation(self) -> None:
        """创建一个简单的浮动动画"""
        self._float_ani = QPropertyAnimation(self, b"pos")
        self._float_ani.setDuration(200)
        self._float_ani.setEasingCurve(QEasingCurve.Type.InQuad)

        # 存储原始位置
        self._original_pos = QPoint(self.pos().x() + 24, self.pos().y() + 24)

    def enterEvent(self, event: QEnterEvent) -> None:
        """重写进入事件以实现动画方法"""
        # 保存当前位置作为起点
        current_pos = self.pos()

        # 设置动画, 向上移动 10 个像素
        target_pos = QPoint(current_pos.x(), current_pos.y() - 10)

        # 启动动画
        self._float_ani.setStartValue(current_pos)
        self._float_ani.setEndValue(target_pos)
        self._float_ani.start()

        super().enterEvent(event)

    def leaveEvent(self, event: QEvent) -> None:
        """重写离开事件以实现动画方法"""
        # 保存当前位置作为起点
        current_pos = self.pos()

        self._float_ani.setStartValue(current_pos)
        self._float_ani.setEndValue(self._original_pos)
        self._float_ani.start()

        super().leaveEvent(event)

    # ==================== 属性方法 ====================
    @property
    def qq_id(self) -> int:
        return self._qq_id

    @qq_id.setter
    def qq_id(self, value: int) -> None:
        self._qq_id = value

        worker = self.GetAvatarWoker(value)
        worker.avatar_pixmap_signal.connect(
            lambda pixmap: (
                self.imageLabel.setImage(pixmap),
                self.imageLabel.scaledToWidth(128),
                self.imageLabel.setBorderRadius(8, 8, 8, 8),
            )
        )

        QThreadPool.globalInstance().start(worker)


class BotInfoWidget(QWidget):
    """Bot 信息展示控件"""

    def __init__(self, parent: BotCard) -> None:
        super().__init__(parent)
