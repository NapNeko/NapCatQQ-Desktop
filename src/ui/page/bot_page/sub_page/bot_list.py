# -*- coding: utf-8 -*-
"""这是 Bot 列表子页面模块"""

# 第三方库导入
from creart import it
from qfluentwidgets import (
    FlowLayout,
    FluentIcon,
    PrimaryToolButton,
    ScrollArea,
    ToolButton,
    TransparentToolButton,
)
from PySide6.QtCore import QSize, Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.config.operate_config import delete_config, read_config
from src.core.logging.crash_bundle import mask_qqid
from src.core.logging import LogSource, logger
from src.core.operation.batch_dispatcher import BatchDispatcher, BatchOutcome
from src.core.runtime.napcat import ManagerAutoRestartProcess, ManagerNapCatQQProcess
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
        """构造顶部批量操作工具条 (默认隐藏)."""
        bar = QWidget(self)
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(8)

        self.batch_count_label = ToolButton(FluentIcon.PEOPLE, bar)
        self.batch_count_label.setEnabled(False)
        self.batch_count_label.setToolTip("已选数量")

        self.batch_select_all_btn = TransparentToolButton(FluentIcon.ACCEPT, bar)
        self.batch_select_all_btn.setToolTip(self.tr("全选"))
        self.batch_select_none_btn = TransparentToolButton(FluentIcon.CLOSE, bar)
        self.batch_select_none_btn.setToolTip(self.tr("取消全选"))
        self.batch_start_btn = TransparentToolButton(FluentIcon.PLAY, bar)
        self.batch_start_btn.setToolTip(self.tr("批量启动"))
        self.batch_stop_btn = TransparentToolButton(FluentIcon.PAUSE, bar)
        self.batch_stop_btn.setToolTip(self.tr("批量停止"))
        self.batch_delete_btn = TransparentToolButton(FluentIcon.DELETE, bar)
        self.batch_delete_btn.setToolTip(self.tr("批量删除"))
        self.batch_exit_btn = TransparentToolButton(FluentIcon.RETURN, bar)
        self.batch_exit_btn.setToolTip(self.tr("退出批量模式"))

        layout.addWidget(self.batch_count_label)
        layout.addStretch(1)
        layout.addWidget(self.batch_select_all_btn)
        layout.addWidget(self.batch_select_none_btn)
        layout.addWidget(self.batch_start_btn)
        layout.addWidget(self.batch_stop_btn)
        layout.addWidget(self.batch_delete_btn)
        layout.addWidget(self.batch_exit_btn)

        # 接线
        self.batch_select_all_btn.clicked.connect(lambda: self._set_all_batch_checked(True))
        self.batch_select_none_btn.clicked.connect(lambda: self._set_all_batch_checked(False))
        self.batch_start_btn.clicked.connect(self.slot_batch_start)
        self.batch_stop_btn.clicked.connect(self.slot_batch_stop)
        self.batch_delete_btn.clicked.connect(self.slot_batch_delete)
        self.batch_exit_btn.clicked.connect(lambda: self.set_batch_mode(False))

        bar.setParent(self)
        bar.hide()
        return bar

    # ==================== 重写方法 ===================
    def resizeEvent(self, event):
        super().resizeEvent(event)
        width = self.width() - self.add_button.width()
        height = self.height() - self.add_button.height()
        self.add_button.move(width - 16, height - 32)
        self.update_button.move(width - 16, height - 82)
        self.batch_toggle_button.move(width - 16, height - 132)
        # P4 F2: 批量工具条贴顶
        if self.batch_toolbar is not None:
            self.batch_toolbar.setGeometry(0, 0, self.width(), 40)

    # ==================== 公共方法 ====================
    def update_bot_list(self) -> None:
        """刷新 Bot 列表

        用于刷新 view 中的 Bot Card
        """
        # 判断原有 bot config list 是否为空, 不为空则清空
        if self._bot_card_list:
            self.remove_all_bot()

        # 读取配置文件
        if (configs := read_config()) == self._bot_config_list:
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
            # P4 F2: 监听批量复选框变化, 同步刷新工具条已选数量
            card.batch_check_changed_signal.connect(self._on_card_batch_check_changed)
            # P4 F2: 列表刷新时若处于批量模式, 立即让新卡片显示复选框
            if self._batch_mode:
                card.set_batch_mode(True)
            card.update_info_card()
            self._bot_card_list.append(card)
            self.view_layout.addWidget(card)
        if self._batch_mode:
            self._refresh_batch_count_label()

    def _is_card_alive(self, card: BotCard) -> bool:
        """判断 Bot Card 是否仍然有效。"""
        try:
            card.parent()
        except RuntimeError:
            logger.warning("检测到已失效的 Bot 卡片引用，已跳过处理", log_source=LogSource.UI)
            return False

        return True

    def _dispose_card(self, card: BotCard) -> None:
        """安全移除单个 Bot Card。"""
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
        """切换批量模式; 显示/隐藏所有卡片复选框 + 顶部工具条."""
        self._batch_mode = enabled
        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue
            card.set_batch_mode(enabled)
        if enabled:
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

    def _on_card_batch_check_changed(self, qq_id: str, checked: bool) -> None:
        """单张卡片复选框变化时刷新工具条已选数量."""
        del qq_id, checked
        self._refresh_batch_count_label()

    def _refresh_batch_count_label(self) -> None:
        selected = sum(1 for c in self._bot_card_list if c.is_batch_selected())
        self.batch_count_label.setToolTip(self.tr(f"已选 {selected} / {len(self._bot_card_list)}"))
        # 让按钮也表达数量, 不破坏既有 icon API: 利用 isEnabled 间接示意有无选中
        self.batch_count_label.setEnabled(False)
        # 启停 / 删除按钮无勾选时灰掉
        any_selected = selected > 0
        self.batch_start_btn.setEnabled(any_selected)
        self.batch_stop_btn.setEnabled(any_selected)
        self.batch_delete_btn.setEnabled(any_selected)

    def _set_all_batch_checked(self, checked: bool) -> None:
        for card in self._bot_card_list:
            if not self._is_card_alive(card):
                continue
            card.set_batch_mode(True, checked=checked)
        self._refresh_batch_count_label()

    # =================== P4 F2: 批量动作 ==============================
    def slot_batch_start(self) -> None:
        """批量启动选中的 Bot."""
        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要启动的 Bot"), parent=self)
            return

        process_manager = it(ManagerNapCatQQProcess)
        items: list[tuple[str, callable]] = []
        for config in configs:
            qq_id = str(config.bot.QQID)
            if process_manager.get_process(qq_id) is not None:
                logger.trace(f"批量启动跳过 (已运行): {mask_qqid(qq_id)}", log_source=LogSource.UI)
                continue

            def _op(cfg=config) -> None:
                process_manager.create_napcat_process(cfg)

            items.append((qq_id, _op))

        if not items:
            info_bar(self.tr("已选 Bot 全部在运行, 无需重复启动"), parent=self)
            return

        dispatcher = it(BatchDispatcher)
        try:
            dispatcher.finished_signal.disconnect(self._on_batch_finished)
        except (TypeError, RuntimeError):
            pass
        dispatcher.finished_signal.connect(self._on_batch_finished)
        dispatcher.dispatch(self.tr("批量启动"), items, sequential=False)

    def slot_batch_stop(self) -> None:
        """批量停止选中的 Bot."""
        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要停止的 Bot"), parent=self)
            return

        process_manager = it(ManagerNapCatQQProcess)
        items: list[tuple[str, callable]] = []
        for config in configs:
            qq_id = str(config.bot.QQID)
            if process_manager.get_process(qq_id) is None:
                continue

            def _op(qid=qq_id) -> None:
                process_manager.stop_process(qid)
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
        dispatcher.dispatch(self.tr("批量停止"), items, sequential=False)

    def slot_batch_delete(self) -> None:
        """批量删除选中的 Bot 配置."""
        # 项目内模块导入: 延迟以避免 main_window 未就绪
        from src.ui.components.message_box import AskBox
        from src.ui.window.main_window.window import MainWindow

        configs = self.get_selected_configs()
        if not configs:
            warning_bar(self.tr("请先勾选要删除的 Bot"), parent=self)
            return

        process_manager = it(ManagerNapCatQQProcess)
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
                self.tr(errors_preview),
                parent=self,
            )
        # 删除场景需要重建列表; 启停场景由 process_changed_signal 已自动刷新卡片
        self.update_bot_list()

    # =================== 槽函数 =======================
    def slot_add_button(self) -> None:
        """添加按钮槽函数"""
        # 判断有没有安装 NapCatQQ
        from src.core.versioning import LocalVersionTask

        if LocalVersionTask().get_napcat_version():
            # 项目内模块导入
            from src.ui.page.bot_page import BotPage

            logger.trace("进入新增 Bot 配置流程", log_source=LogSource.UI)
            page = it(BotPage)
            page.view.setCurrentWidget(page.add_config_page)
            page.add_config_page.clear_config()
            page.header.setup_breadcrumb_bar(999)

        else:
            from src.ui.components.info_bar import warning_bar

            logger.warning("新增 Bot 配置被拒绝: 未检测到 NapCatQQ 安装", log_source=LogSource.UI)
            warning_bar("请先安装 NapCatQQ 后再添加 Bot 配置！")

