# -*- coding: utf-8 -*-

"""
Bot 卡片
"""
from __future__ import annotations
import psutil

# 第三方库导入
import httpx
from qfluentwidgets import (
    CaptionLabel,
    FlowLayout,
    FluentIcon,
    FluentIconBase,
    HeaderCardWidget,
    IconWidget,
    ImageLabel,
    PillPushButton,
    ToolTipFilter,
    TransparentDropDownToolButton,
    TransparentPushButton,
    setFont,
)
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
    QProcess,
    QTimer
)
from PySide6.QtGui import QColor, QEnterEvent, QPixmap
from PySide6.QtWidgets import QAbstractButton, QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import Config, ConnectConfig
from src.core.network.urls import Urls
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import error_bar
from src.core.utils.run_napcat import manager_process
from time import monotonic


class BotCard(HeaderCardWidget):
    """Bot 卡片 Widget"""

    config: Config

    class ButtonGroup(QWidget):
        """按钮组, 当一个按钮点击后, 另一个按钮隐藏"""

        def __init__(self, btn_1: QAbstractButton, btn_2: QAbstractButton, parent: BotCard) -> None:
            super().__init__(parent)
            # 属性
            self.parent_bot_card = parent

            # 创建控件
            self._btn_1 = btn_1
            self._btn_2 = btn_2

            # 设置控件
            self._btn_2.hide()

            # 设置布局
            self.h_box_layout = QHBoxLayout(self)
            self.h_box_layout.setContentsMargins(0, 0, 0, 0)
            self.h_box_layout.setSpacing(8)
            self.h_box_layout.addWidget(self._btn_1)
            self.h_box_layout.addWidget(self._btn_2)

            # 链接信号
            self._btn_1.clicked.connect(self.slot_run_button)
            self._btn_2.clicked.connect(self.slot_stop_button)
            manager_process.process_changed_signal.connect(self.slot_process_changed_button)

        # =================== 槽函数 ====================
        def slot_run_button(self) -> None:
            """处理运行按钮点击"""
            manager_process.create_napcat_process(self.parent_bot_card.config)
        
        def slot_stop_button(self) -> None:
            """处理停止按钮点击"""
            manager_process.stop_process(str(self.parent_bot_card.config.bot.QQID))
        
        def slot_process_changed_button(self, qq_id: str, state: QProcess.ProcessState) -> None:
            """处理 NapCatQQ 进程变化时, 切换按钮显示

            Args:
                qq_id (str): QQ 号
                state (QProcess.ProcessState): 进程状态
            """
            if qq_id != str(self.parent_bot_card.config.bot.QQID):
                return
            
            if state == QProcess.ProcessState.Running:
                self._btn_1.hide()
                self._btn_2.show()
            else:
                self._btn_2.hide()
                self._btn_1.show()
            

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
        self.info_widget = BotInfoWidget(self.config, self)

        self.menu_button = TransparentDropDownToolButton(FluentIcon.MENU, self)
        self.run_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"), self)
        self.stop_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("停止"), self)
        self.run_button_group = self.ButtonGroup(self.run_button, self.stop_button, self)

        # 设置控件
        self.setTitle(f"{self.config.bot.name} ({self.config.bot.QQID})")
        self.setFixedSize(500, 240)

        # 设置布局
        self.viewLayout.addWidget(self.avatar_widget, 1)
        self.viewLayout.addWidget(self.info_widget, 2)

        self.headerLayout.addStretch(1)
        self.headerLayout.addWidget(self.run_button_group)
        self.headerLayout.addWidget(self.menu_button)


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
        self.image_label = ImageLabel(self)

        # 设置控件
        self.image_label.setImage(StaticIcon.LOGO.path())
        self.image_label.scaledToWidth(128)
        self.image_label.setBorderRadius(8, 8, 8, 8)

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
                self.image_label.setImage(pixmap),
                self.image_label.scaledToWidth(128),
                self.image_label.setBorderRadius(8, 8, 8, 8),
            )
        )

        QThreadPool.globalInstance().start(worker)


class BotInfoWidget(QWidget):
    """Bot 信息展示控件"""

    class InfoWidget(QWidget):

        def __init__(self, icon: FluentIconBase, text: str, parent: BotInfoWidget) -> None:
            super().__init__(parent)
            # 设置属性
            self._icon = icon.colored(QColor("#454655"), QColor("#fff3fa"))

            # 创建控件
            self.icon_widget = IconWidget(self._icon, self)
            self.text_label = CaptionLabel(text, self)

            # 设置控件
            self.icon_widget.setFixedSize(16, 16)
            setFont(self.text_label, 16)

            # 设置布局
            self.h_box_layout = QHBoxLayout(self)
            self.h_box_layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
            self.h_box_layout.setContentsMargins(0, 0, 0, 0)
            self.h_box_layout.setSpacing(8)
            self.h_box_layout.addWidget(self.icon_widget)
            self.h_box_layout.addWidget(self.text_label)

    class TagWidget(QWidget):

        def __init__(self, connect_config: ConnectConfig, parent: BotInfoWidget) -> None:
            super().__init__(parent)
            # 创建控件
            self.icon_widget = IconWidget(FluentIcon.TAG, self)
            self.h_box_layout = QHBoxLayout(self)
            self.flow_layout = FlowLayout()

            mapping = [
                ("HTTPC", connect_config.httpClients),
                ("HTTPS", connect_config.httpServers),
                ("SSE", connect_config.httpSseServers),
                ("WSC", connect_config.websocketClients),
                ("WSS", connect_config.websocketServers),
            ]

            for label, items in mapping:
                if items:
                    tag = PillPushButton(label, self)
                    tag.setFixedHeight(22)
                    tag.setCheckable(False)
                    self.flow_layout.addWidget(tag)

            # 设置控件
            self.icon_widget.setFixedSize(16, 16)

            # 设置布局
            self.flow_layout.setContentsMargins(0, 0, 0, 0)
            self.flow_layout.setSpacing(2)
            self.h_box_layout.setContentsMargins(0, 0, 0, 0)
            self.h_box_layout.setSpacing(8)
            self.h_box_layout.addWidget(self.icon_widget)
            self.h_box_layout.addLayout(self.flow_layout)

    def __init__(self, config: Config, parent: BotCard) -> None:
        super().__init__(parent)
        # 设置属性
        self._config = config

        # 创建控件
        self._run_time_info = self.InfoWidget(FluentIcon.DATE_TIME, f"未运行", self)
        self._memory_info = self.InfoWidget(FluentIcon.CALENDAR, f"-M / -M", self)
        self._tag_info = self.TagWidget(self._config.connect, self)

        # 设置布局
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(8)
        self.v_box_layout.addWidget(self._run_time_info)
        self.v_box_layout.addWidget(self._memory_info)
        self.v_box_layout.addWidget(self._tag_info)
        self.v_box_layout.addStretch(1)

        # 调用方法
        self.setup_tooltip()

        # 链接信号
        manager_process.process_changed_signal.connect(self.slot_run_time_start)
        manager_process.process_changed_signal.connect(self.slot_memory_usage_start)

    def setup_tooltip(self) -> None:
        """设置工具提示"""
        self._run_time_info.setToolTip(self.tr("运行时长"))
        self._memory_info.setToolTip(self.tr("内存占用"))
        self._tag_info.setToolTip(self.tr("网络类型"))

        for i in range(self.v_box_layout.count()):
            item = self.v_box_layout.itemAt(i)

            if widget := item.widget():
                widget.setToolTipDuration(1000)
                widget.installEventFilter(ToolTipFilter(widget, showDelay=300))

    # =================== 槽函数 ====================
    def slot_run_time_start(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """处理运行时长开始更新槽函数"""
        if qq_id != str(self._config.bot.QQID):
            return

        if state == QProcess.ProcessState.Running:
            # 获取当前时间作为起始时间
            self.start_time = monotonic()

            # 检查是否已有计时器在运行
            if hasattr(self, "_timer"):
                self._run_time_timer.stop()
                self._run_time_timer.deleteLater()

            # 创建新的计时器
            timer = QTimer(self)
            # 每秒更新一次运行时长显示
            timer.timeout.connect(lambda: self._run_time_info.text_label.setText(
                f"{int(monotonic() - self.start_time)//3600:02}:{(int(monotonic() - self.start_time)%3600)//60:02}:{int(monotonic() - self.start_time)%60:02}"
            ))
            timer.start(1000)
            # 保存计时器引用
            self._run_time_timer = timer
        else:
            if hasattr(self, "_run_time_timer"):
                self._run_time_timer.stop()
                self._run_time_timer.deleteLater()
                del self._run_time_timer

            self._run_time_info.text_label.setText("未运行")

    def slot_memory_usage_start(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """处理内存占用开始更新槽函数"""
        if qq_id != str(self._config.bot.QQID):
            return

        if state == QProcess.ProcessState.Running:
            # 检查是否已有计时器在运行
            if hasattr(self, "_memory_timer"):
                self._memory_timer.stop()
                self._memory_timer.deleteLater()

            # 创建新的计时器
            timer = QTimer(self)
            # 每秒更新一次内存占用显示
            timer.timeout.connect(lambda: self._memory_info.text_label.setText(
                f"{manager_process.get_memory_usage(qq_id)} MB / {int(psutil.virtual_memory().total / (1024 * 1024))} MB"
            ))
            timer.start(1000)
            # 保存计时器引用
            self._memory_timer = timer
        else:
            if hasattr(self, "_memory_timer"):
                self._memory_timer.stop()
                self._memory_timer.deleteLater()
                del self._memory_timer

            self._memory_info.text_label.setText("-M / -M")
