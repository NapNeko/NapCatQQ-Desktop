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
from src.core.config.operate_config import merge_config_for_update, read_config_raw, update_config
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
        # 问题 2 修复: 配置热推送 signals 强引用, 防止 worker 还没跑完 signals 被 GC.
        # 在 _dispatch_hot_reload 中赋值, _on_hot_reload_finished 中清零.
        self._hot_reload_signals: object | None = None

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
        # P2 (Tier A): backend_type 切换 → ConnectConfigWidget / AdvancedConfigWidget 实时显隐
        self.bot_widget.backend_type_changed.connect(self.connect_widget.apply_backend_type)
        self.bot_widget.backend_type_changed.connect(self.advanced_widget.apply_backend_type)

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

        # P2 (Tier A): 首次加载也调一次 apply_backend_type, 保证初始可见性正确.
        # fill_config 期间 BotConfigWidget.fill_value 也会触发 backend_type_changed 信号,
        # 但仅在 ComboBox index 真实变动时才发; 直接显式调一次更可靠 (幂等).
        self.connect_widget.apply_backend_type(bot_config.backend_type)
        self.advanced_widget.apply_backend_type(bot_config.backend_type)

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

        # 问题 3 修复 (层 2: 添加 Bot 时拒绝超 4 个):
        # 新增模式下 (self._config is None) 检查现有 Bot 总数, >= 4 时拒绝.
        # 编辑模式 (self._config is not None) 不受此限制 (用户修改现有 Bot 不增加总数).
        # 注意: 这里读 read_config_raw 拿**完整**列表 (未经截断), 确保用户即使手动改了 bot.json
        # 也能正确反映真实总数. NTQQ 多开真实上限 4 个 — 详见 BotProcessManager.LOCAL_BOT_LIMIT.
        if self._config is None:
            existing_count = len(read_config_raw())
            if existing_count >= 4:
                logger.warning(
                    f"新增 Bot 被拒: 现有 Bot 总数 {existing_count} 已达上限 4 个 (NTQQ 多开限制)",
                    log_source=LogSource.UI,
                )
                error_bar(
                    self.tr("Bot 数量已达上限"),
                    self.tr(
                        f"已有 {existing_count} 个 Bot 配置, 达到 NTQQ 多开 4 个上限. "
                        "请先删除部分 Bot 配置后再新增."
                    ),
                )
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
            # 2026-05-12: backend_type ↔ backend_flavor 兼容性校验
            if not self._check_backend_flavor_compatibility(merged_config.bot.backend_type, new_target):
                return
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

        dialog = MigrationDialog(qq_id, source_label, dest_label, self.window(), backend_type=merged_config.bot.backend_type)
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

        # ② 后台 runnable 搬运配置文件 (NC: onebot11/napcat JSON; SL: onebot JSON)
        plan = derive_plan_from_bot_config(
            qq_id=qq_id,
            old_target=old_target,
            new_target=new_target,
            move_persistent_data=move_data,
            backend_type=merged_config.bot.backend_type,
        )
        runnable = BotMigrationRunnable(plan)
        # 把 signals 挂到 self 持有的属性, 防止 lambda + runnable.setAutoDelete(True)
        # 触发的 GC race 把 finished slot 提前回收
        self._migration_signals = runnable.signals
        runnable.signals.progress.connect(self._on_migration_progress)
        runnable.signals.finished.connect(self._on_migration_finished)
        # P3 perf: 进度 / 完成反馈走 ProgressInfoBar 桥
        # ([`BotMigrationRunnable`](src/core/operation/migration.py) 自带 begin/end 上报),
        # 这里不再叠加 info_bar 的"开始迁移"提示.
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
        """搬运 runnable 收尾回调; ``bot.json`` 已在 dispatch 之前写盘.

        P3 perf: 成败反馈交由 ProgressInfoBar 桥
        ([`BotMigrationRunnable.run`](src/core/operation/migration.py) 在 finally 里
        把 success/message 推到 [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py));
        这里仅记录日志 + 释放 signals 引用.
        """
        if ok:
            logger.info(f"运行位置迁移成功: {message}", log_source=LogSource.UI)
        else:
            logger.warning(f"运行位置迁移搬运失败: {message}", log_source=LogSource.UI)
        # 释放对 signals 的强引用, 让 runnable 与 signals 进入正常 GC 路径
        self._migration_signals = None

    def _persist_config(self, merged_config: Config) -> bool:
        """走 update_config 并刷新 UI; 被原 saving 路径与 P3.W3.B 迁移路径复用.

        Returns:
            ``True`` 表示本地 ``bot.json`` 写盘成功 (远端同步失败仍算成功);
            ``False`` 表示本地写盘失败, 调用方应中止后续流程.

        P3 perf: 远端 Bot 的"保存"语义包含本地写盘 + 远端 SFTP 同步两步, 远端那步
        会异步派发给 ``_RemoteConfigOpRunnable`` → ProgressInfoBar 桥. 如果这里
        立刻弹 ``success_bar("保存配置成功")`` 会让用户在远端 SSH 还在跑时就误判完成,
        因此远端 Bot 路径**不弹**本地 success_bar, 完整反馈交由桥; 本地 Bot
        (``runtime_target == RUNTIME_TARGET_LOCAL``) 不走桥, 仍弹 success_bar.

        2026-05-11 (问题 2 修复): 本地 Bot 写盘成功后, 调用 :func:`push_hot_reload`
        把新配置 fire-and-forget 推送给在跑的 Bot 后端 (NapCat / SnowLuma WebUI 接口),
        无需用户重启 Bot. 推送完成后由 :meth:`_on_hot_reload_finished` 弹相应提示.
        """
        if update_config(merged_config, base_config=merged_config, skip_merge=True):
            from src.ui.page.bot_page import BotPage

            it(BotPage).bot_list_page.update_bot_list()
            self.fill_config(merged_config)
            logger.info(
                f"Bot 配置保存成功(QQID: {mask_qqid(merged_config.bot.QQID)})",
                log_source=LogSource.UI,
            )
            target = merged_config.bot.runtime_target or RUNTIME_TARGET_LOCAL
            if target == RUNTIME_TARGET_LOCAL:
                success_bar(self.tr("保存配置成功"))
                # 问题 2 修复: 本地 Bot 写盘成功后尝试热推送
                self._dispatch_hot_reload(merged_config)
            return True
        logger.error(
            f"Bot 配置保存失败(QQID: {mask_qqid(merged_config.bot.QQID)})",
            log_source=LogSource.UI,
        )
        error_bar(self.tr("保存配置文件时引发错误"))
        return False

    def _dispatch_hot_reload(self, merged_config: Config) -> None:
        """问题 2 修复: 把新配置 fire-and-forget 推送给在跑的本地 Bot.

        - 本地 Bot 在跑 → 走 NapCat / SnowLuma WebUI 热推送, ``_on_hot_reload_finished``
          根据结果弹 ``info_bar`` (热重载成功 / 失败 / 未登录);
        - Bot 未在跑 → :func:`push_hot_reload` 静默 no-op, 配置已落盘下次启动生效, 不弹提示;
        - 远端 Bot → 本函数不应被调用 (caller 已按 runtime_target 分流).

        signals 持强引用到 ``self._hot_reload_signals`` 防 PySide6 提前 GC.
        """
        from src.core.runtime.bot_hot_reload import HotReloadSignals, push_hot_reload

        signals = HotReloadSignals()
        signals.finished.connect(self._on_hot_reload_finished)
        # 持强引用避免 worker 还没跑完 signals 就被回收
        self._hot_reload_signals = signals
        try:
            submitted = push_hot_reload(merged_config, signals)
        except Exception as exc:  # noqa: BLE001 - 热推送提交失败不应阻塞 UI
            logger.warning(
                f"配置热推送提交失败 (qq_id={mask_qqid(merged_config.bot.QQID)}): "
                f"{type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )
            self._hot_reload_signals = None
            return
        if not submitted:
            # fast-skip (未在跑等), 释放 signals
            self._hot_reload_signals = None

    def _on_hot_reload_finished(self, qq_id: str, result: object) -> None:
        """worker 完成后回调; 根据 :class:`HotReloadResult` 弹通知.

        所有路径都在结尾释放 ``self._hot_reload_signals`` 强引用; signals 进入 Python
        GC 通道, Qt 那边随后随 deleteLater 释放.
        """
        from src.core.runtime.bot_hot_reload import HotReloadResult

        try:
            if not isinstance(result, HotReloadResult):
                logger.warning(
                    f"热推送 finished signal payload 异常 (qq_id={mask_qqid(qq_id)}): "
                    f"type={type(result).__name__}",
                    log_source=LogSource.UI,
                )
                return

            masked = mask_qqid(qq_id)
            if result.ok:
                if result.reloaded:
                    info_bar(self.tr(f"Bot {masked} 配置已热重载"))
                else:
                    # SnowLuma 返回 reloaded=False 时表示已落盘但 uin 未在线, 等价"重启生效".
                    info_bar(self.tr(f"Bot {masked} 配置已保存, 重启 Bot 后生效"))
            elif result.not_logged_in:
                info_bar(
                    self.tr(
                        f"Bot {masked} 未登录 QQ, 配置已保存, 扫码登录后重启 Bot 生效"
                    )
                )
            else:
                # 真正失败 (网络 / 后端拒绝 / 异常)
                info_bar(
                    self.tr(
                        f"Bot {masked} 配置已保存, 热推送失败, 请重启 Bot 生效: "
                        f"{result.error_message}"
                    )
                )
        finally:
            self._hot_reload_signals = None

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

        [`BotProcessManager.stop_bot`](src/core/runtime/napcat.py) 同时覆盖
        本地 ``QProcess`` 与远端 SSH 分支, 调用是幂等的 (Bot 未在跑时静默成功).
        """
        try:
            from src.core.runtime.bot_process_manager import BotProcessManager

            manager = it(BotProcessManager)
            manager.stop_bot(qq_id)
        except Exception as exc:  # noqa: BLE001 - 停 Bot 失败不应阻断迁移
            logger.warning(
                f"迁移前停 Bot 失败 (忽略): qq_id={mask_qqid(qq_id)}, exc={exc}",
                log_source=LogSource.UI,
            )

    def _check_backend_flavor_compatibility(self, backend_type: "BackendType", target: str) -> bool:
        """校验 Bot 的 backend_type 与目标服务器的 backend_flavor 是否兼容.

        - 目标为本地 (``RUNTIME_TARGET_LOCAL``): 始终兼容 (本地同时支持 NC 和 SL).
        - 目标为远端: ``backend_type`` 必须与 ``ServerProfile.backend_flavor`` 匹配
          (NAPCAT ↔ NAPCAT, SNOWLUMA ↔ SNOWLUMA); 不匹配时弹 error_bar 并返回 False.

        Returns:
            ``True`` 表示兼容, 可继续迁移; ``False`` 表示不兼容, 已弹提示, 调用方应 return.
        """
        if target == RUNTIME_TARGET_LOCAL:
            return True

        try:
            from src.core.remote.server_manager import ServerManager
            from src.core.remote.servers import BackendFlavor

            manager = it(ServerManager)
            profile = manager.get_server(target)
            if profile is None:
                # 服务器不存在: 让后续迁移流程自己报错, 这里不拦截
                return True

            # 兼容性映射: BackendType.NAPCAT ↔ BackendFlavor.NAPCAT,
            #             BackendType.SNOWLUMA ↔ BackendFlavor.SNOWLUMA
            from src.core.runtime.backend_type import BackendType

            flavor_map = {
                BackendType.NAPCAT: BackendFlavor.NAPCAT,
                BackendType.SNOWLUMA: BackendFlavor.SNOWLUMA,
            }
            expected_flavor = flavor_map.get(backend_type)
            if expected_flavor is not None and profile.backend_flavor != expected_flavor:
                bot_label = backend_type.display_name
                server_label = (
                    "NapCat" if profile.backend_flavor == BackendFlavor.NAPCAT else "SnowLuma"
                )
                error_bar(
                    self.tr("后端类型不匹配"),
                    self.tr(
                        f"Bot 后端为 {bot_label}, 但目标服务器 [{profile.name}] "
                        f"的后端为 {server_label}, 无法迁移. "
                        "请选择匹配的服务器或修改 Bot 后端类型."
                    ),
                )
                logger.warning(
                    f"迁移被拒: backend_type={backend_type.value} vs "
                    f"server_flavor={profile.backend_flavor.value} "
                    f"(server={profile.name}, id={target})",
                    log_source=LogSource.UI,
                )
                return False
        except Exception as exc:  # noqa: BLE001 - ServerManager 不可用时不阻断
            logger.warning(
                f"backend_flavor 兼容性校验异常 (放行): {type(exc).__name__}: {exc}",
                log_source=LogSource.UI,
            )
        return True

    def slot_add_connect_button(self) -> None:
        """添加连接配置按钮槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        # P2 (Tier A): 拿当前 backend_type 让 dialog 按 backend 显隐字段
        current_backend = self.bot_widget.get_config().backend_type

        logger.trace("打开连接配置类型选择对话框", log_source=LogSource.UI)
        _choose_connect_type_box = ChooseConfigTypeDialog(it(MainWindow))
        # P2 (Tier A): SnowLuma 模式下 ChooseConfigTypeDialog 隐藏 HTTP SSE 选项
        _choose_connect_type_box.apply_backend_type(current_backend)
        if not _choose_connect_type_box.exec():
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
        # P2 (Tier A): ConfigDialog 按 backend 显隐字段
        if hasattr(_connect_config_box, "apply_backend_type"):
            _connect_config_box.apply_backend_type(current_backend)
        _connect_config_box.set_name_conflict_validator(validate_name_conflict)
        if not _connect_config_box.exec():
            # 判断用户在配置的时候是否选择了取消
            logger.trace(f"连接配置填写已取消: type={_connect_type}", log_source=LogSource.UI)
            return

        # 拿到配置项添加卡片
        config = _connect_config_box.get_config()
        self.connect_widget.add_card(config)
        logger.info(f"连接配置已添加: type={type(config).__name__}, name={config.name}", log_source=LogSource.UI)

