# -*- coding: utf-8 -*-

"""
Bot 卡片
"""
from __future__ import annotations

# 标准库导入
from time import monotonic
from typing import cast

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
    themeColor,
)
from qfluentwidgets.components.widgets.icon_widget import IconWidget
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
    Slot,
    QSize,
    Signal,
)
from PySide6.QtGui import QColor, QEnterEvent, QFont, QMouseEvent, QPainter, QPaintEvent, QPen, QPixmap
from PySide6.QtWidgets import QGridLayout, QHBoxLayout, QVBoxLayout, QWidget
from creart import it
from qfluentwidgets.common.overload import singledispatchmethod

# 项目内模块导入
from src.core.config import cfg
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
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import mask_qqid
from src.core.runtime.backend_type import BackendType
from src.core.runtime.bot_process_manager import ManagerAutoRestartProcess, ManagerNapCatQQLoginState, BotProcessManager
from src.core.runtime.paths import PathFunc
from src.ui.common.icon import StaticIcon, NapCatDesktopIcon
from src.ui.components.info_bar import error_bar, info_bar, warning_bar
from src.ui.components.message_box import AskBox
from src.ui.page.bot_page.widget.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    QRCodeDialogFactory,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)


# ==================== 2026-05-11: 内存监控异步化 (避免主线程 psutil 卡顿) ====================
# 系统总内存 (RAM) 在单次 App session 内不变, 模块加载时算一次, 后续 BotCard.text 直接读.
# 旧版每个 BotCard 定时器每秒调 ``psutil.virtual_memory().total`` (跨进程 syscall),
# N 个 Bot × 每秒 1 次 = 显著 UI 卡顿. 现在缓存一次:
_CACHED_TOTAL_MEMORY_MB: int | None = None


def _total_memory_mb() -> int:
    """返回系统总物理内存 (MB), 模块级懒加载缓存.

    单 App session 内系统 RAM 不变, 缓存安全. 失败时返回 0 (UI 显示 ``... / 0 MB``,
    不影响 Bot 启动).
    """
    global _CACHED_TOTAL_MEMORY_MB
    if _CACHED_TOTAL_MEMORY_MB is None:
        try:
            _CACHED_TOTAL_MEMORY_MB = int(psutil.virtual_memory().total / (1024 * 1024))
        except Exception:  # noqa: BLE001 - psutil 任何异常 fallback 到 0
            _CACHED_TOTAL_MEMORY_MB = 0
    return _CACHED_TOTAL_MEMORY_MB


class _MemoryUsageWorker(QObject, QRunnable):
    """后台 worker: 在 ``QThreadPool`` 工作线程跑 ``BotProcessManager.get_memory_usage``.

    背景: ``BotProcessManager.get_memory_usage(qq_id)`` 内部委托给
    :meth:`NapCatDriver.get_memory_usage_for_pid`, 后者用 ``psutil`` walk QQ.exe
    进程树 (Electron 多子进程 5-15 个), 每个子进程一次跨进程 syscall, 累计 50-200ms.
    在主线程 ``QTimer.timeout`` 直调会卡 UI (用户实测热启动 + 内存监控定时器开启后明显卡顿).

    本 worker 把 walk 工作甩到线程池, 完成后通过 ``finished`` 信号回主线程更新 UI.

    SnowLuma 热启动 + NapCat 启动两条路径都受益.
    """

    # (qq_id, mem_mb, total_mb): total_mb 远端走服务器 RAM, 本地走 psutil
    finished = Signal(str, int, int)

    def __init__(self, qq_id: str) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._qq_id = qq_id
        # 由 QThreadPool 跑完自动 delete (worker 短寿, 不被 BotCard 持引用)
        self.setAutoDelete(True)

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            manager = it(BotProcessManager)
            mem_mb = manager.get_memory_usage(self._qq_id)
            # 修复 (2026-05-12): total 不再走 ``psutil.virtual_memory()``;
            # 远端 Bot 应显示**服务器 RAM**, 由 ``BotProcessManager.get_total_memory_mb``
            # 按 record.server_total_memory_bytes / 本地 psutil 自动分发.
            total_mb = manager.get_total_memory_mb(self._qq_id)
        except Exception:  # noqa: BLE001 - worker 边界吞所有异常避免 QThreadPool 上抛
            mem_mb = 0
            total_mb = 0
        self.finished.emit(self._qq_id, mem_mb, total_mb)


class BotCard(HeaderCardWidget):
    """Bot 卡片 Widget"""

    # 当自身被移除时发出信号 值为QQID
    remove_signal = Signal(str)
    # P4 F2: 批量模式下选中状态变化, 参数为 (qq_id, selected)
    selected_changed_signal = Signal(str, bool)

    @singledispatchmethod
    def __init__(self, config: Config, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None): 父控件, 可为 None. 
        """
        super().__init__(parent)

        # 设置属性
        self._config = config
        # P4 F2 / P4 W4: 批量模式 - 整卡可点击切换"选中态", 不再使用复选框
        self._batch_mode: bool = False
        self._selected: bool = False

        # 创建控件
        self.avatar_widget = BotAvatarWidget(str(self._config.bot.QQID), self)
        self.info_widget = BotInfoWidget(self._config, self)
        self.run_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"), self)
        self.stop_button = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("停止"), self)
        self.web_ui_button = TransparentToolButton(FluentIcon.CONNECT, self)
        self.vnc_button = TransparentToolButton(FluentIcon.VIDEO, self)
        self.qr_code_button = TransparentToolButton(FluentIcon.QRCODE, self)
        self.log_button = TransparentToolButton(NapCatDesktopIcon.LOG, self)
        self.setting_button = TransparentToolButton(FluentIcon.SETTING, self)
        self.remove_button = TransparentToolButton(FluentIcon.DELETE, self)

        # 设置控件
        self.setTitle(f"{self._config.bot.name} ({self._config.bot.QQID})")
        self.setFixedSize(500, 240)
        self.stop_button.hide()
        self.log_button.hide()
        self.web_ui_button.hide()
        self.vnc_button.hide()
        self.qr_code_button.hide()

        # 设置布局
        self.viewLayout.addWidget(self.avatar_widget, 1)
        self.viewLayout.addWidget(self.info_widget, 2)

        # P4 W4: 不再使用复选框, 转为整卡点击切换 "选中态". 按钮组照常摆在右侧操作区,
        # 批量模式下整体 hide; 普通模式下按 process_state 显隐. title 始终贴左 24px.
        self.headerLayout.addStretch(1)
        self.headerLayout.setSpacing(8)
        self.headerLayout.addWidget(self.run_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.stop_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.log_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.web_ui_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.vnc_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.qr_code_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.setting_button, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addWidget(self.remove_button, 0, Qt.AlignmentFlag.AlignVCenter)

        # 链接信号
        it(BotProcessManager).process_changed_signal.connect(self.slot_process_changed_button)
        it(ManagerNapCatQQLoginState).qr_code_available_signal.connect(self.slot_qr_code_available)
        it(ManagerNapCatQQLoginState).qr_code_removed_signal.connect(self.slot_qr_code_removed)
        self.run_button.clicked.connect(self.slot_run_button)
        self.stop_button.clicked.connect(self.slot_stop_button)
        self.log_button.clicked.connect(self.slot_log_button)
        self.web_ui_button.clicked.connect(self.slot_web_ui_button)
        self.vnc_button.clicked.connect(self.slot_vnc_button)
        self.qr_code_button.clicked.connect(self.slot_qr_code_button)
        self.setting_button.clicked.connect(self.slot_setting_button)
        self.remove_button.clicked.connect(self.slot_remove_button)

        # 调用方法
        self.set_tooltip()
        if it(QRCodeDialogFactory).has_qr_code(str(self._config.bot.QQID)):
            self.qr_code_button.show()

    # ==================== 公共方法 ==================
    # ---------- P4 F2 / P4 W4: 批量模式 ----------
    def set_batch_mode(self, enabled: bool, *, checked: bool = False) -> None:
        """切换批量模式; 启用时整卡可点击切换"选中态" + 隐藏 header 操作按钮.

        Args:
            enabled: True 进入批量模式, False 退出.
            checked: 进入批量模式时初始选中状态; 退出模式忽略.
        """
        self._batch_mode = enabled
        if enabled:
            # 整卡可点 -> 鼠标悬停手型, 提示用户卡片可点击选中
            self.setCursor(Qt.CursorShape.PointingHandCursor)
            # 批量模式下统一隐藏单卡操作按钮, 让底部 batch CommandBar 接管
            for btn in (
                self.run_button,
                self.stop_button,
                self.log_button,
                self.web_ui_button,
                self.vnc_button,
                self.qr_code_button,
                self.setting_button,
                self.remove_button,
            ):
                btn.hide()
            # 进入批量模式时按 ``checked`` 设定初始选中态; 不发信号 (避免回声)
            self._set_selected_silently(checked)
        else:
            self.unsetCursor()
            # 退出批量模式时无条件清除选中态; 不发信号
            self._set_selected_silently(False)
            # 退出批量模式: 按当前进程状态 + 二维码可见性还原 header 按钮
            self._restore_action_buttons()

    def _restore_action_buttons(self) -> None:
        """退出批量模式时还原 header 按钮显隐态.

        - run / stop / log / web_ui 由 ``slot_process_changed_button`` 统一处理.
        - setting / remove 在普通模式下应始终可见.
        - qr_code 取决于 ``QRCodeDialogFactory.has_qr_code``.
        """
        qq_id = str(self._config.bot.QQID)
        record = it(BotProcessManager).get_process(qq_id)
        state = record.state if record is not None else QProcess.ProcessState.NotRunning
        self.slot_process_changed_button(qq_id, state)
        self.setting_button.show()
        self.remove_button.show()
        if it(QRCodeDialogFactory).has_qr_code(qq_id):
            self.qr_code_button.show()

    def is_batch_mode(self) -> bool:
        """当前是否处于批量模式."""
        return self._batch_mode

    def is_batch_selected(self) -> bool:
        """批量模式下是否被选中; 非批量模式返回 False."""
        return self._batch_mode and self._selected

    def set_selected(self, value: bool) -> None:
        """外部 (BotListPage 全选/取消全选) 调用; 触发选中态变化信号."""
        if self._selected == bool(value):
            return
        self._selected = bool(value)
        self.update()  # 触发 paintEvent 重绘选中态边框
        self.selected_changed_signal.emit(str(self._config.bot.QQID), self._selected)

    def _set_selected_silently(self, value: bool) -> None:
        """内部使用; 设置选中态但**不发信号** (用于 set_batch_mode 进入/退出场景)."""
        self._selected = bool(value)
        self.update()

    def update_info_card(self) -> None:
        """更新信息卡片显示内容,  用于外部调用, 刷新后调用.

        P2.6 起 ``BotProcessManager.get_process`` 既可能返回本地
        [`NapCatProcessModel`](src/core/runtime/napcat.py) (带 ``process`` 字段),
        也可能返回远端 [`RemoteProcessRecord`](src/core/runtime/napcat.py) (没有 ``process``).
        两类 record 都暴露 ``state`` 字段, 这里统一以 ``state`` 判断, 避免硬依赖 QProcess.

        P3 perf: 同步反映 ``Starting`` 状态, 让 BotList 重建后仍保留启动中指示.
        """
        record = it(BotProcessManager).get_process(str(self._config.bot.QQID))
        if record is None:
            return

        qq_id = str(self._config.bot.QQID)
        if record.state == QProcess.ProcessState.Starting:
            self.slot_process_changed_button(qq_id, QProcess.ProcessState.Starting)
            return
        if record.state != QProcess.ProcessState.Running:
            return

        self.slot_process_changed_button(qq_id, QProcess.ProcessState.Running)
        self.info_widget.slot_run_time_start(qq_id, QProcess.ProcessState.Running)
        self.info_widget.slot_memory_usage_start(qq_id, QProcess.ProcessState.Running)

    # ==================== UI方法 ====================
    def set_tooltip(self) -> None:
        """设置工具提示"""
        self.run_button.setToolTip(self.tr("启动 Bot"))
        self.stop_button.setToolTip(self.tr("停止 Bot"))
        self.log_button.setToolTip(self.tr("查看日志"))
        self.web_ui_button.setToolTip(self.tr("打开 WebUI"))
        self.vnc_button.setToolTip(self.tr("打开 VNC 扫码页"))
        self.qr_code_button.setToolTip(self.tr("查看登录二维码"))
        self.setting_button.setToolTip(self.tr("配置 Bot"))
        self.remove_button.setToolTip(self.tr("移除 Bot"))

        for button in [
            self.run_button,
            self.stop_button,
            self.log_button,
            self.web_ui_button,
            self.vnc_button,
            self.qr_code_button,
            self.setting_button,
            self.remove_button,
        ]:
            button.setToolTipDuration(1000)
            button.installEventFilter(ToolTipFilter(button, showDelay=300))

    # ==================== 槽函数 ====================
    def slot_run_button(self) -> None:
        """处理运行按钮点击.

        Q2 (SnowLuma 启动模式): SnowLuma 后端 + 本地运行 时弹冷/热启动选择对话框;
        热启动且系统有多个 QQ.exe 时再弹 PID picker. NapCat / 远端走原始路径 (无对话框).
        """
        qq_id_masked = mask_qqid(self._config.bot.QQID)
        logger.info(f"请求启动 Bot(QQID: {qq_id_masked})", log_source=LogSource.UI)

        # Q2: 只有本地 SnowLuma 才需要选启动模式 (NapCat / 远端无热启动概念)
        is_local_snowluma = (
            not self._config.bot.is_remote
            and self._config.bot.backend_type == BackendType.SNOWLUMA
        )
        if not is_local_snowluma:
            it(BotProcessManager).start_bot(self._config)
            return

        # SnowLuma 本地路径: 走 Q2 启动模式选择流程
        self._snowluma_run_with_mode_dialog()

    def _snowluma_run_with_mode_dialog(self) -> None:
        """Q2: SnowLuma 启动模式对话框流程 (冷/热) + 热启动 PID 选择.

        流程:
        1. 弹 :class:`SnowLumaStartModeDialog` 让用户选模式; 用户取消 → abort.
        2. 冷启动 → 直接调 ``start_bot(mode=COLD_START)`` (与历史一致).
        3. 热启动 → 弹 info_bar "正在扫描 QQ.exe 进程..." + 异步 worker 跑 psutil 枚举
           (实测 cold call ~2.6s, 主线程同步会卡 UI); worker 完成后调 :meth:`_on_qq_enum_done`.
        4. :meth:`_on_qq_enum_done` 根据候选数量:
           - 0 个 QQ.exe: error_bar 提示用户先启动 QQ 或改冷启动, abort.
           - 1 个 QQ.exe: 直接用其 PID, 跳过 picker.
           - N>1 个 QQ.exe: 弹 :class:`SnowLumaPidPickerDialog`; 用户取消 → abort.
        5. 调 ``start_bot(mode=HOT_START, attach_pid=...)``.

        所有用户取消 / abort 路径都**不**触发 start_bot, UI 状态保持原样.
        """
        # 延迟导入避免循环 (snowluma_start_dialog 依赖 SnowLumaStartMode 但 card 模块加载早)
        from src.core.runtime.snowluma_driver import SnowLumaStartMode
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            EnumerateQQProcessesWorker,
            SnowLumaStartModeDialog,
        )

        qq_id_masked = mask_qqid(self._config.bot.QQID)

        # Step 1: 模式选择
        mode_dialog = SnowLumaStartModeDialog(self.window())
        if not mode_dialog.exec():
            logger.info(
                f"用户取消 SnowLuma 启动模式选择, abort 启动(QQID: {qq_id_masked})",
                log_source=LogSource.UI,
            )
            return

        mode = mode_dialog.get_value()

        # Step 2: 冷启动 → 直接 start (无需枚举)
        if mode == SnowLumaStartMode.COLD_START:
            logger.info(
                f"SnowLuma 冷启动模式 (spawn 新 QQ.exe)(QQID: {qq_id_masked})",
                log_source=LogSource.UI,
            )
            it(BotProcessManager).start_bot(
                self._config, snowluma_start_mode=SnowLumaStartMode.COLD_START
            )
            return

        # Step 3: 热启动 → 异步枚举 QQ.exe (避免 2.6s UI freeze)
        # 2026-05-11 诊断: 加细粒度 trace 排查"提交 worker 后 UI 完全锁死" 问题.
        # 用户实测主线程被卡住, 不是 worker 慢. 每步打时间戳定位真正同步阻塞点.
        import time as _time

        _t_step3 = _time.monotonic()

        info_bar(
            self.tr("正在扫描系统中的 QQ.exe 进程..."),
            title=self.tr("热启动"),
            parent=self,
        )
        _t_after_info_bar = _time.monotonic()
        logger.trace(
            f"SnowLuma 热启动 step3 info_bar 耗时 {(_t_after_info_bar - _t_step3)*1000:.1f}ms",
            log_source=LogSource.UI,
        )

        logger.info(
            f"SnowLuma 热启动: 启动后台 QQ.exe 枚举 worker(QQID: {qq_id_masked})",
            log_source=LogSource.UI,
        )
        _t_after_log = _time.monotonic()

        worker = EnumerateQQProcessesWorker()
        _t_after_worker_init = _time.monotonic()
        logger.trace(
            f"SnowLuma 热启动 EnumerateQQProcessesWorker.__init__ 耗时 "
            f"{(_t_after_worker_init - _t_after_log)*1000:.1f}ms",
            log_source=LogSource.UI,
        )

        # 持 worker 强引用防 GC; worker 完成后 _on_qq_enum_done 会释放
        self._snowluma_enum_worker = worker
        worker.finished.connect(self._on_qq_enum_done)
        _t_after_connect = _time.monotonic()

        # 诊断池状态: maxThreadCount / activeThreadCount 帮判断 worker 是否会立即开跑
        pool = QThreadPool.globalInstance()
        logger.trace(
            f"SnowLuma 热启动 worker.connect 耗时 "
            f"{(_t_after_connect - _t_after_worker_init)*1000:.1f}ms; "
            f"QThreadPool max={pool.maxThreadCount()} active={pool.activeThreadCount()}",
            log_source=LogSource.UI,
        )

        pool.start(worker)
        _t_after_start = _time.monotonic()
        logger.trace(
            f"SnowLuma 热启动 QThreadPool.start 耗时 "
            f"{(_t_after_start - _t_after_connect)*1000:.1f}ms; "
            f"step3 总耗时 {(_t_after_start - _t_step3)*1000:.1f}ms",
            log_source=LogSource.UI,
        )

    def _on_qq_enum_done(self, candidates: object) -> None:
        """Q2 热启动 QQ 枚举完成回调 (在主线程跑).

        根据候选数量: 0 → abort, 1 → 自动用, N → 弹 picker.
        完成后释放 worker 强引用, 供 Python GC.
        """
        from src.core.runtime.snowluma_driver import SnowLumaStartMode
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            QQProcessInfo,
            SnowLumaPidPickerDialog,
        )

        qq_id_masked = mask_qqid(self._config.bot.QQID)

        # 释放 worker 引用 + deleteLater
        worker = getattr(self, "_snowluma_enum_worker", None)
        if worker is not None:
            try:
                worker.deleteLater()
            except Exception:  # noqa: BLE001
                pass
            self._snowluma_enum_worker = None

        if not isinstance(candidates, list):
            candidates = []
        # Type-narrow: 过滤非 QQProcessInfo (防御异常 payload)
        candidates = [c for c in candidates if isinstance(c, QQProcessInfo)]

        if not candidates:
            error_bar(
                self.tr("未检测到 QQ.exe, 请先手动启动并登录 QQ"),
                title=self.tr("热启动失败"),
                parent=self,
            )
            logger.warning(
                f"SnowLuma 热启动 abort: 未检测到 QQ.exe 进程(QQID: {qq_id_masked})",
                log_source=LogSource.UI,
            )
            return

        attach_pid: int
        if len(candidates) == 1:
            attach_pid = candidates[0].pid
            logger.info(
                (
                    f"SnowLuma 热启动: 唯一候选自动选中 (QQID: {qq_id_masked}, "
                    f"attach_pid={attach_pid}, started_at={candidates[0].create_time_iso})"
                ),
                log_source=LogSource.UI,
            )
        else:
            # 多个候选 → 弹 PID picker
            picker = SnowLumaPidPickerDialog(self.window(), candidates)
            if not picker.exec():
                logger.info(
                    f"用户取消 SnowLuma PID 选择, abort 热启动(QQID: {qq_id_masked})",
                    log_source=LogSource.UI,
                )
                return
            attach_pid = picker.get_value()
            if attach_pid <= 0:
                error_bar(
                    self.tr("未选中任何 QQ.exe 进程"),
                    title=self.tr("热启动失败"),
                    parent=self,
                )
                return
            logger.info(
                f"SnowLuma 热启动: 用户选中 PID={attach_pid}(QQID: {qq_id_masked})",
                log_source=LogSource.UI,
            )

        # 真正 start
        it(BotProcessManager).start_bot(
            self._config,
            snowluma_start_mode=SnowLumaStartMode.HOT_START,
            snowluma_attach_pid=attach_pid,
        )

    def slot_stop_button(self) -> None:
        """处理停止按钮点击"""
        logger.info(f"请求停止 Bot(QQID: {mask_qqid(self._config.bot.QQID)})", log_source=LogSource.UI)
        it(BotProcessManager).stop_bot(str(self._config.bot.QQID))
        it(ManagerAutoRestartProcess).remove_auto_restart_timer(str(self._config.bot.QQID))

    def slot_process_changed_button(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """处理 NapCatQQ 进程变化时, 切换按钮显示.

        P3 perf: 区分 ``Starting`` / ``Running`` / ``NotRunning``. ``Starting`` 时
        卡片本身仅把 "启动" 按钮 disable + 文案改 "启动中...", 卡片视觉保持稳定;
        进度反馈与最终成败统一在主窗口右上角的
        [`ProgressInfoBar`](src/ui/components/progress_info_bar_bridge.py) 上展示.

        Args:
            qq_id (str): QQ 号
            state (QProcess.ProcessState): 进程状态
        """
        if qq_id != str(self._config.bot.QQID):
            return

        # P4 W4 修复: 批量模式下不响应 process state 变化, 否则远端 Bot 停止/启动
        # 完成时这里会把 run/stop/log/web_ui 按钮重新 show 出来, 与批量模式预期
        # (按钮全部 hide, 只显示底部 CommandBar) 冲突. 仍然更新 run_button 的
        # enabled/text 状态, 这样退出批量模式后按钮恢复时能反映正确的文案.
        if self._batch_mode:
            # 仅同步内部状态 (enabled / text), 不动 visibility
            if state == QProcess.ProcessState.Starting:
                self.run_button.setEnabled(False)
                self.run_button.setText(self.tr("启动中…"))
            else:
                self.run_button.setEnabled(True)
                self.run_button.setText(self.tr("启动"))
            return

        if state == QProcess.ProcessState.Starting:
            # 启动中: run_button 灰掉 + 文案改, 其他按钮维持现状
            self.run_button.show()
            self.run_button.setEnabled(False)
            self.run_button.setText(self.tr("启动中…"))
            self.stop_button.hide()
            self.log_button.hide()
            self.web_ui_button.hide()
            self.vnc_button.hide()
            return

        # 离开 Starting: 还原按钮可用性与文案
        self.run_button.setEnabled(True)
        self.run_button.setText(self.tr("启动"))

        if state == QProcess.ProcessState.Running:
            self.run_button.hide()
            self.stop_button.show()
            self.log_button.show()
            self.web_ui_button.show()
            # VNC 按钮仅在远端 SnowLuma 场景显示 (本地 SL 用户已在桌面看见 Xvfb 画面;
            # 远端 NC / 本地 NC 没有 noVNC 端点)
            if self._is_remote_snowluma():
                self.vnc_button.show()
            else:
                self.vnc_button.hide()
        else:
            self.run_button.show()
            self.stop_button.hide()
            self.log_button.hide()
            self.web_ui_button.hide()
            self.vnc_button.hide()

    def slot_log_button(self) -> None:
        """处理日志按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        logger.trace(f"打开 Bot 日志页(QQID: {mask_qqid(self._config.bot.QQID)})", log_source=LogSource.UI)
        page = it(BotPage)
        page.view.setCurrentWidget(page.log_page)
        page.log_page.set_current_log_manager(self._config)

    def slot_web_ui_button(self) -> None:
        """处理 WebUI 按钮槽函数, 打开 Bot 的 WebUI.

        分流策略:

        - **本地 NapCat**: 读 :class:`ManagerNapCatQQLoginState` 拿 port + token 拼
          ``http://127.0.0.1:{port}/webui?token={token}`` (历史行为零变更).
        - **远端 NapCat**: 走 :class:`ManagerNapCatQQLoginState` (由 BotProcessManager
          ``_publish_remote_login_state`` 写入隧道本地 port + token); URL 形态与本地一致.
        - **本地 SnowLuma**: 读 ``<snowluma_path>/config/runtime.json`` 拿 ``webuiPort``
          (默认 5099), 打开 ``http://127.0.0.1:{webuiPort}/``, 同时把
          :func:`load_session` 拿到的明文密码**自动复制到剪贴板** + 弹 InfoBar
          告知用户 (SnowLuma WebUI 内手动粘贴即可登录).
        - **远端 SnowLuma** (2026-05-12 新增): 走 :class:`ManagerNapCatQQLoginState`
          拿隧道本地 port + webui.secret 密码 (由
          :meth:`RemoteSnowLumaBackend.get_webui_endpoint` 经 SSH 拉 ``webui.secret``,
          再由 ``_publish_remote_login_state`` 注册); URL 形态 ``http://127.0.0.1:{port}/``
          (SL WebUI 走表单登录, 不接受 ?token query 参数), 密码复制到剪贴板.
        """
        from PySide6.QtGui import QDesktopServices
        from PySide6.QtCore import QUrl
        from PySide6.QtWidgets import QApplication

        qq_id = str(self._config.bot.QQID)
        is_snowluma = self._config.bot.backend_type == BackendType.SNOWLUMA
        is_remote = self._config.bot.is_remote

        # 远端 SnowLuma: 直接从 SL backend 实时拿 endpoint (隧道 port + webui.secret 密码).
        # 不走 ``ManagerNapCatQQLoginState`` - 那套管理器设计给 NapCat 的 /api/auth/login
        # 轮询, 对 SL 没意义; SL 的 endpoint 在 daemon READY 期间由 backend 持有,
        # 点按钮时 backend.get_webui_endpoint() 即可返回. 不依赖 poll 链路.
        if is_snowluma and is_remote:
            from creart import it as _it

            from src.core.remote import ServerManager

            server_id = self._config.bot.runtime_target or ""
            if not server_id:
                error_bar(
                    self.tr("该 Bot 未绑定远端服务器, 无法定位 SnowLuma WebUI"),
                    title=self.tr("WebUI 打开失败"),
                    parent=self,
                )
                return
            try:
                backend = _it(ServerManager).get_backend(server_id)
                endpoint = backend.get_webui_endpoint(qq_id)
            except Exception as exc:  # noqa: BLE001
                error_bar(
                    self.tr(
                        "获取 SnowLuma 远端 WebUI 端点失败: {err}"
                    ).format(err=f"{type(exc).__name__}: {exc}"),
                    title=self.tr("WebUI 打开失败"),
                    parent=self,
                )
                logger.warning(
                    (
                        f"打开远端 SnowLuma WebUI 失败: backend.get_webui_endpoint 异常 "
                        f"(QQID: {mask_qqid(qq_id)}, server_id={server_id}, "
                        f"exc={type(exc).__name__}: {exc})"
                    ),
                    log_source=LogSource.UI,
                )
                return
            if endpoint is None:
                error_bar(
                    self.tr(
                        "该 Bot 尚未启动或 SnowLuma 远端隧道未建立, 请先启动 Bot 再试"
                    ),
                    title=self.tr("WebUI 未就绪"),
                    parent=self,
                )
                logger.warning(
                    f"打开远端 SnowLuma WebUI 失败: endpoint 为 None "
                    f"(QQID: {mask_qqid(qq_id)}, server_id={server_id})",
                    log_source=LogSource.UI,
                )
                return
            if not endpoint.token:
                warning_bar(
                    self.tr(
                        "SnowLuma 远端 webui.secret 读取失败, 密码未复制到剪贴板; "
                        "请手动登录或检查远端 {path} 文件"
                    ).format(path="~/snowluma-remote/workspace/webui.secret"),
                    parent=self,
                )

            # SL WebUI 走表单登录, 不带 ?token 查询参数; base_url 即 http://127.0.0.1:{port}
            # (隧道本地端口); 末尾补一个 ``/`` 避免浏览器 30x 重定向冷启动.
            web_ui_url = endpoint.base_url.rstrip("/") + "/"
            if endpoint.token:
                QApplication.clipboard().setText(str(endpoint.token))
                info_bar(
                    self.tr(
                        "已将远端 SnowLuma WebUI 密码复制到剪贴板, 直接粘贴登录即可"
                    ),
                    title=self.tr("密码已就绪"),
                    parent=self,
                )
            QDesktopServices.openUrl(QUrl(web_ui_url))
            logger.info(
                f"已打开远端 SnowLuma WebUI(QQID: {mask_qqid(qq_id)}, url={web_ui_url})",
                log_source=LogSource.UI,
            )
            return

        # 本地 SnowLuma: 不依赖 ManagerNapCatQQLoginState (该 manager 仅服务 NapCat 本地)
        if is_snowluma:
            import json

            from src.core.runtime.snowluma_session import load_session

            webui_port = 5099
            runtime_json = it(PathFunc).get_snowluma_config_dir() / "runtime.json"
            if runtime_json.exists():
                try:
                    payload = json.loads(runtime_json.read_text(encoding="utf-8"))
                    if isinstance(payload, dict):
                        candidate = payload.get("webuiPort", 5099)
                        if isinstance(candidate, int) and candidate > 0:
                            webui_port = candidate
                except (OSError, json.JSONDecodeError, ValueError, TypeError):
                    pass
            web_ui_url = f"http://127.0.0.1:{webui_port}/"

            # W6 (2026-05-11): 密码源从 per-Bot 改为 App 级 QConfig
            # ``cfg.snowluma_webui_password_override`` (与 daemon 读取一致).
            # 本按钮仍是**只读**视图: 不调 ``resolve_effective_password`` 避免 session
            # 不存在时副作用地 ``create_session`` 写盘 (启动 Bot 时再生成).
            from src.core.config import cfg
            override = (cfg.get(cfg.snowluma_webui_password_override) or "").strip()
            effective_password: str | None = None
            password_source = ""
            if override:
                effective_password = override
                password_source = self.tr("组件页 SnowLuma 全局密码 override")
            else:
                session = load_session()
                if session is not None:
                    effective_password = session.password
                    password_source = self.tr("Desktop 自动生成密码")

            if effective_password is not None:
                QApplication.clipboard().setText(effective_password)
                info_bar(
                    self.tr("已将 SnowLuma WebUI 密码复制到剪贴板 ({source}), 直接粘贴登录即可").format(
                        source=password_source
                    ),
                    title=self.tr("密码已就绪"),
                    parent=self,
                )
                logger.info(
                    f"SnowLuma WebUI 密码已写入剪贴板(QQID: {mask_qqid(qq_id)}, source={password_source})",
                    log_source=LogSource.UI,
                )
            else:
                warning_bar(
                    self.tr(
                        "未找到 snowluma-session.json 且未设置自定义密码 override. "
                        "请先启动一次 Bot 让 Desktop 生成密码, 或在 Bot 配置中填入自定义 WebUI 密码."
                    ),
                    parent=self,
                )

            QDesktopServices.openUrl(QUrl(web_ui_url))
            logger.info(
                f"已打开 SnowLuma WebUI(QQID: {mask_qqid(qq_id)}, url={web_ui_url})",
                log_source=LogSource.UI,
            )
            return

        # NapCat 分支 (历史行为)
        login_state = it(ManagerNapCatQQLoginState).get_login_state(qq_id)

        if login_state is None:
            error_bar(
                self.tr("该 Bot 尚未启动或 WebUI 信息未获取"),
                title=self.tr("WebUI 未就绪"),
                parent=self,
            )
            logger.warning(f"打开 WebUI 失败：Bot 未就绪(QQID: {mask_qqid(qq_id)})", log_source=LogSource.UI)
            return

        # 构建 WebUI URL
        web_ui_url = f"http://127.0.0.1:{login_state.port}/webui?token={login_state.token}"

        QDesktopServices.openUrl(QUrl(web_ui_url))
        logger.info(f"已打开 WebUI(QQID: {mask_qqid(qq_id)}, url={web_ui_url})", log_source=LogSource.UI)

    def slot_vnc_button(self) -> None:
        """处理 VNC 按钮槽函数, 在浏览器打开远端 SnowLuma noVNC 扫码页.

        仅远端 SnowLuma 场景可见; 走 :meth:`RemoteSnowLumaBackend.open_vnc` 一站式
        (拿 daemon 隧道 bundle + 读 vnc.secret + 调系统浏览器). 失败时 error_bar 提示.
        """
        from src.core.remote import ServerManager

        qq_id = str(self._config.bot.QQID)
        if not self._is_remote_snowluma():
            # 防御性兜底: 按钮显隐已经卡了, 这条理论上不会触发
            error_bar(
                self.tr("仅远端 SnowLuma Bot 支持打开 VNC"),
                title=self.tr("不支持的操作"),
                parent=self,
            )
            return

        server_id = self._config.bot.runtime_target or ""
        if not server_id:
            error_bar(
                self.tr("该 Bot 未绑定远端服务器, 无法打开 VNC"),
                title=self.tr("VNC 打开失败"),
                parent=self,
            )
            return

        try:
            backend = it(ServerManager).get_backend(server_id)
        except Exception as exc:  # noqa: BLE001
            error_bar(
                self.tr("获取远端 SnowLuma backend 失败: {err}").format(
                    err=f"{type(exc).__name__}: {exc}"
                ),
                title=self.tr("VNC 打开失败"),
                parent=self,
            )
            logger.warning(
                (
                    f"打开远端 SnowLuma VNC 失败: ServerManager.get_backend 异常 "
                    f"(QQID: {mask_qqid(qq_id)}, server_id={server_id}, "
                    f"exc={type(exc).__name__}: {exc})"
                ),
                log_source=LogSource.UI,
            )
            return

        open_vnc = getattr(backend, "open_vnc", None)
        if open_vnc is None:
            # backend 不是 RemoteSnowLumaBackend (例如远端 NC); 理论上 _is_remote_snowluma
            # 已经过滤, 这里仅做防御
            error_bar(
                self.tr("当前 backend 不支持 VNC 操作"),
                title=self.tr("VNC 打开失败"),
                parent=self,
            )
            return

        try:
            ok, message = open_vnc()
        except Exception as exc:  # noqa: BLE001
            error_bar(
                self.tr("打开 VNC 异常: {err}").format(
                    err=f"{type(exc).__name__}: {exc}"
                ),
                title=self.tr("VNC 打开失败"),
                parent=self,
            )
            logger.warning(
                (
                    f"打开远端 SnowLuma VNC 异常 "
                    f"(QQID: {mask_qqid(qq_id)}, server_id={server_id}): "
                    f"{type(exc).__name__}: {exc}"
                ),
                log_source=LogSource.UI,
            )
            return

        if ok:
            info_bar(
                self.tr("已在浏览器打开 noVNC 扫码页 ({url})").format(url=message),
                title=self.tr("VNC 已打开"),
                parent=self,
            )
            logger.info(
                f"已打开远端 SnowLuma VNC(QQID: {mask_qqid(qq_id)}, server_id={server_id}, "
                f"endpoint={message})",
                log_source=LogSource.UI,
            )
        else:
            error_bar(
                self.tr("打开 VNC 失败: {msg}").format(msg=message),
                title=self.tr("VNC 打开失败"),
                parent=self,
            )
            logger.warning(
                f"打开远端 SnowLuma VNC 失败 "
                f"(QQID: {mask_qqid(qq_id)}, server_id={server_id}): {message}",
                log_source=LogSource.UI,
            )

    def _is_remote_snowluma(self) -> bool:
        """当前 Bot 是否为 "远端 + SnowLuma backend"; VNC 按钮显隐依据."""
        return (
            self._config.bot.backend_type == BackendType.SNOWLUMA
            and self._config.bot.is_remote
        )

    def slot_qr_code_button(self) -> None:
        """处理二维码按钮槽函数.

        P1 (SnowLuma 适配): SnowLuma 分支不走 :class:`QRCodeDialogFactory`,
        改为提示用户到 SnowLuma WebUI 内扫码 (二维码不在 Desktop 内渲染).
        """
        if self._config.bot.backend_type == BackendType.SNOWLUMA:
            info_bar(self.tr("请在 SnowLuma WebUI 内完成扫码登录"), parent=self)
            return

        it(QRCodeDialogFactory).show(str(self._config.bot.QQID))

    def slot_qr_code_available(self, qq_id: str, qr_code: str) -> None:
        """当前 Bot 有待扫码二维码时显示入口按钮. """
        del qr_code
        if qq_id != str(self._config.bot.QQID):
            return
        self.qr_code_button.show()

    def slot_qr_code_removed(self, qq_id: str) -> None:
        """当前 Bot 的二维码失效后隐藏入口按钮. """
        if qq_id != str(self._config.bot.QQID):
            return
        self.qr_code_button.hide()

    def slot_setting_button(self) -> None:
        """处理配置按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        logger.trace(f"打开 Bot 配置页(QQID: {mask_qqid(self._config.bot.QQID)})", log_source=LogSource.UI)
        page = it(BotPage)
        page.view.setCurrentWidget(page.bot_config_page)
        page.bot_config_page.fill_config(self._config)

    def slot_remove_button(self) -> None:
        """处理移除自身槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        qq_id = str(self._config.bot.QQID)
        process_manager = it(BotProcessManager)

        if process_manager.get_process(qq_id) is not None:
            logger.warning(f"拒绝移除运行中的 Bot(QQID: {mask_qqid(qq_id)})", log_source=LogSource.UI)
            warning_bar(self.tr("请先停止正在运行的 Bot，再执行移除"))
            return

        if AskBox(
            self.tr("确认移除 Bot"),
            self.tr(f"确定要移除 Bot ({self._config.bot.QQID}) 吗？\n此操作无法恢复!"),
            it(MainWindow),
        ).exec():
            logger.info(f"确认移除 Bot(QQID: {mask_qqid(qq_id)})", log_source=LogSource.UI)

            it(ManagerNapCatQQLoginState).remove_login_state(qq_id)

            it(ManagerAutoRestartProcess).remove_auto_restart_timer(qq_id)
            self.remove_signal.emit(str(self._config.bot.QQID))

    # ==================== 事件 (P4 W4 选中态) ====================
    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802 - Qt 命名固定
        """批量模式下整卡可点; 左键单击切换选中态."""
        if self._batch_mode and event.button() == Qt.MouseButton.LeftButton:
            new_value = not self._selected
            # 走 set_selected 路径, 触发 selected_changed_signal 让 BotListPage
            # 同步刷新 "已选 N / M" 计数与启停删按钮的可用性.
            self.set_selected(new_value)
            event.accept()
            return
        super().mousePressEvent(event)

    def paintEvent(self, event: QPaintEvent) -> None:  # noqa: N802 - Qt 命名固定
        """先让 SimpleCardWidget 画原 card, 选中时再叠 2px themeColor 边框."""
        super().paintEvent(event)
        if not (self._batch_mode and self._selected):
            return
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        # themeColor 是 qfluentwidgets 当前主题主色; 与全局 hover/选中观感一致
        pen = QPen(themeColor(), 2)
        painter.setPen(pen)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        # 内缩 1px 避免边框被父 widget 裁掉; 圆角与 SimpleCardWidget 默认 ~6px 对齐
        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 6, 6)


class BotAvatarWidget(QWidget):
    """Bot 头像展示控件

    封装了获取头像的功能, 便于维护
    """

    class GetAvatarWorker(QObject, QRunnable):
        """使用 QRunnable 异步获取头像

        注意: 不在工作线程中创建/使用任何 GUI 对象(QPixmap/QWidget 等). 
        仅下载原始字节并通过信号传回主线程处理. 
        """

        avatar_bytes_signal = Signal(str, bytes)
        avatar_error_signal = Signal(str, str)

        def __init__(self, qq_id: str) -> None:
            QObject.__init__(self)
            QRunnable.__init__(self)
            # 解析出对应的头像 URL
            url = Urls.QQ_AVATAR.value
            query = QUrlQuery()
            query.addQueryItem("spec", "640")
            query.addQueryItem("dst_uin", qq_id)
            url.setQuery(query)

            # 设置属性
            self._qq_id = qq_id
            self._url = url

        def run(self) -> None:
            """在工作线程中下载头像原始数据并通过信号发送"""
            try:
                resp = httpx.get(self._url.toString(), timeout=10.0)
                resp.raise_for_status()
                self.avatar_bytes_signal.emit(self._qq_id, resp.content)

            except (httpx.HTTPStatusError, httpx.RequestError, httpx.TimeoutException) as e:
                self.avatar_error_signal.emit(
                    self._qq_id,
                    (
                        "请求头像时发生错误!\n"
                        f"  - QQ号: {self._qq_id}\n"
                        f"  - 错误类型: {e.__class__.__name__}\n"
                        f"  - 错误信息: {e}"
                    ),
                )

    def __init__(self, qq_id: str, parent: BotCard) -> None:
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

        # 设置动画, 向上移动 4 个像素
        target_pos = QPoint(current_pos.x(), current_pos.y() - 4)

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
    def qq_id(self) -> str:
        return self._qq_id

    @qq_id.setter
    def qq_id(self, value: str) -> None:
        # 保存请求的 qq_id, 并把它作为当前活动请求的标识, 用于忽略过时的 worker 结果
        self._qq_id = value
        self._active_avatar_qq_id = value

        worker = self.GetAvatarWorker(value)
        # 在主线程中将字节转换为 QPixmap 并更新 UI, 避免跨线程创建 GUI 对象
        worker.avatar_bytes_signal.connect(self._on_avatar_bytes)
        worker.avatar_error_signal.connect(self._on_avatar_error)

        QThreadPool.globalInstance().start(worker)

    @Slot(str, str)
    def _on_avatar_error(self, qq_id: str, message: str) -> None:
        """在主线程中处理头像下载失败提示. """
        if qq_id != getattr(self, "_active_avatar_qq_id", None):
            return

        error_bar(message)

        try:
            self.image_label.setImage(StaticIcon.LOGO.path())
            self.image_label.scaledToWidth(128)
            self.image_label.setBorderRadius(8, 8, 8, 8)
        except Exception:
            pass

    @Slot(str, bytes)
    def _on_avatar_bytes(self, qq_id: str, data: bytes) -> None:
        """将下载的头像字节转换为 QPixmap 并更新到 UI (主线程执行)

        仅在收到的 qq_id 与当前活动请求一致时才更新, 避免竞态条件导致显示过时头像. 
        """
        # Only update UI if the avatar is for the latest requested qq_id
        if qq_id != getattr(self, "_active_avatar_qq_id", None):
            return

        pixmap = QPixmap()
        if pixmap.loadFromData(data):
            self.image_label.setImage(pixmap)
            self.image_label.scaledToWidth(128)
            self.image_label.setBorderRadius(8, 8, 8, 8)
        else:
            # 加载失败时显示默认占位图, 避免空白或保持旧头像
            try:
                self.image_label.setImage(StaticIcon.LOGO.path())
                self.image_label.scaledToWidth(128)
                self.image_label.setBorderRadius(8, 8, 8, 8)
            except Exception:
                # 最后兜底: 记录错误到 info bar
                error_bar(self.tr("头像数据无法解析，已使用占位图"))


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
            self.h_box_layout.setSpacing(10)
            self.h_box_layout.addWidget(self.icon_widget, 0)
            self.h_box_layout.addSpacing(4)
            self.h_box_layout.addWidget(self.text_label, 0)

        def set_icon(self, icon: FluentIconBase, light: str = "#454655", dark: str = "#fff3fa") -> None:
            """更新左侧图标. """
            self._icon = icon.colored(QColor(light), QColor(dark))
            self.icon_widget.setIcon(self._icon)

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
        self.start_time: float | None = None

        # 创建控件
        self._run_time_info = self.InfoWidget(FluentIcon.DATE_TIME, "未运行", self)
        self._memory_info = self.InfoWidget(FluentIcon.SPEED_HIGH, "-M / -M", self)
        self._tag_info = self.TagWidget(self._config.connect, self)

        # 设置布局
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(10)
        self.v_box_layout.addWidget(self._run_time_info)
        self.v_box_layout.addWidget(self._memory_info)
        self.v_box_layout.addWidget(self._tag_info)
        self.v_box_layout.addStretch(1)

        # 调用方法
        self.setup_tooltip()

        # 链接信号
        it(BotProcessManager).process_changed_signal.connect(self.slot_run_time_start)
        it(BotProcessManager).process_changed_signal.connect(self.slot_memory_usage_start)
        cfg.bot_memory_monitor_interval.valueChanged.connect(self._on_monitor_interval_changed)

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
            self._run_time_info.set_icon(FluentIcon.DATE_TIME, light="#176c3a", dark="#7ee2a8")
            # 判断 start_time 是否为 None, 为 None 代表第一次启动, 从 monotonic() 获取启动时间, 否则查找进程启动时间
            if self.start_time is None:
                process_model = it(BotProcessManager).get_process(qq_id)
                self.start_time = process_model.started_at if process_model is not None else monotonic()
            else:
                self.start_time = monotonic()

            # 检查是否已有计时器在运行
            if hasattr(self, "_run_time_timer"):
                self._run_time_timer.stop()
                self._run_time_timer.deleteLater()

            # 创建新的计时器 - 运行时长固定1秒更新
            timer = QTimer(self)

            # 每秒更新一次运行时长显示 格式 00:00:00
            timer.timeout.connect(
                lambda: self._run_time_info.text_label.setText(
                    (
                        f"{int(monotonic() - (self.start_time or 0.0))//3600:02}:"
                        f"{(int(monotonic() - (self.start_time or 0.0))%3600)//60:02}:"
                        f"{int(monotonic() - (self.start_time or 0.0))%60:02}"
                    )
                )
            )
            timer.start(1000)  # 固定1秒

            # 保存计时器引用
            self._run_time_timer = timer
        else:
            self._run_time_info.set_icon(FluentIcon.DATE_TIME, light="#6b7280", dark="#cbd5e1")
            if hasattr(self, "_run_time_timer"):
                self._run_time_timer.stop()
                self._run_time_timer.deleteLater()
                del self._run_time_timer

            self._run_time_info.text_label.setText("未运行")

    def slot_memory_usage_start(self, qq_id: str, state: QProcess.ProcessState) -> None:
        """处理内存占用开始更新槽函数 (2026-05-11 改异步 worker, 避免阻塞主线程).

        历史 (主线程同步): 旧版定时器直接 ``lambda: setText(get_memory_usage(qq_id))``,
        在 SnowLuma 热启动场景下 ``get_memory_usage`` → ``NapCatDriver.get_memory_usage_for_pid``
        → 用 ``psutil`` walk QQ.exe Electron 多子进程树 (5-15 个子进程, 每个一次 syscall),
        累计 50-200ms 阻塞 UI 线程; 加上 ``psutil.virtual_memory().total`` 又是一次 syscall.
        用户实测**热启动后 UI 明显卡顿**.

        现在 (异步 worker): 每次 tick 派发 :class:`_MemoryUsageWorker` 到 ``QThreadPool``,
        worker 跑 psutil walk 在后台线程, 完成后 emit 信号回主线程更新 ``text_label``.
        ``virtual_memory().total`` (系统 RAM, session 内不变) 在模块初始化时缓存一次,
        不再每 tick 调.
        ``_memory_in_flight`` 守护防止 worker 堆积 (psutil 偶发慢响应时跳过本 tick).
        """
        if qq_id != str(self._config.bot.QQID):
            return

        if state == QProcess.ProcessState.Running:
            # 检查是否已有计时器在运行
            if hasattr(self, "_memory_timer"):
                self._memory_timer.stop()
                self._memory_timer.deleteLater()

            # 创建新的计时器
            timer = QTimer(self)
            # 使用配置的更新间隔
            update_interval = cfg.get(cfg.bot_memory_monitor_interval)
            self._memory_in_flight = False  # in-flight 守护
            # 每隔指定时间触发异步 worker 算 memory; 不再在主线程同步 walk psutil.
            timer.timeout.connect(self._schedule_memory_update)
            timer.start(update_interval)
            # 保存计时器引用
            self._memory_timer = timer
        else:
            if hasattr(self, "_memory_timer"):
                self._memory_timer.stop()
                self._memory_timer.deleteLater()
                del self._memory_timer

            self._memory_info.text_label.setText("-M / -M")

    def _schedule_memory_update(self) -> None:
        """定时器回调: 派发 :class:`_MemoryUsageWorker` 到 ``QThreadPool``.

        in-flight 守护: 若上次 worker 未完成 (psutil 调用偶发慢响应或 QQ 进程树极大),
        跳过本次 tick 避免 worker 堆积. UI 文本保持上次值不动, 用户感知 = 这秒数据
        晚一秒更新, 不影响整体监控可用性.
        """
        if self._memory_in_flight:
            return
        self._memory_in_flight = True
        qq_id = str(self._config.bot.QQID)
        worker = _MemoryUsageWorker(qq_id)
        worker.finished.connect(self._update_memory_text)
        QThreadPool.globalInstance().start(worker)

    @Slot(str, int, int)
    def _update_memory_text(self, qq_id: str, mem_mb: int, total_mb: int) -> None:
        """``_MemoryUsageWorker.finished`` 信号槽 (主线程): 更新内存显示文本.

        worker 在工作线程算完 ``mem_mb`` (Bot 进程树 RSS) + ``total_mb`` (远端服务器
        RAM 或本地 psutil) 后 emit ``(qq_id, mem_mb, total_mb)``; 本 slot 仅做字符串
        拼接 + ``setText``, 不再调 psutil.

        修复 (2026-05-12): 远端 Bot 卡片此前总内存走的是 ``_total_memory_mb()`` (本机
        psutil), 现在改成 worker 根据 backend 提供的 ``server_total_memory_bytes`` 给
        出正确值; 本地 Bot 仍走 psutil, ``BotProcessManager.get_total_memory_mb`` 内部
        分发.
        """
        if qq_id != str(self._config.bot.QQID):
            return
        # shutdown race: 计时器停了但 worker 仍在跑完后 emit, _memory_info 可能已 deleteLater
        try:
            self._memory_info.text_label.setText(f"{mem_mb} MB / {total_mb} MB")
        except RuntimeError:
            # 底层 C++ 对象已销毁 (用户关窗 / Bot card 被移除), 静默忽略
            pass
        finally:
            self._memory_in_flight = False

    def _on_monitor_interval_changed(self, interval_ms: int) -> None:
        """监控间隔配置变化时更新定时器 (仅更新内存监控) """
        # 只更新内存监控定时器, 运行时长固定1秒
        timer = getattr(self, "_memory_timer", None)
        if timer is None:
            return
        try:
            if timer.isActive():
                timer.setInterval(interval_ms)
        except RuntimeError:
            # 底层 C++ 对象已被 deleteLater 释放, 忽略本次更新
            pass


class EnableTag(PillPushButton):
    """显示启用/禁用状态的标签控件"""

    @singledispatchmethod
    def __init__(self, status: bool, parent: QObject | None = None) -> None:
        """初始化启用标签

        Args:
            status: 初始状态, True 为启用, False 为禁用
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
            status: 新的状态值, True 为启用, False 为禁用
        """
        if status:
            self.setText(self.tr("启用"))
        else:
            self.setText(self.tr("禁用"))


class FormateTag(PillPushButton):
    """消息格式显示标签控件"""

    @singledispatchmethod
    def __init__(self, format_str: str, parent: QObject | None = None) -> None:
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
    """配置卡片基类, 提供通用的配置显示和操作功能"""

    remove_signal = Signal(NetworkBaseConfig)

    @singledispatchmethod
    def __init__(self, config: NetworkBaseConfig, parent: QObject | None = None) -> None:
        """初始化配置卡片基类

        Args:
            config: 配置数据模型
            parent: 父控件
        """
        super().__init__(parent)

        # 属性
        self.config = config
        self.config_view: QWidget
        self.config_view_layout: QGridLayout
        self.remove_button: TransparentToolButton
        self.edit_button: TransparentToolButton

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

    def _get_current_backend(self) -> BackendType:
        """P2 (Tier A): 走父控件链找 ConnectConfigWidget._current_backend.

        ConnectConfigWidget 是 cards 的祖先 (cards 加进 view 里, view 是
        ConnectConfigWidget 的子控件). 走 parent() 链直到找到带
        ``_current_backend`` 属性的对象, 没有则降级为 NAPCAT 与 P1 行为一致.
        """
        node = self.parent()
        while node is not None:
            backend = getattr(node, "_current_backend", None)
            if isinstance(backend, BackendType):
                return backend
            node = node.parent() if hasattr(node, "parent") else None
        return BackendType.NAPCAT


class HttpServerConfigCard(ConfigCardBase):
    """HTTP 服务器配置卡片"""

    @singledispatchmethod
    def __init__(self, config: HttpServersConfig, parent: QWidget | None = None) -> None:
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
        # P2 (Tier A): 按当前 backend 显隐 dialog 字段
        dialog.apply_backend_type(self._get_current_backend())
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class HttpSSEConfigCard(ConfigCardBase):
    """HTTP SSE 服务器配置卡片"""

    @singledispatchmethod
    def __init__(self, config: HttpSseServersConfig, parent: QWidget | None = None) -> None:
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

    @singledispatchmethod
    def __init__(self, config: HttpClientsConfig, parent: QWidget | None = None) -> None:
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
        # P2 (Tier A): 按当前 backend 显隐 dialog 字段
        dialog.apply_backend_type(self._get_current_backend())
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class WebsocketServersConfigCard(ConfigCardBase):
    """WebSocket 服务器配置卡片"""

    @singledispatchmethod
    def __init__(self, config: WebsocketServersConfig, parent: QWidget | None = None) -> None:
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
        # P2 (Tier A): 按当前 backend 显隐 dialog 字段
        dialog.apply_backend_type(self._get_current_backend())
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()


class WebsocketClientConfigCard(ConfigCardBase):
    """WebSocket 客户端配置卡片"""

    @singledispatchmethod
    def __init__(self, config: WebsocketClientsConfig, parent: QWidget | None = None) -> None:
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
        # P2 (Tier A): 按当前 backend 显隐 dialog 字段
        dialog.apply_backend_type(self._get_current_backend())
        if dialog.exec():
            self.config = dialog.get_config()
            self.fill_config()
