# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
# 标准库导入
from enum import Enum
from typing import Callable, cast

# 第三方库导入
from creart import it
from pydantic import ValidationError
from qfluentwidgets import FluentIcon, SegmentedWidget, TransparentPushButton
from PySide6.QtCore import QThreadPool
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_model import RUNTIME_TARGET_LOCAL, AdvancedConfig, BotConfig, Config, ConnectConfig
from src.core.config.operate_config import merge_config_for_update, update_config
from src.core.logging import LogSource, logger
from src.core.logging.crash_bundle import mask_qqid
from src.core.operation.migration import BotMigrationRunnable, derive_plan_from_bot_config
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.bot_page.utils.enum import ConnectType
from src.ui.page.bot_page.widget import (
    ChooseConfigTypeDialog,
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.ui.page.bot_page.widget.config import AdvancedConfigWidget, BotConfigWidget, ConnectConfigWidget
from src.ui.page.bot_page.widget.migration_dialog import MigrationDialog


class ConfigPage(QWidget):
    """配置机器人页面"""

    _config: Config | None
    bot_widget: BotConfigWidget
    connect_widget: ConnectConfigWidget
    advanced_widget: AdvancedConfigWidget

    CONNECT_TYPE_AND_DIALOG = {
        ConnectType.HTTP_SERVER: HttpServerConfigDialog,
        ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
        ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
        ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
        ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
    }

    class PageEnum(Enum):
        """页面枚举"""

        BOT_WIDGET = 0
        CONNECT_WIDGET = 1
        ADVANCED_WIDGET = 2

    def __init__(self, parent: QWidget | None = None):
        """初始化页面"""
        super().__init__(parent)
        # 设置属性
        self._config = None

        # 创建控件
        self.piovt = SegmentedWidget(self)
        self.view = TransparentStackedWidget()
        self.bot_widget = cast(BotConfigWidget, BotConfigWidget(self))
        self.connect_widget = cast(ConnectConfigWidget, ConnectConfigWidget(self))
        self.advanced_widget = cast(AdvancedConfigWidget, AdvancedConfigWidget(self))
        self.return_button = TransparentPushButton(FluentIcon.LEFT_ARROW, self.tr("返回"), self)
        self.add_connect_button = TransparentPushButton(FluentIcon.ADD, self.tr("添加"), self)
        self.save_config_button = TransparentPushButton(FluentIcon.SAVE, self.tr("保存"), self)

        # 设置控件
        self.view.addWidget(self.bot_widget)
        self.view.addWidget(self.connect_widget)
        self.view.addWidget(self.advanced_widget)
        self.view.setCurrentWidget(self.bot_widget)

        self.piovt.addItem(
            routeKey=f"bot_widget",
            text=self.tr("基本配置"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_widget),
        )
        self.piovt.addItem(
            routeKey="connect_widget",
            text=self.tr("连接配置"),
            onClick=lambda: self.view.setCurrentWidget(self.connect_widget),
        )
        self.piovt.addItem(
            routeKey=f"advanced_widget",
            text=self.tr("高级配置"),
            onClick=lambda: self.view.setCurrentWidget(self.advanced_widget),
        )
        self.piovt.setCurrentItem("bot_widget")

        self.add_connect_button.hide()

        # 设置布局
        self.top_layout = QHBoxLayout()
        self.top_layout.setContentsMargins(0, 0, 0, 0)
        self.top_layout.addWidget(self.piovt)
        self.top_layout.addStretch(1)
        self.top_layout.addWidget(self.add_connect_button)
        self.top_layout.addWidget(self.save_config_button)
        self.top_layout.addWidget(self.return_button)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addLayout(self.top_layout)
        self.v_box_layout.addWidget(self.view, 1)

        # 链接信号
        self.view.currentChanged.connect(self.slot_view_current_index_changed)
        self.add_connect_button.clicked.connect(self.slot_add_connect_button)
        self.save_config_button.clicked.connect(self.slot_save_config_button)
        self.return_button.clicked.connect(self.slot_return_button)

    # ==================== 公共函数===================
    def get_config(self) -> Config:
        """返回所有配置"""
        return Config(
            **{
                "bot": self.bot_widget.get_config(),
                "connect": self.connect_widget.get_config(),
                "advanced": self.advanced_widget.get_config(),
            }
        )

    def fill_config(self, config: Config | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config.model_copy(deep=True)
        bot_config = cast(BotConfig, config.bot)
        connect_config = cast(ConnectConfig, config.connect)
        advanced_config = cast(AdvancedConfig, config.advanced)

        bot_widget = cast(BotConfigWidget, self.bot_widget)
        connect_widget = cast(ConnectConfigWidget, self.connect_widget)
        advanced_widget = cast(AdvancedConfigWidget, self.advanced_widget)

        fill_bot_config = cast(Callable[[BotConfig | None], None], bot_widget.fill_config)
        fill_connect_config = cast(Callable[[ConnectConfig | None], None], connect_widget.fill_config)
        fill_advanced_config = cast(Callable[[AdvancedConfig | None], None], advanced_widget.fill_config)

        fill_bot_config(bot_config)
        fill_connect_config(connect_config)
        fill_advanced_config(advanced_config)

    def clear_config(self) -> None:
        """清空配置"""
        self._config = None
        self.bot_widget.clear_config()
        self.connect_widget.clear_config()
        self.advanced_widget.clear_config()

    # ==================== 槽函数 ====================
    def slot_view_current_index_changed(self, index: int) -> None:
        """当 view 切换时更新 piovt 的选中状态

        Args:
            index (int): 当前索引
        """
        match self.PageEnum(index):
            case self.PageEnum.BOT_WIDGET:
                self.piovt.setCurrentItem("bot_widget")
                self.add_connect_button.hide()
            case self.PageEnum.CONNECT_WIDGET:
                self.piovt.setCurrentItem("connect_widget")
                self.add_connect_button.show()
            case self.PageEnum.ADVANCED_WIDGET:
                self.piovt.setCurrentItem("advanced_widget")
                self.add_connect_button.hide()

    def slot_return_button(self) -> None:
        """返回按钮槽函数"""
        # 项目内模块导入
        from src.ui.page.bot_page import BotPage

        logger.trace("Bot 配置页返回到 Bot 列表", log_source=LogSource.UI)
        page = it(BotPage)
        page.view.setCurrentWidget(page.bot_list_page)

    def slot_save_config_button(self) -> None:
        """保存按钮槽函数"""
        try:
            config = self.get_config()
        except (ValidationError, ValueError) as exc:
            logger.warning(f"配置校验失败: {exc}", log_source=LogSource.UI)
            error_bar(self.tr("配置校验失败，请检查输入内容"))
            return

        merged_config = merge_config_for_update(config, base_config=self._config)

        logger.info(
            (
                "准备保存 Bot 配置: "
                f"QQID={mask_qqid(merged_config.bot.QQID)}, "
                f"http_servers={len(merged_config.connect.httpServers)}, "
                f"http_sse_servers={len(merged_config.connect.httpSseServers)}, "
                f"http_clients={len(merged_config.connect.httpClients)}, "
                f"ws_servers={len(merged_config.connect.websocketServers)}, "
                f"ws_clients={len(merged_config.connect.websocketClients)}"
            ),
            log_source=LogSource.UI,
        )

        # P3.W3.B: 检测 runtime_target 变化 -> 走迁移路径
        old_target = (
            self._config.bot.runtime_target if self._config is not None else RUNTIME_TARGET_LOCAL
        )
        new_target = merged_config.bot.runtime_target or RUNTIME_TARGET_LOCAL
        if self._config is not None and old_target != new_target:
            self._handle_target_migration(merged_config, old_target=old_target, new_target=new_target)
            return

        # 原路径: 直接保存 (未切换运行位置)
        self._persist_config(merged_config)

    # ==================== P3.W3.B: 运行位置迁移 ====================
    def _handle_target_migration(
        self,
        merged_config: Config,
        *,
        old_target: str,
        new_target: str,
    ) -> None:
        """弹 [`MigrationDialog`](src/ui/page/bot_page/widget/migration_dialog.py) 二次确认,
        接受后顺序: 停旧 Bot → 立即写盘新 ``runtime_target`` → 后台搬运配置文件.

        为什么是这个顺序 (P3.W3.B v2):
        - **先写盘后搬运**: 用户点"确认迁移"后立刻在主线程持久化新 ``runtime_target``,
          UI / bot 列表同步反映, 解决"看起来啥都没保存"的 bug
          (旧实现把 update_config 放到 runnable 完成后的 lambda 回调里,
           跨线程 + setAutoDelete 释放时机存在 GC race, 部分场景 lambda 不被回调)
        - **失败语义**: 后台搬运失败时, ``bot.json`` 已经是新 target,
          目标端可能缺 onebot11/napcat 文件; 用户重新点保存 (target 没变, 走原路径)
          或回切再切 (走反向迁移) 都能恢复. 这是可接受的弱一致, 远好于"啥都没动"
        """
        qq_id = str(merged_config.bot.QQID)
        source_label = self._format_target_label(old_target)
        dest_label = self._format_target_label(new_target)

        dialog = MigrationDialog(qq_id, source_label, dest_label, self.window())
        if not dialog.exec():
            error_bar(self.tr("已取消迁移; 本次修改未保存"))
            logger.info(
                f"用户取消运行位置迁移: qq_id={mask_qqid(qq_id)}, {old_target} -> {new_target}",
                log_source=LogSource.UI,
            )
            return
        move_data = dialog.get_move_persistent_data()

        # 主线程: 停止源端如果在跑; 失败不阻断迁移
        self._stop_bot_if_running_locally(qq_id)

        # ① 立即写盘 + 刷新 UI: 让 bot 列表 / 当前页面同步反映新 target
        if not self._persist_config(merged_config):
            # _persist_config 内部已 error_bar, 这里仅日志补充
            logger.warning(
                f"迁移前持久化配置失败, 已中止迁移: qq_id={mask_qqid(qq_id)}",
                log_source=LogSource.UI,
            )
            return

        # ② 后台 runnable 搬运 NapCat 配置文件 (onebot11/napcat JSON)
        plan = derive_plan_from_bot_config(
            qq_id=qq_id,
            old_target=old_target,
            new_target=new_target,
            move_persistent_data=move_data,
        )
        runnable = BotMigrationRunnable(plan)
        # 把 signals 挂到 self 持有的属性, 防止 lambda + runnable.setAutoDelete(True)
        # 触发的 GC race 把 finished slot 提前回收
        self._migration_signals = runnable.signals
        runnable.signals.progress.connect(self._on_migration_progress)
        runnable.signals.finished.connect(self._on_migration_finished)
        info_bar(self.tr(f"开始迁移: {source_label} → {dest_label}..."))
        logger.info(
            (
                f"启动运行位置迁移 (配置已先行写盘): qq_id={mask_qqid(qq_id)}, "
                f"{old_target} -> {new_target}, move_data={move_data}"
            ),
            log_source=LogSource.UI,
        )
        QThreadPool.globalInstance().start(runnable)

    def _on_migration_progress(self, message: str, percent: int) -> None:
        # 进度量轻, 在 UI 仅输出 trace 日志; 不弹频繁 InfoBar 避免骚扰
        logger.trace(f"迁移进度 {percent}%: {message}", log_source=LogSource.UI)

    def _on_migration_finished(self, ok: bool, message: str) -> None:
        """搬运 runnable 收尾回调; ``bot.json`` 已在 dispatch 之前写盘, 这里只做提示."""
        if ok:
            success_bar(self.tr(f"迁移完成: {message}"))
            logger.info(f"运行位置迁移成功: {message}", log_source=LogSource.UI)
        else:
            error_bar(
                self.tr(
                    f"迁移搬运失败 (配置已切换, 但目标端文件可能不全, 请重试): {message}"
                )
            )
            logger.warning(f"运行位置迁移搬运失败: {message}", log_source=LogSource.UI)
        # 释放对 signals 的强引用, 让 runnable 与 signals 进入正常 GC 路径
        self._migration_signals = None

    def _persist_config(self, merged_config: Config) -> bool:
        """走 update_config 并刷新 UI; 被原 saving 路径与 P3.W3.B 迁移路径复用.

        Returns:
            ``True`` 表示本地 ``bot.json`` 写盘成功 (远端同步失败仍算成功);
            ``False`` 表示本地写盘失败, 调用方应中止后续流程.
        """
        if update_config(merged_config, base_config=merged_config, skip_merge=True):
            from src.ui.page.bot_page import BotPage

            it(BotPage).bot_list_page.update_bot_list()
            self.fill_config(merged_config)
            logger.info(
                f"Bot 配置保存成功(QQID: {mask_qqid(merged_config.bot.QQID)})",
                log_source=LogSource.UI,
            )
            success_bar(self.tr("保存配置成功"))
            return True
        logger.error(
            f"Bot 配置保存失败(QQID: {mask_qqid(merged_config.bot.QQID)})",
            log_source=LogSource.UI,
        )
        error_bar(self.tr("保存配置文件时引发错误"))
        return False

    @staticmethod
    def _format_target_label(target: str) -> str:
        """把 ``runtime_target`` 转为人读名称; 远端尝试查 ServerProfile.name."""
        if not target or target == RUNTIME_TARGET_LOCAL:
            return "本地"
        try:
            from src.core.remote.server_manager import ServerManager

            manager = it(ServerManager)
            profile = manager.get_server(target)
            if profile is not None:
                return f"远端 [{profile.name}]"
        except Exception:  # noqa: BLE001 - creart 不可用时退化到 server_id
            pass
        return f"远端 [{target}]"

    def _stop_bot_if_running_locally(self, qq_id: str) -> None:
        """迁移前尝试停源端 Bot (本地 / 远端均可); 任何异常仅 warning 不阻迁移.

        [`ManagerNapCatQQProcess.stop_process`](src/core/runtime/napcat.py) 同时覆盖
        本地 ``QProcess`` 与远端 SSH 分支, 调用是幂等的 (Bot 未在跑时静默成功).
        """
        try:
            from src.core.runtime.napcat import ManagerNapCatQQProcess

            manager = it(ManagerNapCatQQProcess)
            manager.stop_process(qq_id)
        except Exception as exc:  # noqa: BLE001 - 停 Bot 失败不应阻断迁移
            logger.warning(
                f"迁移前停 Bot 失败 (忽略): qq_id={mask_qqid(qq_id)}, exc={exc}",
                log_source=LogSource.UI,
            )

    def slot_add_connect_button(self) -> None:
        """添加连接配置按钮槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        logger.trace("打开连接配置类型选择对话框", log_source=LogSource.UI)
        if not (_choose_connect_type_box := ChooseConfigTypeDialog(it(MainWindow))).exec():
            # 获取用户选择的结果并判断是否取消
            logger.trace("连接配置类型选择已取消", log_source=LogSource.UI)
            return

        if (_connect_type := _choose_connect_type_box.get_value()) == ConnectType.NO_TYPE:
            # 判断用户选择的类型, 如果没有选择则直接退出
            logger.trace("连接配置类型选择为空，终止添加流程", log_source=LogSource.UI)
            return

        dialog_class = self.CONNECT_TYPE_AND_DIALOG.get(_connect_type)
        if dialog_class is None:
            logger.warning(f"未找到连接配置对话框: type={_connect_type}", log_source=LogSource.UI)
            return

        def validate_name_conflict(name: str) -> str | None:
            if self.connect_widget.has_config_name(name):
                return self.tr("连接配置名称不能重复")
            return None

        _connect_config_box = dialog_class(it(MainWindow))
        _connect_config_box.set_name_conflict_validator(validate_name_conflict)
        if not _connect_config_box.exec():
            # 判断用户在配置的时候是否选择了取消
            logger.trace(f"连接配置填写已取消: type={_connect_type}", log_source=LogSource.UI)
            return

        # 拿到配置项添加卡片
        config = _connect_config_box.get_config()
        self.connect_widget.add_card(config)
        logger.info(f"连接配置已添加: type={type(config).__name__}, name={config.name}", log_source=LogSource.UI)

