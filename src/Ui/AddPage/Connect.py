# -*- coding: utf-8 -*-
# 标准库导入
import random

# 第三方库导入
from qfluentwidgets import BodyLabel, ImageLabel
from PySide6.QtGui import QMovie
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Core.Config.ConfigModel import ConnectConfig


class ConnectWidget(QStackedWidget):
    """
    ## 连接配置项对应的 QStackedWidget
    """

    def __init__(self, parent=None, config: ConnectConfig | None = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")

        if config is not None:
            # 如果传入了 config 则进行解析并填充内部卡片
            self.config = config
            self.fillValue()

        self._postInit()

    def _postInit(self) -> None:
        """初始化"""
        self.defultPage = DefaultPage(self)

        self.addWidget(self.defultPage)

        self.setCurrentWidget(self.defultPage)

    def fillValue(self) -> None:
        """
        ## 如果传入了 config 则对其内部卡片的值进行填充
        """
        ...

    def getValue(self) -> dict:
        """
        ## 返回内部卡片的配置结果
        """
        return {}

    def clearValues(self) -> None:
        """
        ## 清空(还原)内部卡片的配置
        """
        ...


class DefaultPage(QWidget):
    """无配置时的默认页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.images = [
            ":/FuFuFace/image/FuFuFace/g_fufu_01.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_02.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_03.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_04.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_05.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_06.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_07.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_08.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_09.gif",
            ":/FuFuFace/image/FuFuFace/g_fufu_10.gif",
        ]

        self.imageLabel = ImageLabel(self)
        self.movie = QMovie(random.choice(self.images))
        self.label = BodyLabel(self.tr("还没有添加配置项喔"), self)
        self.hBoxLayout = QVBoxLayout(self)

        self.imageLabel.setMovie(self.movie)
        self.imageLabel.scaledToWidth(self.width())

        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.imageLabel, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.addSpacing(12)
        self.hBoxLayout.addWidget(self.label, alignment=Qt.AlignmentFlag.AlignCenter)
        self.hBoxLayout.addStretch(1)
