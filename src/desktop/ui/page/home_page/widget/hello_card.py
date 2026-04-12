# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget

from src.core.config import cfg
from src.ui.common.icon import NapCatDesktopIcon, SvgStaticIcon
from src.ui.components.theme_color_label import ThemeColorTitleLabel, ThemeColorTone

from qfluentwidgets import BodyLabel, SimpleCardWidget
from qfluentwidgets.components.widgets.icon_widget import IconWidget


class HelloCard(SimpleCardWidget):

    def __init__(self) -> None:
        super().__init__()
        self._floating_icon_host: QWidget | None = None

        # 创建控件
        self.hello_label = ThemeColorTitleLabel("Hello!!", tone=ThemeColorTone.PRIMARY)
        self.welcome_label = BodyLabel(
            "欢迎来到主页! NapCatQQ Desktop 能够帮助你快速使用 NapCatQQ 的各种功能, 提升你的使用效率!"
        )
        self.good_icon_label = IconWidget(NapCatDesktopIcon.GOOD)
        self.good_content_label = BodyLabel("如果你喜欢这个软件, 请给个 Star 吧!")
        self.cat_girl_icon_label = IconWidget(SvgStaticIcon.CAT_GIRL.themed())
        self.cat_girl_placeholder = QWidget(self)

        # 设置控件
        self.setMinimumHeight(200)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.good_icon_label.setFixedSize(24, 24)
        self.welcome_label.setWordWrap(True)
        self.cat_girl_icon_label.setFixedSize(150, 225)
        self.cat_girl_icon_label.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.cat_girl_placeholder.setFixedWidth(120)

        # 设置布局
        self.good_h_box_layout = QHBoxLayout()
        self.good_h_box_layout.addWidget(self.good_icon_label)
        self.good_h_box_layout.addWidget(self.good_content_label)

        self.v_box_layout = QVBoxLayout()
        self.v_box_layout.addWidget(self.hello_label)
        self.v_box_layout.addWidget(self.welcome_label)
        self.v_box_layout.addLayout(self.good_h_box_layout)

        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.addLayout(self.v_box_layout)
        self.h_box_layout.addWidget(self.cat_girl_placeholder)
        self.h_box_layout.setStretch(0, 1)

        self.h_box_layout.setContentsMargins(32, 24, 24, 24)

        # 连接信号
        cfg.themeChanged.connect(lambda *_: self.cat_girl_icon_label.update())
        cfg.themeColorChanged.connect(lambda *_: self.cat_girl_icon_label.update())

    def attach_floating_icon(self, host: QWidget) -> None:
        """将猫娘图标挂到外层宿主上，实现超出卡片边界的悬浮效果。"""
        self._floating_icon_host = host
        self.cat_girl_icon_label.setParent(host)
        self.cat_girl_icon_label.show()
        self._update_floating_icon_position()

    def _update_floating_icon_position(self) -> None:
        if self._floating_icon_host is None:
            return

        card_top_right = self.mapTo(self._floating_icon_host, self.rect().topRight())
        x = card_top_right.x() - self.cat_girl_icon_label.width() - 16
        y = max(0, card_top_right.y() - 28)
        self.cat_girl_icon_label.move(x, y)
        self.cat_girl_icon_label.raise_()

    def moveEvent(self, event) -> None:
        super().moveEvent(event)
        self._update_floating_icon_position()

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self._update_floating_icon_position()

    def showEvent(self, event) -> None:
        super().showEvent(event)
        if self._floating_icon_host is not None:
            self.cat_girl_icon_label.show()
            self._update_floating_icon_position()

    def hideEvent(self, event) -> None:
        super().hideEvent(event)
        if self._floating_icon_host is not None:
            self.cat_girl_icon_label.hide()
