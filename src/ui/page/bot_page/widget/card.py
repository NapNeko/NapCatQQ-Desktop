# -*- coding: utf-8 -*-

"""
Bot 卡片
"""
from __future__ import annotations

# 标准库导入
from time import monotonic
from typing import Optional, cast

# 第三方库导入
import httpx
import psutil
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FlowLayout,
    FluentIcon,
    FluentIconBase,
    HeaderCardWidget,
    IconWidget,
    ImageLabel,
    PillPushButton,
    PushButton,
    TeachingTip,
    TeachingTipTailPosition,
    TeachingTipView,
    ToolTipFilter,
    TransparentPushButton,
    TransparentToolButton,
    setFont,
)
from PySide6.QtCore import (
    QEasingCurve,
    QEvent,
    QObject,
    QPoint,
    QProcess,
    QPropertyAnimation,
    QRunnable,
    Qt,
    QThreadPool,
    QTimer,
    QUrlQuery,
    Signal,
)
from PySide6.QtGui import QColor, QEnterEvent, QFont, QPixmap
from PySide6.QtWidgets import QGridLayout, QHBoxLayout, QVBoxLayout, QWidget
from creart import it

# 项目内模块导入
from src.core.config.config_model import (
    Config,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.core.network.urls import Urls
from src.core.utils.run_napcat import ManagerNapCatQQProcess, ManagerAutoRestartProcess
from src.ui.common.icon import StaticIcon, NapCatDesktopIcon
from src.ui.components.info_bar import error_bar
from src.ui.components.message_box import AskBox
from src.ui.page.bot_page.widget.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)


class BotCard(HeaderCardWidget):
    """Bot 卡片 Widget"""

    # 当自身被移除时发出信号 值为QQID
    remove_signal = Signal(str)

    def __init__(self, config: Config, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(parent)

        # 设置属性
        self._config = config

        # 创建控件
        self.avatar_widget = BotAvatarWidget(int(self._config.bot.QQID), self)
        self.info_widget = BotInfoWidget(self._config, self)
        self.run_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"), self)
        self.stop_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("停止"), self)
        self.log_button = TransparentToolButton(NapCatDesktopIcon.LOG, self)
        self.setting_button = TransparentToolButton(FluentIcon.SETTING, self)
        self.remove_button = TransparentToolButton(FluentIcon.DELETE, self)

        # 设置控件
        self.setTitle(f"{self._config.bot.name} ({self._config.bot.QQID})")
        self.setFixedSize(500, 240)
        self.stop_button.hide()
        self.log_button.hide()

        # 设置布局
        self.viewLayout.addWidget(self.avatar_widget, 1)
        self.viewLayout.addWidget(self.info_widget, 2)

        self.headerLayout.addStretch(1)
        self.headerLayout.addWidget(self.run_button)
        self.headerLayout.addWidget(self.stop_button)
        self.headerLayout.addWidget(self.log_button)
        self.headerLayout.addWidget(self.setting_button)
        self.headerLayout.addWidget(self.remove_button)

        # 链接信号
        it(ManagerNapCatQQProcess).process_changed_signal.connect(self.slot_process_changed_button)
        self.run_button.clicked.connect(self.slot_run_button)
        self.stop_button.clicked.connect(self.slot_stop_button)
        self.log_button.clicked.connect(self.slot_log_button)
        self.setting_button.clicked.connect(self.slot_setting_button)
        self.remove_button.clicked.connect(self.slot_remove_button)

        # 调用方法
        self.set_tooltip()

    # ==================== 公共方法 ==================
    def update_info_card(self) -> None:
        """更新信息卡片显示内容， 用于外部调用，刷新后调用"""
        if (process := it(ManagerNapCatQQProcess).get_process(str(self._config.bot.QQID))) is None:
            return

        if (process := process.process) and process.state() == QProcess.ProcessState.Running:
            self.slot_process_changed_button(str(self._config.bot.QQID), QProcess.ProcessState.Running)
            self.info_widget.slot_run_time_start(str(self._config.bot.QQID), QProcess.ProcessState.Running)
            self.info_widget.slot_memory_usage_start(str(self._config.bot.QQID), QProcess.ProcessState.Running)

    # ==================== UI方法 ====================
    def set_tooltip(self) -> None:
        """设置工具提示"""
        self.run_button.setToolTip(self.tr("启动 Bot"))
        self.stop_button.setToolTip(self.tr("停止 Bot"))
        self.log_button.setToolTip(self.tr("查看日志"))
        self.setting_button.setToolTip(self.tr("配置 Bot"))
        self.remove_button.setToolTip(self.tr("移除 Bot"))

        for button in [
            self.run_button,
            self.stop_button,
            self.log_button,
            self.setting_button,
            self.remove_button,
        ]:
            button.setToolTipDuration(1000)
            button.installEventFilter(ToolTipFilter(button, showDelay=300))

    # ==================== 槽函数 ====================
    def slot_run_button(self) -> None:
        """处理运行按钮点击"""
        it(ManagerNapCatQQProcess).create_napcat_process(self._config)

    def slot_stop_button(self) -> None:
        """处理停止按钮点击"""
        it(ManagerNapCatQQProcess).stop_process(str(self._config.bot.QQID))
        it(ManagerAutoRestartProcess).remove_auto_restart_timer(str(self._config.bot.QQID))

    def slot_process_changed_button(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """处理 NapCatQQ 进程变化时, 切换按钮显示

        Args:
            qq_id (str): QQ 号
            state (QProcess.ProcessState): 进程状态
        """
        if qq_id != str(self._config.bot.QQID):
            return

        if state == QProcess.ProcessState.Running:
            self.run_button.hide()
            self.stop_button.show()
            self.log_button.show()
        else:
            self.run_button.show()
            self.stop_button.hide()
            self.log_button.hide()

    def slot_log_button(self) -> None:
        """处理日志按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        page = it(BotPage)
        page.view.setCurrentWidget(page.log_page)
        page.log_page.set_current_log_manager(self._config)

    def slot_setting_button(self) -> None:
        """处理配置按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        page = it(BotPage)
        page.view.setCurrentWidget(page.bot_config_page)
        page.bot_config_page.fill_config(self._config)

    def slot_remove_button(self) -> None:
        """处理移除自身槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if AskBox(
            self.tr("确认移除 Bot"),
            self.tr(
                f"确定要移除 Bot ({self._config.bot.QQID}) 吗？\n此操作无法恢复!"
            ),
            it(MainWindow),
        ).exec():
            self.stop_button.click()
            self.remove_signal.emit(str(self._config.bot.QQID))


class BotAvatarWidget(QWidget):
    """Bot 头像展示控件

    封装了获取头像的功能, 便于维护
    """

    class GetAvatarWoker(QObject, QRunnable):
        """使用 QRunnable 异步获取头像

        注意: 不在工作线程中创建/使用任何 GUI 对象(QPixmap/QWidget 等)。
        仅下载原始字节并通过信号传回主线程处理。
        """

        avatar_bytes_signal = Signal(bytes)

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
            """在工作线程中下载头像原始数据并通过信号发送"""
            try:
                resp = httpx.get(self._url.toString(), timeout=10.0)
                resp.raise_for_status()
                self.avatar_bytes_signal.emit(resp.content)

            except (httpx.HTTPStatusError, httpx.RequestError, httpx.TimeoutException) as e:
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
        # 在主线程中将字节转换为 QPixmap 并更新 UI，避免跨线程创建 GUI 对象
        worker.avatar_bytes_signal.connect(self._on_avatar_bytes)

        QThreadPool.globalInstance().start(worker)

    def _on_avatar_bytes(self, data: bytes) -> None:
        """将下载的头像字节转换为 QPixmap 并更新到 UI (主线程执行)"""
        pixmap = QPixmap()
        if pixmap.loadFromData(data):
            self.image_label.setImage(pixmap)
            self.image_label.scaledToWidth(128)
            self.image_label.setBorderRadius(8, 8, 8, 8)


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

            if self.flow_layout.count() == 0:
                self.hide()

            # 设置控件
            self.icon_widget.setFixedSize(16, 16)

            # 设置布局
            self.flow_layout.setContentsMargins(0, 0, 0, 0)
            self.flow_layout.setSpacing(2)
            self.h_box_layout.setContentsMargins(0, 0, 0, 0)
            self.h_box_layout.setSpacing(8)
            self.h_box_layout.addWidget(self.icon_widget, alignment=Qt.AlignmentFlag.AlignLeft)
            self.h_box_layout.addLayout(self.flow_layout, 1)

    def __init__(self, config: Config, parent: BotCard) -> None:
        super().__init__(parent)
        # 设置属性
        self._config = config
        self.start_time: Optional[float] = None

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
        it(ManagerNapCatQQProcess).process_changed_signal.connect(self.slot_run_time_start)
        it(ManagerNapCatQQProcess).process_changed_signal.connect(self.slot_memory_usage_start)

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
            # 判断 start_time 是否为 None, 为 None 代表第一次启动, 从 monotonic() 获取启动时间, 否则查找进程启动时间
            self.start_time = (
                it(ManagerNapCatQQProcess).get_process(qq_id).started_at if self.start_time is None else monotonic()
            )

            # 检查是否已有计时器在运行
            if hasattr(self, "_run_time_timer"):
                self._run_time_timer.stop()
                self._run_time_timer.deleteLater()

            # 创建新的计时器
            timer = QTimer(self)

            # 每秒更新一次运行时长显示 格式 00:00:00
            timer.timeout.connect(
                lambda: self._run_time_info.text_label.setText(
                    f"{int(monotonic() - self.start_time)//3600:02}:{(int(monotonic() - self.start_time)%3600)//60:02}:{int(monotonic() - self.start_time)%60:02}"
                )
            )
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
            timer.timeout.connect(
                lambda: self._memory_info.text_label.setText(
                    f"{it(ManagerNapCatQQProcess).get_memory_usage(qq_id)} MB / {int(psutil.virtual_memory().total / (1024 * 1024))} MB"
                )
            )
            timer.start(1000)
            # 保存计时器引用
            self._memory_timer = timer
        else:
            if hasattr(self, "_memory_timer"):
                self._memory_timer.stop()
                self._memory_timer.deleteLater()
                del self._memory_timer

            self._memory_info.text_label.setText("-M / -M")


class EnableTag(PillPushButton):
    """显示启用/禁用状态的标签控件"""

    def __init__(self, status: bool, parent: Optional[QObject] = None) -> None:
        """初始化启用标签

        Args:
            status: 初始状态，True 为启用，False 为禁用
            parent: 父控件
        """
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 8))
        self.update_status(status)

    def update_status(self, status: bool) -> None:
        """更新标签显示状态

        Args:
            status: 新的状态值，True 为启用，False 为禁用
        """
        if status:
            self.setText(self.tr("启用"))
        else:
            self.setText(self.tr("禁用"))


class FormateTag(PillPushButton):
    """消息格式显示标签控件"""

    def __init__(self, format_str: str, parent: Optional[QObject] = None) -> None:
        """初始化格式标签

        Args:
            format_str: 消息格式字符串
            parent: 父控件
        """
        super().__init__(parent)

        # 设置属性
        self.setFixedSize(48, 24)
        self.setCheckable(False)
        self.setFont(QFont(self.font().family(), 7))
        self.update_format(format_str)

    def update_format(self, format_str: str) -> None:
        """更新格式显示

        Args:
            format_str: 新的格式字符串
        """
        self.setText(format_str)


class ConfigCardBase(HeaderCardWidget):
    """配置卡片基类，提供通用的配置显示和操作功能"""

    remove_signal = Signal(NetworkBaseConfig)

    def __init__(self, config: NetworkBaseConfig, parent: Optional[QObject] = None) -> None:
        """初始化配置卡片基类

        Args:
            config: 配置数据模型
            parent: 父控件
        """
        super().__init__(parent)

        # 属性
        self.config = config
        self.config_view: Optional[QWidget] = None
        self.config_view_layout: Optional[QGridLayout] = None
        self.remove_button: Optional[TransparentToolButton] = None
        self.edit_button: Optional[TransparentToolButton] = None

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()

    def _create_widgets(self) -> None:
        """创建子控件"""
        self.edit_button = TransparentToolButton(FluentIcon.EDIT, self)
        self.remove_button = TransparentToolButton(FluentIcon.DELETE, self)
        self.config_view = QWidget(self)

    def _setup_layout(self) -> None:
        """设置控件布局"""
        self.setTitle(self.config.name)
        self.setFixedSize(335, 170)

        # 设置布局
        self.headerLayout.addStretch(1)
        self.headerLayout.addWidget(self.edit_button)
        self.headerLayout.addWidget(self.remove_button)
        self.viewLayout.addWidget(self.config_view)
        self.viewLayout.setContentsMargins(16, 16, 16, 16)

        self.config_view_layout = QGridLayout(self.config_view)
        self.config_view_layout.setContentsMargins(0, 0, 0, 0)
        self.config_view_layout.setVerticalSpacing(10)

    def _connect_signals(self) -> None:
        """连接信号与槽"""
        self.edit_button.clicked.connect(self._slot_edit_button_clicked)
        self.remove_button.clicked.connect(self._slot_remove_button_clicked)

    # ==================== 公共方法 ====================
    def fill_config(self) -> None:
        """填充配置数据显示 - 由子类实现"""
        raise NotImplementedError("子类必须实现 fill_value 方法")

    def get_config(self) -> NetworkBaseConfig:
        """获取配置数据

        Returns:
            NetworkBaseConfig: 配置数据模型
        """
        return self.config

    # ==================== 槽函数 ====================
    def _slot_remove_button_clicked(self) -> None:
        """处理删除按钮点击事件"""
        view = TeachingTipView(
            title=self.tr("删除配置"),
            content=self.tr("确定要删除该配置吗?这个操作不可逆!"),
            isClosable=False,
            tailPosition=TeachingTipTailPosition.TOP,
        )
        button = PushButton(self.tr("删除"), self)

        view.addWidget(button, align=Qt.AlignmentFlag.AlignRight)

        widget = TeachingTip.make(
            target=self.remove_button,
            view=view,
            duration=2000,
            tailPosition=TeachingTipTailPosition.BOTTOM,
            parent=self,
        )
        view.closed.connect(widget.close)
        button.clicked.connect(lambda: self.remove_signal.emit(self.config))

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件 - 由子类实现"""
        raise NotImplementedError("子类必须实现 _on_edit_button_clicked 方法")


class HttpServerConfigCard(ConfigCardBase):
    """HTTP 服务器配置卡片"""

    def __init__(self, config: HttpServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP 服务器配置卡片

        Args:
            config: HTTP 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.cors_label = BodyLabel(self.tr("CORS"), self)
        self.cors_config_label = EnableTag(self.config.enableCors, self)

        self.websocket_label = BodyLabel(self.tr("WS"), self)
        self.websocket_config_label = EnableTag(self.config.enableWebsocket, self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.cors_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.cors_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.websocket_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.websocket_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 2, 1, 1, 5)

    def fill_config(self) -> None:
        """填充 HTTP 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.cors_config_label.update_status(self.config.enableCors)
        self.websocket_config_label.update_status(self.config.enableWebsocket)
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)

    def get_config(self) -> HttpServersConfig:
        """获取 HTTP 服务器配置数据

        Returns:
            HttpServersConfig: HTTP 服务器配置
        """
        return cast(HttpServersConfig, self.config)

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpServerConfigDialog(it(MainWindow), cast(HttpServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class HttpSSEConfigCard(ConfigCardBase):
    """HTTP SSE 服务器配置卡片"""

    def __init__(self, config: HttpSseServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP SSE 服务器配置卡片

        Args:
            config: HTTP SSE 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.cors_label = BodyLabel(self.tr("CORS"), self)
        self.cors_config_label = EnableTag(self.config.enableCors, self)

        self.websocket_label = BodyLabel(self.tr("WS"), self)
        self.websocket_config_label = EnableTag(self.config.enableWebsocket, self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.cors_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.cors_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.websocket_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.websocket_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 4, 1, 2)

    def fill_config(self) -> None:
        """填充 HTTP SSE 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.cors_config_label.update_status(self.config.enableCors)
        self.websocket_config_label.update_status(self.config.enableWebsocket)
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_config(self) -> HttpSseServersConfig:
        """获取 HTTP SSE 服务器配置数据

        Returns:
            HttpSseServersConfig: HTTP SSE 服务器配置
        """
        return cast(HttpSseServersConfig, self.config)

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpSSEServerConfigDialog(it(MainWindow), cast(HttpSseServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class HttpClientConfigCard(ConfigCardBase):
    """HTTP 客户端配置卡片"""

    def __init__(self, config: HttpClientsConfig, parent: Optional[QObject] = None) -> None:
        """初始化 HTTP 客户端配置卡片

        Args:
            config: HTTP 客户端配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.url_label = BodyLabel(self.tr("URL"), self)
        self.url_config_label = BodyLabel(str(self.config.url), self)

        self.format_label = BodyLabel(self.tr("格式"), self)
        self.format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.url_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.url_config_label, 0, 1, 1, 6)

        self.config_view_layout.addWidget(self.format_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.format_config_label, 1, 1, 1, 1)

        self.config_view_layout.addWidget(self.report_self_message_label, 1, 4, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 1, 5, 1, 1)

    def fill_config(self) -> None:
        """填充 HTTP 客户端配置数据"""
        self.url_config_label.setText(str(self.config.url))
        self.format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_config(self) -> HttpClientsConfig:
        """获取 HTTP 客户端配置数据

        Returns:
            HttpClientsConfig: HTTP 客户端配置
        """
        return cast(HttpClientsConfig, self.config)

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = HttpClientConfigDialog(it(MainWindow), cast(HttpClientsConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class WebsocketServersConfigCard(ConfigCardBase):
    """WebSocket 服务器配置卡片"""

    def __init__(self, config: WebsocketServersConfig, parent: Optional[QObject] = None) -> None:
        """初始化 WebSocket 服务器配置卡片

        Args:
            config: WebSocket 服务器配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.host_label = BodyLabel(self.tr("主机"), self)
        self.host_config_label = BodyLabel(self.config.host, self)

        self.port_label = BodyLabel(self.tr("端口"), self)
        self.port_config_label = BodyLabel(str(self.config.port), self)

        self.heart_interval_label = BodyLabel(self.tr("心跳间隔"), self)
        self.heart_interval_config_label = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.msg_post_format_label = BodyLabel(self.tr("格式"), self)
        self.msg_post_format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        self.enable_force_push_event_label = BodyLabel(self.tr("强制推送事件"), self)
        self.enable_force_push_event_config_label = EnableTag(self.config.enableForcePushEvent, self)

        # 布局
        self.config_view_layout.addWidget(self.host_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.host_config_label, 0, 1, 1, 2)

        self.config_view_layout.addWidget(self.port_label, 0, 3, 1, 1)
        self.config_view_layout.addWidget(self.port_config_label, 0, 4, 1, 2)

        self.config_view_layout.addWidget(self.heart_interval_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.heart_interval_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.msg_post_format_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.msg_post_format_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.enable_force_push_event_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.enable_force_push_event_config_label, 2, 4, 1, 2)

    def fill_config(self) -> None:
        """填充 WebSocket 服务器配置数据"""
        self.host_config_label.setText(self.config.host)
        self.port_config_label.setText(str(self.config.port))
        self.heart_interval_config_label.setText(str(self.config.heartInterval) + "ms")
        self.msg_post_format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)
        self.enable_force_push_event_config_label.update_status(self.config.enableForcePushEvent)

    def get_config(self) -> WebsocketServersConfig:
        """获取 WebSocket 服务器配置数据

        Returns:
            WebsocketServersConfig: WebSocket 服务器配置
        """
        return cast(WebsocketServersConfig, self.config)

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = WebsocketServerConfigDialog(it(MainWindow), cast(WebsocketServersConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class WebsocketClientConfigCard(ConfigCardBase):
    """WebSocket 客户端配置卡片"""

    def __init__(self, config: WebsocketClientsConfig, parent: Optional[QObject] = None) -> None:
        """初始化 WebSocket 客户端配置卡片

        Args:
            config: WebSocket 客户端配置数据
            parent: 父控件
        """
        super().__init__(config, parent)

        # 创建控件
        self.url_label = BodyLabel(self.tr("URL"), self)
        self.url_config_label = BodyLabel(str(self.config.url), self)

        self.reconnect_interval_label = BodyLabel(self.tr("重连间隔"), self)
        self.reconnect_interval_config_label = BodyLabel(str(self.config.reconnectInterval) + "ms", self)

        self.heart_interval_label = BodyLabel(self.tr("心跳间隔"), self)
        self.heart_interval_config_label = BodyLabel(str(self.config.heartInterval) + "ms", self)

        self.format_label = BodyLabel(self.tr("格式"), self)
        self.format_config_label = FormateTag(self.config.messagePostFormat, self)

        self.report_self_message_label = BodyLabel(self.tr("上报自身消息"), self)
        self.report_self_message_config_label = EnableTag(self.config.reportSelfMessage, self)

        # 布局
        self.config_view_layout.addWidget(self.url_label, 0, 0, 1, 1)
        self.config_view_layout.addWidget(self.url_config_label, 0, 1, 1, 6)

        self.config_view_layout.addWidget(self.reconnect_interval_label, 1, 0, 1, 1)
        self.config_view_layout.addWidget(self.reconnect_interval_config_label, 1, 1, 1, 2)

        self.config_view_layout.addWidget(self.heart_interval_label, 1, 3, 1, 1)
        self.config_view_layout.addWidget(self.heart_interval_config_label, 1, 4, 1, 2)

        self.config_view_layout.addWidget(self.format_label, 2, 0, 1, 1)
        self.config_view_layout.addWidget(self.format_config_label, 2, 1, 1, 2)

        self.config_view_layout.addWidget(self.report_self_message_label, 2, 3, 1, 1)
        self.config_view_layout.addWidget(self.report_self_message_config_label, 2, 4, 1, 2)

    def fill_config(self) -> None:
        """填充 WebSocket 客户端配置数据"""
        self.url_config_label.setText(str(self.config.url))
        self.reconnect_interval_config_label.setText(str(self.config.reconnectInterval) + "ms")
        self.heart_interval_config_label.setText(str(self.config.heartInterval) + "ms")
        self.format_config_label.update_format(self.config.messagePostFormat)
        self.report_self_message_config_label.update_status(self.config.reportSelfMessage)

    def get_config(self) -> WebsocketClientsConfig:
        """获取 WebSocket 客户端配置数据

        Returns:
            WebsocketClientsConfig: WebSocket 客户端配置
        """
        return cast(WebsocketClientsConfig, self.config)

    def _slot_edit_button_clicked(self) -> None:
        """处理编辑按钮点击事件"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        dialog = WebsocketClientConfigDialog(it(MainWindow), cast(WebsocketClientsConfig, self.config))
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()
