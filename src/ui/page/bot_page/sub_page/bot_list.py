# -*- coding: utf-8 -*-
"""这是 Bot 列表子页面模块"""

# 第三方库导入
from creart import it
from qfluentwidgets import (
    Action,
    BodyLabel,
    CommandBar,
    FlowLayout,
    FluentIcon,
    PrimaryToolButton,
    ScrollArea,
    ToolButton,
)
from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QFrame, QHBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.config.operate_config import consume_truncated_bots_warning, delete_config, read_config
from src.core.logging.crash_bundle import mask_qqid
from src.core.logging import LogSource, logger
from src.core.operation.batch_dispatcher import BatchDispatcher, BatchOutcome, inline_executor
from src.core.runtime.bot_process_manager import ManagerAutoRestartProcess, BotProcessManager
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar

from ..widget.card import BotCard


class BotListPage(ScrollArea):
    """Bot 列表子页面"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数"""
        super().__init__(parent)
        # 设置属性
        self._bot_config_list: list[Config] = []
        self._bot_card_list: list[BotCard] = []
        # P4 F2: 批量模式开关
        self._batch_mode: bool = False

        # 创建视图和布局
        self.view = QWidget(self)
        self.view_layout = FlowLayout(self.view)
        self.add_button = PrimaryToolButton(FluentIcon.ADD, self)
        self.update_button = ToolButton(FluentIcon.UPDATE, self)
        # P4 F2: 批量模式入口按钮 (悬浮于右下, 与 add_button 类似)
        self.batch_toggle_button = ToolButton(FluentIcon.MENU, self)
        # P4 F2: 批量操作工具条 (默认隐藏, 进入批量模式后从顶部出现)
        self.batch_toolbar = self._build_batch_toolbar()

        # 设置控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view_layout.setContentsMargins(0, 0, 0, 0)
        self.view_layout.setSpacing(4)

        self.add_button.setFixedSize(40, 40)
        self.add_button.setIconSize(QSize(20, 20))
        self.update_button.setFixedSize(40, 40)
        self.update_button.setIconSize(QSize(20, 20))
        self.batch_toggle_button.setFixedSize(40, 40)
        self.batch_toggle_button.setIconSize(QSize(20, 20))
        self.batch_toggle_button.setToolTip(self.tr("批量模式 (多选启停 / 删除)"))

        # 连接信号
        self.add_button.clicked.connect(self.slot_add_button)
        self.update_button.clicked.connect(self.update_bot_list)
        self.batch_toggle_button.clicked.connect(self.slot_toggle_batch_mode)

        # 调用方法
        self.update_bot_list()

    # ==================== P4 F2: 批量模式工具条 ====================
    def _build_batch_toolbar(self) -> QWidget:
        """构造顶部批量操作工具条 (默认隐藏).

        采用 [`CommandBar`](qfluentwidgets) 风格统一展示批量动作:
        左侧 "已选 N / M" 计数 Label, 中间 stretch, 右侧 CommandBar
        分组展示 [全选 | 取消全选] / [启动 | 停止 | 删除] / [退出].
        Action 比 ``ToolButton`` + 散乱 lambda 更易维护且响应窄宽时自动 overflow.
        """
        # 外层容器: 参考 CommandBarView 风格 - 白底 + 1px 边框 + 圆角, 视觉上像浮动 chip.
        # 不用 QGraphicsDropShadowEffect: 把 effect 套在 ScrollArea 内的子 widget 上会
        # 与 viewport 卡片的 painter 冲突 (Qt 报 "QPainter::begin: A paint device can
        # only be painted by one painter at a time" + "QWidgetEffectSourcePrivate::
        # pixmap: Painter not active"); 改用稍深的 border 模拟"浮起感".
        bar = QFrame(self)
        bar.setObjectName("batchToolbar")
        bar.setStyleSheet(
            "#batchToolbar {"
            "  background-color: white;"
            "  border: 1px solid rgba(0, 0, 0, 0.12);"
            "  border-radius: 10px;"
            "}"
        )

        layout = QHBoxLayout(bar)
        layout.setContentsMargins(14, 4, 6, 4)
        layout.setSpacing(8)

        # 已选计数 Label (CommandBar 不擅长展示纯文本, 用独立 BodyLabel)
        self.batch_count_label = BodyLabel(self.tr("已选 0 / 0"), bar)
        self.batch_count_label.setStyleSheet("color: rgba(0, 0, 0, 0.65);")

        # CommandBar: 分组承载所有批量动作
        self.batch_command_bar = CommandBar(bar)

        self.batch_select_all_action = Action(
            FluentIcon.ACCEPT, self.tr("全选"), self.batch_command_bar
        )
        self.batch_select_none_action = Action(
            FluentIcon.CLOSE, self.tr("取消全选"), self.batch_command_bar
        )
        self.batch_start_action = Action(
            FluentIcon.PLAY, self.tr("批量启动"), self.batch_command_bar
        )
        self.batch_stop_action = Action(
            FluentIcon.PAUSE, self.tr("批量停止"), self.batch_command_bar
        )
        self.batch_delete_action = Action(
            FluentIcon.DELETE, self.tr("批量删除"), self.batch_command_bar
        )
        self.batch_exit_action = Action(
            FluentIcon.RETURN, self.tr("退出批量模式"), self.batch_command_bar
        )

        # 选择 / 启停删 / 退出, 三组用 separator 分隔
        self.batch_command_bar.addAction(self.batch_select_all_action)
        self.batch_command_bar.addAction(self.batch_select_none_action)
        self.batch_command_bar.addSeparator()
        self.batch_command_bar.addAction(self.batch_start_action)
        self.batch_command_bar.addAction(self.batch_stop_action)
        self.batch_command_bar.addAction(self.batch_delete_action)
        self.batch_command_bar.addSeparator()
        self.batch_command_bar.addAction(self.batch_exit_action)

        # 信号接线: 用 Action.triggered 而非 ToolButton.clicked
        self.batch_select_all_action.triggered.connect(lambda: self._set_all_batch_checked(True))
        self.batch_select_none_action.triggered.connect(lambda: self._set_all_batch_checked(False))
        self.batch_start_action.triggered.connect(self.slot_batch_start)
        self.batch_stop_action.triggered.connect(self.slot_batch_stop)
        self.batch_delete_action.triggered.connect(self.slot_batch_delete)
        self.batch_exit_action.triggered.connect(lambda: self.set_batch_mode(False))

        layout.addWidget(self.batch_count_label, 0, Qt.AlignmentFlag.AlignVCenter)
        layout.addStretch(1)
        layout.addWidget(self.batch_command_bar, 0, Qt.AlignmentFlag.AlignVCenter)

        # CommandBar 初始 width=0 时会把全部 action 收进 overflow ⋯; 显式 resize 一次,
        # 让它按 action 总宽度展开 (后续随 toolbar resize 在 set_batch_mode 中再触发).
        self.batch_command_bar.resizeToSuitableWidth()

        bar.setParent(self)
        bar.hide()
        return bar

    # ==================== 重写方法 ===================
    # 批量工具栏几何参数; toolbar 浮动 overlay 在 BotListPage 中底部, 水平居中,
    # 距底部 ``_BATCH_TOOLBAR_BOTTOM`` 像 Material Design BottomBar; 尺寸跟随内容自适应.
    _BATCH_TOOLBAR_BOTTOM = 24

    def resizeEvent(self, event):
        super().resizeEvent(event)
        width = self.width() - self.add_button.width()
        height = self.height() - self.add_button.height()
        self.add_button.move(width - 16, height - 32)
        self.update_button.move(width - 16, height - 82)
        self.batch_toggle_button.move(width - 16, height - 132)
        # P4 F2: 批量工具条 - 自适应宽度, 中底部水平居中, 视觉上像悬浮 chip
        self._reposition_batch_toolbar()

    def _reposition_batch_toolbar(self) -> None:
        """根据内容 sizeHint 把 batch_toolbar 居中悬浮到底部 ``_BATCH_TOOLBAR_BOTTOM``.

        水平: 居中 (减去 bar 自身宽度的一半).
        垂直: 贴底, 距 BotListPage 底边 ``_BATCH_TOOLBAR_BOTTOM`` 像 BottomActionBar.
        """
        if self.batch_toolbar is None:
            return
        # adjustSize 让 layout 按内容自然撑开; 之后用 sizeHint 拿到目标尺寸
        self.batch_toolbar.adjustSize()
        bar_w = self.batch_toolbar.sizeHint().width()
        bar_h = self.batch_toolbar.sizeHint().height()
        # 容器极窄时退化为左对齐, 避免负数 x
        x = max(8, (self.width() - bar_w) // 2)
        # 容器极矮时退化为顶部, 避免负数 y
        y = max(8, self.height() - bar_h - self._BATCH_TOOLBAR_BOTTOM)
        self.batch_toolbar.setGeometry(x, y, bar_w, bar_h)

    # ==================== 公共方法 ====================
    def update_bot_list(self) -> None:
        """刷新 Bot 列表

        用于刷新 view 中的 Bot Card
        """
        # 判断原有 bot config list 是否为空, 不为空则清空
        if self._bot_card_list:
            self.remove_all_bot()

        # 读取配置文件
        configs = read_config()
        # 问题 3 修复 (层 3): 截断警告**必须先消费**, 避免下面 early-return 让 deferred
        # queue 在多次 update_bot_list 调用下累积. 即使列表无变化也要消费 (队列在 read_config
        # 中已 push, 不消费就泄漏).
        self._consume_and_emit_truncated_warning()
        if configs == self._bot_config_list:
            # 如果读取的配置文件与现有配置文件一致, 则跳过
            logger.trace("Bot 列表刷新跳过: 配置未发生变化", log_source=LogSource.UI)
            return
        else:
            # 不一致则赋值给属性
            self._bot_config_list = configs.copy()
            logger.trace(f"Bot 列表已刷新: count={len(self._bot_config_list)}", log_source=LogSource.UI)

        # 创建 Bot Card 并添加到布局
        for config in self._bot_config_list:
            card = BotCard(config)
            card.remove_signal.connect(self.remove_bot_by_qqid)
            # P4 F2 / P4 W4: 监听卡片选中态变化, 同步刷新工具条 "已选 N/M" + 启停删按钮可用性
            card.selected_changed_signal.connect(self._on_card_selected_changed)
            # P4 F2: 列表刷新时若处于批量模式, 立即让新卡片显示选中态光标
            if self._batch_mode:
                card.set_batch_mode(True)
            card.update_info_card()
            self._bot_card_list.append(card)
            self.view_layout.addWidget(card)
        if self._batch_mode:
            self._refresh_batch_count_label()

    def _consume_and_emit_truncated_warning(self) -> None:
        """问题 3 修复 (层 3): 消费 :func:`read_config` 的截断警告, 弹一次 ``warning_bar``.

        ``read_config`` 截断时把被隐藏的 QQID push 到 deferred queue, 这里消费后清空;
        用户后续刷新列表不再重复弹 (除非 read_config 又截断了新的 QQID).

        多次调用 :func:`update_bot_list` (例如自动刷新 + 手动刷新) 都会进这里, 但只要
        队列空就无声 return, 不会骚扰用户.
        """
        truncated = consume_truncated_bots_warning()
        if not truncated:
            return
        # 去重: 同一次 update_bot_list 内 read_config 只调一次, 不会有重复; 但防御一下
        # 万一未来有别处 push 多次的逻辑.
        unique_qqids = list(dict.fromkeys(truncated))
        masked = ", ".join(mask_qqid(q) for q in unique_qqids)
        warning_bar(
            self.tr(f"已隐藏 {len(unique_qqids)} 个超出 NTQQ 4 开上限的 Bot"),
            title=self.tr("Bot 数量超限"),
            parent=self,
        )

    def _is_card_alive(self, card: BotCard) -> bool:
        """判断 Bot Card 是否仍然有效. """
        try:
            card.parent()
        except RuntimeError:
            logger.warning("检测到已失效的 Bot 卡片引用，已跳过处理", log_source=LogSource.UI)
            return False

        return True

    def _dispose_card(self, card: BotCard) -> None:
        """安全移除单个 Bot Card. """
        if not self._is_card_alive(card):
            return

        try:
            self.view_layout.removeWidget(card)
            card.setParent(None)
            card.deleteLater()
        except RuntimeError:
            logger.warning("移除 Bot 卡片时检测到对象已失效，已跳过剩余清理", log_source=LogSource.UI)

    def remove_bot_by_qqid(self, qqid: str) -> None:
        """通过 QQID 移除 Bot Card

        用于移除 view 中指定 QQID 的 Bot Card
        """
        target_config = next((config for config in self._bot_config_list if str(config.bot.QQID) == qqid), None)
        if target_config is None:
            logger.warning(f"尝试移除不存在的 Bot 配置(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            error_bar(self.tr("未找到待移除的 Bot 配置"))
            return

        logger.info(f"准备移除 Bot 配置(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
        if not delete_config(target_config):
            logger.error(f"移除 Bot 配置失败(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            error_bar(self.tr("移除 Bot 配置失败"))
            return

        self._bot_config_list = [config for config in self._bot_config_list if str(config.bot.QQID) != qqid]

        target_card: BotCard | None = None
        remaining_cards: list[BotCard] = []

        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue

            if target_card is None and str(card._config.bot.QQID) == qqid:
                target_card = card
                continue

            remaining_cards.append(card)

        self._bot_card_list = remaining_cards

        if target_card is None:
            logger.warning(f"Bot 卡片不存在或已失效(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)
            return

        self._dispose_card(target_card)
        logger.info(f"Bot 卡片已从列表移除(QQID: {mask_qqid(qqid)})", log_source=LogSource.UI)

    def remove_all_bot(self) -> None:
        """移除所有 Bot Card

        用于移除 view 中的所有 Bot Card
        """
        for card in self._bot_card_list:
            self._dispose_card(card)

        self._bot_card_list.clear()
        self._bot_config_list.clear()

    # =================== P4 F2: 批量模式公共接口 =======================
    def set_batch_mode(self, enabled: bool) -> None:
        """切换批量模式; 显示/隐藏所有卡片复选框 + 顶部工具条.

        ``batch_toolbar`` 通过绝对定位 ``setGeometry`` 浮动在 BotListPage 顶部
        (``setParent(self)`` + ``raise_()``), **不**修改 view_layout 的 margin,
        因此卡片不会被挤下去; toolbar 直接 overlay 在最上层.
        """
        self._batch_mode = enabled
        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue
            card.set_batch_mode(enabled)
        if enabled:
            # 先 resize CommandBar 让所有 actions 撑开 (避免 ⋯ 全 collapse),
            # 再 reposition toolbar 拿到正确的 sizeHint 居中悬浮.
            self.batch_command_bar.resizeToSuitableWidth()
            self._reposition_batch_toolbar()
            self.batch_toolbar.show()
            self.batch_toolbar.raise_()
            self._refresh_batch_count_label()
        else:
            self.batch_toolbar.hide()

    def is_batch_mode(self) -> bool:
        """当前是否处于批量模式."""
        return self._batch_mode

    def get_selected_configs(self) -> list[Config]:
        """返回所有当前批量勾选的 Bot 配置."""
        return [card._config for card in self._bot_card_list if card.is_batch_selected()]

    def slot_toggle_batch_mode(self) -> None:
        """点击右下批量按钮: 切换批量模式."""
        self.set_batch_mode(not self._batch_mode)

    def _on_card_selected_changed(self, qq_id: str, selected: bool) -> None:
        """单张卡片选中态变化时刷新工具条已选数量."""
        del qq_id, selected
        self._refresh_batch_count_label()

    def _refresh_batch_count_label(self) -> None:
        selected = sum(1 for c in self._bot_card_list if c.is_batch_selected())
        total = len(self._bot_card_list)
        # BodyLabel 直接显示 "已选 N / M", 比 ToolButton+tooltip 更直观
        self.batch_count_label.setText(self.tr(f"已选 {selected} / {total}"))
        # 启停 / 删除按钮无勾选时灰掉; CommandBar 通过 Action.setEnabled 控制
        any_selected = selected > 0
        self.batch_start_action.setEnabled(any_selected)
        self.batch_stop_action.setEnabled(any_selected)
        self.batch_delete_action.setEnabled(any_selected)

    def _set_all_batch_checked(self, checked: bool) -> None:
        """全选 / 取消全选: 对所有卡片调用 ``set_selected``; 信号会被去重 (相同值不发)."""
        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue
            card.set_selected(checked)
        # set_selected 已经触发 selected_changed_signal -> _refresh_batch_count_label;
        # 但有些卡片可能已经处于目标状态被去重, 这里再补一次显式刷新更稳妥.
        self._refresh_batch_count_label()

    # =================== P4 F2: 批量动作 ==============================
    def slot_batch_start(self) -> None:
        """批量启动选中的 Bot.

        2026-05-11 (问题 4 修复): 含本地 SnowLuma Bot 时, 单次弹
        :class:`SnowLumaStartModeDialog` 让用户选启动模式, **一次性应用到所有勾选的
        SnowLuma Bot**. 热启动需为每个 Bot 单独选 PID, 批量模式下不支持; 用户选热启动
        时弹错误提示并 abort.

        - NapCat / 远端 Bot 不受影响, 永远走原始路径.
        - 用户取消模式选择 → abort 整个批量启动.
        """
        # 延迟导入避免循环 (snowluma_start_dialog 依赖 SnowLumaStartMode)
        from src.core.runtime.backend_type import BackendType
        from src.core.runtime.snowluma_driver import SnowLumaStartMode
        from src.ui.page.bot_page.widget.snowluma_start_dialog import SnowLumaStartModeDialog

        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要启动的 Bot"), parent=self)
            return

        process_manager = it(BotProcessManager)

        # 过滤已运行 Bot
        pending_configs: list[Config] = []
        for config in configs:
            qq_id = str(config.bot.QQID)
            if process_manager.get_process(qq_id) is not None:
                logger.trace(f"批量启动跳过 (已运行): {mask_qqid(qq_id)}", log_source=LogSource.UI)
                continue
            pending_configs.append(config)

        if not pending_configs:
            info_bar(self.tr("已选 Bot 全部在运行, 无需重复启动"), parent=self)
            return

        # 问题 4 修复: 检测勾选中的本地 SnowLuma Bot, 弹一次启动模式选择
        local_snowluma_configs = [
            c for c in pending_configs
            if not c.bot.is_remote and c.bot.backend_type == BackendType.SNOWLUMA
        ]
        snowluma_start_mode = SnowLumaStartMode.COLD_START
        if local_snowluma_configs:
            mode_dialog = SnowLumaStartModeDialog(self.window())
            if not mode_dialog.exec():
                logger.info(
                    f"用户取消批量启动 SnowLuma 模式选择, abort "
                    f"(sl_bots={len(local_snowluma_configs)})",
                    log_source=LogSource.UI,
                )
                return
            snowluma_start_mode = mode_dialog.get_value()
            if snowluma_start_mode == SnowLumaStartMode.HOT_START:
                error_bar(
                    self.tr("批量启动不支持热启动, 请改用冷启动或单个启动"),
                    title=self.tr("批量启动失败"),
                    parent=self,
                )
                logger.info(
                    "批量启动 SnowLuma 热启动被拒绝, abort",
                    log_source=LogSource.UI,
                )
                return
            logger.info(
                f"批量启动 SnowLuma Bot 模式确认: "
                f"mode={snowluma_start_mode.value}, sl_bots={len(local_snowluma_configs)}",
                log_source=LogSource.UI,
            )

        # 构造 items: 本地 SnowLuma Bot 传 mode, NapCat / 远端走默认参数
        items: list[tuple[str, callable]] = []
        for config in pending_configs:
            qq_id = str(config.bot.QQID)
            is_local_sl = (
                not config.bot.is_remote
                and config.bot.backend_type == BackendType.SNOWLUMA
            )

            if is_local_sl:
                def _op(cfg=config, mode=snowluma_start_mode) -> None:
                    process_manager.start_bot(cfg, snowluma_start_mode=mode)
            else:
                def _op(cfg=config) -> None:
                    process_manager.start_bot(cfg)

            items.append((qq_id, _op))

        dispatcher = it(BatchDispatcher)
        try:
            dispatcher.finished_signal.disconnect(self._on_batch_finished)
        except (TypeError, RuntimeError):
            pass
        dispatcher.finished_signal.connect(self._on_batch_finished)
        # P4 修复: ``create_napcat_process`` 已是非阻塞操作 (本地 QProcess.start()
        # 不再 waitForStarted, 远端走自己的内部 QThreadPool runner). 强制走 inline
        # executor 让闭包在主线程执行, 否则本地 QProcess 会被构造在无事件循环的
        # 工作线程上, 导致 ``readyReadStandardOutput`` 永远不触发, BotLog 一片空白.
        dispatcher.dispatch(
            self.tr("批量启动"), items, sequential=False, executor=inline_executor
        )

    def slot_batch_stop(self) -> None:
        """批量停止选中的 Bot."""
        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要停止的 Bot"), parent=self)
            return

        process_manager = it(BotProcessManager)
        items: list[tuple[str, callable]] = []
        for config in configs:
            qq_id = str(config.bot.QQID)
            if process_manager.get_process(qq_id) is None:
                continue

            def _op(qid=qq_id) -> None:
                process_manager.stop_bot(qid)
                it(ManagerAutoRestartProcess).remove_auto_restart_timer(qid)

            items.append((qq_id, _op))

        if not items:
            info_bar(self.tr("已选 Bot 全部未运行, 无需停止"), parent=self)
            return

        dispatcher = it(BatchDispatcher)
        try:
            dispatcher.finished_signal.disconnect(self._on_batch_finished)
        except (TypeError, RuntimeError):
            pass
        dispatcher.finished_signal.connect(self._on_batch_finished)
        # P4 修复: 与 ``slot_batch_start`` 对称, 让 ``stop_process`` 在主线程执行,
        # 避免对 QProcess 的方法调用穿越线程边界.
        dispatcher.dispatch(
            self.tr("批量停止"), items, sequential=False, executor=inline_executor
        )

    def slot_batch_delete(self) -> None:
        """批量删除选中的 Bot 配置."""
        # 项目内模块导入: 延迟以避免 main_window 未就绪
        from src.ui.components.message_box import AskBox
        from src.ui.window.main_window.window import MainWindow

        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要删除的 Bot"), parent=self)
            return

        process_manager = it(BotProcessManager)
        running_names = [
            f"{c.bot.name} ({c.bot.QQID})"
            for c in configs
            if process_manager.get_process(str(c.bot.QQID)) is not None
        ]
        if running_names:
            warning_bar(
                self.tr(f"以下 Bot 正在运行, 请先停止: {', '.join(running_names)}"),
                parent=self,
            )
            return

        names = "\n".join(f"- {c.bot.name} ({c.bot.QQID})" for c in configs)
        if not AskBox(
            self.tr(f"确认批量删除 {len(configs)} 个 Bot"),
            self.tr(f"以下 Bot 配置将被永久删除, 此操作无法恢复:\n\n{names}"),
            it(MainWindow),
        ).exec():
            return

        items: list[tuple[str, callable]] = []
        for config in configs:
            qq_id = str(config.bot.QQID)

            def _op(cfg=config) -> None:
                if not delete_config(cfg):
                    raise RuntimeError(f"删除 Bot 配置失败 (QQID: {mask_qqid(str(cfg.bot.QQID))})")

            items.append((qq_id, _op))

        dispatcher = it(BatchDispatcher)
        try:
            dispatcher.finished_signal.disconnect(self._on_batch_finished)
        except (TypeError, RuntimeError):
            pass
        dispatcher.finished_signal.connect(self._on_batch_finished)
        # 删除走 sequential 避免 operate_config 并发写盘
        dispatcher.dispatch(self.tr("批量删除"), items, sequential=True)

    def _on_batch_finished(self, outcomes: list[BatchOutcome]) -> None:
        """所有批量任务完成后聚合 InfoBar + 刷新列表."""
        success = sum(1 for o in outcomes if o.ok)
        failed = len(outcomes) - success
        if failed == 0:
            success_bar(self.tr(f"批量操作完成: {success}/{len(outcomes)} 成功"), parent=self)
        else:
            errors_preview = "; ".join(
                f"{o.key}: {o.error}" for o in outcomes if not o.ok
            )[:200]
            error_bar(
                self.tr(f"批量操作部分失败: 成功 {success} / 失败 {failed}"),
                title=self.tr("批量操作"),
                parent=self,
            )
        # 删除场景需要重建列表; 启停场景由 process_changed_signal 已自动刷新卡片
        self.update_bot_list()

    # =================== 槽函数 =======================
    def slot_add_button(self) -> None:
        """添加按钮槽函数"""
        # 判断有没有安装本地后端 (NapCat 或 SnowLuma)
        from src.core.versioning import LocalVersionTask

        local_task = LocalVersionTask()
        has_napcat = bool(local_task.get_napcat_version())
        has_snowluma = bool(local_task.get_snowluma_version())

        if has_napcat or has_snowluma:
            # 项目内模块导入
            from src.ui.page.bot_page import BotPage

            logger.trace("进入新增 Bot 配置流程", log_source=LogSource.UI)
            page = it(BotPage)
            page.view.setCurrentWidget(page.add_config_page)
            page.add_config_page.clear_config()
            page.header.setup_breadcrumb_bar(999)

        else:
            from src.ui.components.info_bar import warning_bar

            logger.warning("新增 Bot 配置被拒绝: 未检测到本地后端安装", log_source=LogSource.UI)
            warning_bar(self.tr("请先安装本地后端 (NapCat 或 SnowLuma)"))

