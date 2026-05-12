# -*- coding: utf-8 -*-
"""
Bot 配置页面
"""
from __future__ import annotations

from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import ComboBox, ExpandLayout, FlowLayout, FluentIcon, PushButton, ScrollArea, SettingCard, SettingCardGroup
from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import (
    RUNTIME_TARGET_LOCAL,
    AdvancedConfig,
    BotConfig,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    HttpSseServersConfig,
    NetworkBaseConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.core.runtime.backend_type import BackendType
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.components.input_card import ComboBoxConfigCard, LineEditConfigCard, SwitchConfigCard, ShowDialogCard
from src.ui.page.bot_page.widget import (
    HttpClientConfigCard,
    HttpServerConfigCard,
    HttpSSEConfigCard,
    WebsocketClientConfigCard,
    WebsocketServersConfigCard,
    AdvancedBackendDialog,
    AutoRestartDialog,
)

if TYPE_CHECKING:
    from src.core.remote.servers import ServerProfile


class RuntimeTargetConfigCard(SettingCard):
    """运行位置选择卡片(P2.7).

    在 [`BotConfigWidget`](src/ui/page/bot_page/widget/config.py) 中
    展示一个下拉框, 包含 "本地" + 每台已添加的远端服务器, 让用户选择 Bot
    在哪台机器上运行. 持久化的值是 [`BotConfig.runtime_target`](src/core/config/config_model.py),
    形如 ``"local"`` 或 [`ServerProfile.id`](src/core/remote/servers.py).

    设计要点:
    - 下拉项构造时刻向 [`ServerManager`](src/core/remote/server_manager.py)
      取一次服务器列表; 调用方在显示前可调用 ``refresh_targets()`` 主动刷新.
    - 当设置了 ``_backend_filter`` 时, 仅展示 ``backend_flavor`` 匹配的远端服务器,
      避免用户选中不兼容的服务器后保存时才被拦截.
    - 服务器档案被外部删除后, ``fill_value(server_id)`` 会临时插入一条
      "(已删除) <id>" 的占位项让用户感知, 避免静默回退到本地造成行为漂移.
    """

    _LOCAL_LABEL = "本地"

    # 当用户选中远端服务器时, 发射该服务器的 backend_flavor (BackendType 枚举);
    # 选中"本地"时发射 None, 表示不锁定.
    target_flavor_changed = Signal(object)

    def __init__(
        self,
        icon=FluentIcon.GLOBE,
        title: str = "",
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.combo_box = ComboBox(self)
        self.combo_box.setFixedWidth(200)
        self.hBoxLayout.addWidget(self.combo_box, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

        # index -> server_id ("local" 或 UUID), 与 combo_box 表项一一对应
        self._target_ids: list[str] = []
        # index -> ServerProfile (None for local), 用于查询选中服务器的 flavor
        self._profiles: list["ServerProfile | None"] = []
        # 后端类型过滤: None 表示不过滤 (显示全部), 设置后仅显示匹配 flavor 的服务器
        self._backend_filter: BackendType | None = None
        # 是否抑制 target_flavor_changed 信号 (fill_value / refresh 期间避免 spurious emit)
        self._suppress_signal: bool = False
        self.refresh_targets()

        self.combo_box.currentIndexChanged.connect(self._on_target_index_changed)

    def _on_target_index_changed(self, idx: int) -> None:
        """用户切换运行位置下拉时, 发射目标服务器的 flavor."""
        if self._suppress_signal:
            return
        if idx < 0 or idx >= len(self._profiles):
            return
        profile = self._profiles[idx]
        if profile is None:
            # 选中"本地": 不锁定 backend_type
            self.target_flavor_changed.emit(None)
        else:
            # 选中远端: 发射该服务器的 flavor
            from src.core.remote.servers import BackendFlavor

            flavor_to_backend = {
                BackendFlavor.NAPCAT: BackendType.NAPCAT,
                BackendFlavor.SNOWLUMA: BackendType.SNOWLUMA,
            }
            backend = flavor_to_backend.get(profile.backend_flavor, BackendType.NAPCAT)
            self.target_flavor_changed.emit(backend)

    def apply_backend_filter(self, backend_type: BackendType) -> None:
        """按 Bot 的 backend_type 过滤运行位置下拉列表.

        仅展示 backend_flavor 与 backend_type 匹配的远端服务器 + "本地".
        调用后自动 refresh_targets() 重建下拉项.
        """
        if self._backend_filter == backend_type:
            return
        self._backend_filter = backend_type
        self.refresh_targets()

    def refresh_targets(self) -> None:
        """重新拉取服务器列表; 保留当前选择 (若仍存在)."""
        self._suppress_signal = True
        try:
            previous = self.get_value() if self._target_ids else RUNTIME_TARGET_LOCAL

            self.combo_box.clear()
            self._target_ids = [RUNTIME_TARGET_LOCAL]
            self._profiles = [None]
            self.combo_box.addItem(self.tr(self._LOCAL_LABEL))

            try:
                from src.core.remote.server_manager import ServerManager

                servers = it(ServerManager).list_servers()
            except Exception:
                servers = []

            for profile in servers:
                # 按 backend_filter 过滤: 仅展示 flavor 匹配的服务器
                if self._backend_filter is not None:
                    from src.core.remote.servers import BackendFlavor

                    flavor_map = {
                        BackendType.NAPCAT: BackendFlavor.NAPCAT,
                        BackendType.SNOWLUMA: BackendFlavor.SNOWLUMA,
                    }
                    expected_flavor = flavor_map.get(self._backend_filter)
                    if expected_flavor is not None and profile.backend_flavor != expected_flavor:
                        continue

                label = f"{profile.name} ({profile.credentials.host})"
                self._target_ids.append(profile.id)
                self._profiles.append(profile)
                self.combo_box.addItem(label)

            # 还原之前的选择
            self.fill_value(previous)
        finally:
            self._suppress_signal = False

    def fill_value(self, target: str | None) -> None:
        """选中匹配 target 的下拉项; 找不到则插入"(已删除) <id>"占位."""
        normalized = target or RUNTIME_TARGET_LOCAL
        if normalized in self._target_ids:
            self.combo_box.setCurrentIndex(self._target_ids.index(normalized))
            return
        # 服务器档案不在: 插入占位标识让用户感知
        placeholder = f"(已删除) {normalized}"
        self._target_ids.append(normalized)
        self.combo_box.addItem(placeholder)
        self.combo_box.setCurrentIndex(len(self._target_ids) - 1)

    def get_value(self) -> str:
        """返回当前选中的 target 字符串."""
        idx = self.combo_box.currentIndex()
        if idx < 0 or idx >= len(self._target_ids):
            return RUNTIME_TARGET_LOCAL
        return self._target_ids[idx]

    def clear(self) -> None:
        """重置为默认本地."""
        self.refresh_targets()
        self.fill_value(RUNTIME_TARGET_LOCAL)


class BotConfigWidget(ScrollArea):
    """Bot 设置页面"""

    # P2 (Tier A): backend_type 切换信号. 由 ``backend_type_card`` 内部
    # ``ComboBox.currentIndexChanged`` 触发后转发, 携带 :class:`BackendType` 枚举值.
    # 上层 :class:`ConfigPage` 把它接到 :meth:`ConnectConfigWidget.apply_backend_type`
    # 与 :meth:`AdvancedConfigWidget.apply_backend_type`, 让两个 widget 实时显隐.
    backend_type_changed = Signal(BackendType)

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 创建控件
        self.view = QWidget()

        self.bot_name_card = LineEditConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Bot 名称"),
            content=self.tr("设置机器人的名称"),
            placeholder_text=self.tr("QIAO Bot"),
            parent=self.view,
        )
        self.bot_qq_id_card = LineEditConfigCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("Bot QQ"),
            content=self.tr("设置机器人 QQ 号, 不能为空"),
            placeholder_text=self.tr("114514"),
            parent=self.view,
        )
        # P1 (SnowLuma 适配): 后端类型单选. 下拉项文本与 BackendType.display_name 严格对齐,
        # 这样 get_value() 返回的文本可直接反查为 BackendType。
        # SnowLuma 未安装时不在此限制选择 (实际启动时 _create_snowluma_process 会报错提示),
        # 避免表单依赖 PathFunc.get_snowluma_node_executable() 的极端状态判断。
        self.backend_type_card = ComboBoxConfigCard(
            icon=FluentIcon.APPLICATION,
            title=self.tr("后端类型"),
            content=self.tr("选择 Bot 的协议端实现; SnowLuma 需先在组件页 SnowLuma tab 安装"),
            texts=[BackendType.NAPCAT.display_name, BackendType.SNOWLUMA.display_name],
            parent=self.view,
        )
        # W6 (2026-05-11): per-Bot SnowLuma WebUI 密码卡被删除; daemon 架构下密码是
        # App 级全局设置不是 per-Bot 字段, 该入口迁到组件页 SnowLuma tab 的
        # "全局 WebUI 密码 override" 卡 (可读/写 ``cfg.snowluma_webui_password_override``).
        # 历史 ``snowluma_webui_password_override`` 字段由 W4 迁移逻辑从 bot.json pop 后
        # 写入 cfg, 用户无需人工介入.

        self.runtime_target_card = RuntimeTargetConfigCard(
            icon=FluentIcon.GLOBE,
            title=self.tr("运行位置"),
            content=self.tr("选择 Bot 运行在本机或某台已添加的远端服务器"),
            parent=self.view,
        )
        self.music_sign_url_card = LineEditConfigCard(
            icon=FluentIcon.MUSIC,
            title=self.tr("音乐签名URL"),
            content=self.tr("用于处理音乐相关请求, 为空则使用默认签名服务器"),
            placeholder_text=self.tr("https://example.com/music"),
            parent=self.view,
        )
        self.auto_restart_dialog_card = ShowDialogCard(
            dialog=AutoRestartDialog,
            icon=FluentIcon.IOT,
            title=self.tr("自动重启"),
            content=self.tr("设置自动重启 Bot 的相关选项"),
            parent=self.view,
        )
        # SL-Q (Tier A): 掉线重启仅在 NapCat 模式下有效.
        # ``offlineAutoRestart`` 的消费者只在 :class:`NapCatQQLoginState.slot_update_online_status`
        # (``bot_process_manager.py``), 依赖 OneBot ``get_status`` 轮询驱动的在线状态翻转;
        # SnowLuma 路径用 :class:`SnowLumaStatusPoller` 监控 WebUI ``/api/processes`` 的
        # 4 档登录态, 没有 ``slot_update_online_status`` 的调用链, 即没有"掉线->重启"的
        # 触发器. 因此在 SnowLuma 模式下整卡隐藏 (与高级配置页 ``offline_notice_card``
        # 同理), 避免用户开关后以为生效. 持久化值仍保留, 切回 NapCat 后用户原选择不丢.
        self.offline_auto_restart_card = SwitchConfigCard(
            icon=FluentIcon.HISTORY,
            title=self.tr("掉线重启"),
            content=self.tr("当 Bot 掉线时自动重启, 与掉线通知可以配合使用"),
            parent=self.view,
        )

        # 设置属性
        self._config = None
        # 卡片显示顺序: 基础身份 → 后端及其专属参数 → 运行位置/扩展 → 重启策略.
        # W6 (2026-05-11): 移除 snowluma_webui_password_card; 迁到组件页.
        self.cards = [
            self.bot_name_card,
            self.bot_qq_id_card,
            self.backend_type_card,
            self.runtime_target_card,
            self.music_sign_url_card,
            self.auto_restart_dialog_card,
            self.offline_auto_restart_card,
        ]

        # 设置控件
        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建布局
        self.card_layout = ExpandLayout(self.view)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(2)
        for card in self.cards:
            self.card_layout.addWidget(card)
        self.adjustSize()

        # P2 (Tier A): 把 ComboBox 切换转发为 BackendType 枚举.
        # 用 currentIndexChanged 而非 currentTextChanged 是为了避免 fill_value 反填时
        # 触发 spurious 重发 (虽然 fill_value 也会改 index, 但 ConfigPage 在 fill_config
        # 之后会显式调一次 apply_backend_type, 中间任何 noop 触发都不影响最终态).
        self.backend_type_card.comboBox.currentIndexChanged.connect(self._on_backend_index_changed)

        # 运行位置切换 → 锁定/解锁 backend_type:
        # 选中远端服务器时, backend_type 锁定为该服务器的 flavor (不可编辑);
        # 切回"本地"时解锁, 用户可自由切换.
        self.runtime_target_card.target_flavor_changed.connect(self._on_target_flavor_changed)

        # Q1 / SL-Q: 初始按当前 backend 同步隐显 backend 相关卡 (默认 NapCat).
        self._apply_backend_card_visibility(BackendType.NAPCAT)

    # ==================== 公共方法 ====================
    def _on_backend_index_changed(self, _idx: int) -> None:
        """ComboBox 切换 → emit ``backend_type_changed`` (BackendType 枚举).

        ``ComboBox.currentText()`` 返回的是 display_name (``"NapCat"`` / ``"SnowLuma"``);
        :meth:`BackendType.from_str` 已对未知文本退化为 ``NAPCAT``.
        """
        backend_label = (self.backend_type_card.get_value() or "").strip().lower()
        backend = BackendType.from_str(backend_label)
        # Q1 / SL-Q: 本地同步隐显 backend 相关卡; 不依赖外部 apply_backend_type 调用,
        # 避免切换 ComboBox 瞬间出现卡片闪烁.
        self._apply_backend_card_visibility(backend)
        self.backend_type_changed.emit(backend)

    def _on_target_flavor_changed(self, flavor: BackendType | None) -> None:
        """运行位置下拉切换时, 按目标服务器 flavor 锁定/解锁 backend_type.

        - ``flavor is None`` (选中"本地"): 解锁 backend_type ComboBox, 用户可自由切换.
        - ``flavor is BackendType.XXX`` (选中远端): 强制 backend_type 为该 flavor 并锁定
          ComboBox (setEnabled(False)), 因为远端服务器只支持一种后端.

        同时更新 runtime_target_card 的过滤, 确保下拉列表只展示兼容的服务器.
        """
        if flavor is None:
            # 本地: 解锁
            self.backend_type_card.comboBox.setEnabled(True)
        else:
            # 远端: 强制切换 backend_type 并锁定
            self.backend_type_card.fill_value(flavor.display_name)
            self.backend_type_card.comboBox.setEnabled(False)

    def _apply_backend_card_visibility(self, backend: BackendType) -> None:
        """按 backend 同步隐显基础配置页中与 backend 相关的卡片 (幂等).

        W6 (2026-05-11): per-Bot SnowLuma 密码卡被移除; 仅剩 ``offline_auto_restart_card``
        要按 backend 隐显.

        - ``offline_auto_restart_card``: 仅 NapCat 可见 (SL-Q).
          SnowLuma 没有 ``offlineAutoRestart`` 的消费链路
          (见 ``NapCatQQLoginState.slot_update_online_status`` 仅 NapCat 调用),
          整卡隐藏避免误导.

        持久化值零丢失: 隐藏期间 ``get_config`` 仍会序列化对应字段; 切换 backend 后
        用户此前的选择保留.

        同时更新 ``runtime_target_card`` 的后端过滤, 仅展示兼容的远端服务器.
        """
        is_snowluma = backend == BackendType.SNOWLUMA
        self.offline_auto_restart_card.setVisible(not is_snowluma)
        # 按 backend_type 过滤运行位置下拉中的远端服务器
        self.runtime_target_card.apply_backend_filter(backend)

    def get_config(self) -> BotConfig:
        """获取配置"""
        # P1 (SnowLuma 适配): backend_type_card 返回的是 display_name ("NapCat" / "SnowLuma"),
        # 转为 BackendType 枚举; 未知文本走 from_str 退化逻辑 (NAPCAT 默认).
        backend_label = (self.backend_type_card.get_value() or "").strip().lower()
        backend_type = BackendType.from_str(backend_label)
        # W6 (2026-05-11): ``snowluma_webui_password_override`` 已删除; per-Bot 字段不再存在.
        return BotConfig(
            **{
                "name": self.bot_name_card.get_value(),
                "QQID": self.bot_qq_id_card.get_value(),
                "musicSignUrl": self.music_sign_url_card.get_value(),
                "autoRestartSchedule": self.auto_restart_dialog_card.get_value(),
                "offlineAutoRestart": self.offline_auto_restart_card.get_value(),
                "runtime_target": self.runtime_target_card.get_value(),
                "backend_type": backend_type,
            }
        )

    def fill_config(self, config: BotConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.bot_name_card.fill_value(self._config.name)
        self.bot_qq_id_card.fill_value(self._config.QQID)
        self.bot_qq_id_card.setEnabled(False)
        self.music_sign_url_card.fill_value(self._config.musicSignUrl)
        self.auto_restart_dialog_card.fill_value(self._config.autoRestartSchedule)
        self.offline_auto_restart_card.fill_value(self._config.offlineAutoRestart)
        # P1 (SnowLuma 适配): 反填 backend_type_card; ComboBoxConfigCard.fill_value 接受文本.
        self.backend_type_card.fill_value(self._config.backend_type.display_name)
        # 先设置过滤再刷新, 确保下拉列表只展示兼容的服务器
        self.runtime_target_card.apply_backend_filter(self._config.backend_type)
        # 服务器列表可能在编辑期间变化, 每次填充前都刷新
        self.runtime_target_card.refresh_targets()
        self.runtime_target_card.fill_value(self._config.runtime_target)
        # W6 (2026-05-11): ``snowluma_webui_password_override`` 迁移到组件页, 本处不再反填.
        self._apply_backend_card_visibility(self._config.backend_type)

        # 如果 Bot 当前已在远端运行, 锁定 backend_type (远端服务器只支持一种后端);
        # 本地则解锁.
        if self._config.runtime_target and self._config.runtime_target != RUNTIME_TARGET_LOCAL:
            self.backend_type_card.comboBox.setEnabled(False)
        else:
            self.backend_type_card.comboBox.setEnabled(True)

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()
        self.bot_qq_id_card.setEnabled(True)
        # 新建 Bot: 解锁 backend_type, 用户可自由选择
        self.backend_type_card.comboBox.setEnabled(True)

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.card_layout.heightForWidth(self.width()) + 46)


class ConnectConfigWidget(ScrollArea):
    """Bot 连接设置页面"""

    CONFIG_KEY_AND_CARD_DICT = {
        "httpServers": HttpServerConfigCard,
        "httpSseServers": HttpSSEConfigCard,
        "httpClients": HttpClientConfigCard,
        "websocketServers": WebsocketServersConfigCard,
        "websocketClients": WebsocketClientConfigCard,
    }

    CINFIG_AND_CARD_DICT = {
        HttpServersConfig: HttpServerConfigCard,
        HttpSseServersConfig: HttpSSEConfigCard,
        HttpClientsConfig: HttpClientConfigCard,
        WebsocketServersConfig: WebsocketServersConfigCard,
        WebsocketClientsConfig: WebsocketClientConfigCard,
    }

    CONFIG_KEY_NAME = [
        "httpServers",
        "httpSseServers",
        "httpClients",
        "websocketServers",
        "websocketClients",
    ]

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        # 设置属性
        self.cards = []

        # 创建控件
        self.view = QWidget()

        # 设置属性
        self._config = None
        # P2 (Tier A): 当前 backend_type, 默认 NapCat (与 P1 行为一致); 由 ConfigPage
        # 在 fill_config 与 BotConfigWidget.backend_type_changed 信号触发时同步.
        self._current_backend: BackendType = BackendType.NAPCAT

        # 设置控件
        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)

        # 创建布局
        self.card_layout = FlowLayout(self.view)
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(8)

    # ==================== 公共方法 ====================
    def apply_backend_type(self, backend: BackendType) -> None:
        """按 backend 显隐 ConnectConfigWidget 内的卡片 (Tier A, 幂等).

        SnowLuma 模式下:
        - HTTP SSE 卡片整卡 ``setVisible(False)`` (SnowLuma 不识别 SSE);
        - 其他卡片仍可见, 但点开 Dialog 时由 :meth:`ConfigDialogBase.apply_backend_type`
          决定字段可见性.

        切回 NapCat 后 SSE 卡片自动重现 (持久化保留, 仅 ``setVisible``).
        """
        self._current_backend = backend
        is_snowluma = backend == BackendType.SNOWLUMA
        for card in self.cards:
            # SSE 整类在 SnowLuma 模式隐藏 (持久化保留)
            if isinstance(card, HttpSSEConfigCard):
                card.setVisible(not is_snowluma)
            else:
                card.setVisible(True)
        # 触发 FlowLayout 重排
        self.card_layout.update()
        self.updateGeometry()

    def add_card(self, config: NetworkBaseConfig) -> None:
        """添加卡片到列表"""
        if card_class := self.CINFIG_AND_CARD_DICT.get(type(config)):
            card = card_class(config, self.view)
            card.remove_signal.connect(self.remove_card)
            self.cards.append(card)
            self.card_layout.addWidget(card)
            # P2 (Tier A): 新加的 SSE 卡片在 SnowLuma 模式下立即隐藏 (避免 add_card
            # 时机晚于 apply_backend_type 导致一闪而过)
            if isinstance(card, HttpSSEConfigCard) and self._current_backend == BackendType.SNOWLUMA:
                card.setVisible(False)
            self.card_layout.update()
            self.updateGeometry()

    def has_config_name(self, name: str) -> bool:
        """检查是否已存在同名网络配置. """
        normalized_name = name.strip().casefold()
        if not normalized_name:
            return False

        for card in self.cards:
            if card.config.name.strip().casefold() == normalized_name:
                return True
        return False

    def remove_card(self, config: NetworkBaseConfig) -> None:
        """从列表删除卡片"""
        for card in self.cards:
            if card.config != config:
                continue
            self.card_layout.removeWidget(card)
            card.setParent(None)
            card.deleteLater()
            self.cards.remove(card)

    def get_config(self) -> ConnectConfig:
        """获取配置"""
        config_data = {
            key: [card.get_config() for card in self.cards if isinstance(card, card_type)]
            for key, card_type in self.CONFIG_KEY_AND_CARD_DICT.items()
        }
        config_data["plugins"] = []

        return ConnectConfig(**config_data)

    def fill_config(self, config: ConnectConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self.clear_config()

        for attr in self.CONFIG_KEY_NAME:
            for _config in getattr(config, attr, []):
                self.add_card(_config)

    def clear_config(self) -> None:
        """清空配置"""
        self.cards.clear()
        self.card_layout.takeAllWidgets()


class AdvancedConfigWidget(ScrollArea):
    """Bot 高级设置页面"""

    class BackendConfigCard(SettingCard):
        """底层配置入口卡片. """

        def __init__(self, parent: QWidget | None = None) -> None:
            super().__init__(
                FluentIcon.DEVELOPER_TOOLS,
                self.tr("底层与反检测"),
                self.tr("Packet、O3 Hook 与 bypass 等低频配置，默认情况下无需调整"),
                parent,
            )
            self._dialog: AdvancedBackendDialog | None = None
            self._config = AdvancedConfig()
            self.button = PushButton(self.tr("展开配置"), self)
            self.button.clicked.connect(self.slot_show_dialog)
            self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
            self.hBoxLayout.addSpacing(16)
            self._refresh_summary()

        def _ensure_dialog(self) -> AdvancedBackendDialog:
            """惰性创建对话框, 避免把对话框控件挂进当前页面. """
            if self._dialog is None:
                from src.ui.window.main_window import MainWindow

                self._dialog = AdvancedBackendDialog(it(MainWindow))
            return self._dialog

        def _refresh_summary(self) -> None:
            config = self._config
            enabled_bypass_count = sum(
                [
                    config.bypass.hook,
                    config.bypass.window,
                    config.bypass.module,
                    config.bypass.process,
                    config.bypass.container,
                    config.bypass.js,
                ]
            )
            packet_server_text = self.tr("已配置") if config.packetServer else self.tr("默认")
            self.setContent(
                self.tr(
                    "Packet={0} · Server={1} · O3 Hook={2} · 反检测启用 {3}/6"
                ).format(config.packetBackend, packet_server_text, config.o3HookMode, enabled_bypass_count)
            )

        def slot_show_dialog(self) -> None:
            dialog = self._ensure_dialog()
            dialog.fill_config(self._config)
            if dialog.exec():
                self._config = dialog.get_config()
                self._refresh_summary()
                return

        def get_value(self) -> AdvancedConfig:
            return self._config

        def fill_value(self, config: AdvancedConfig | None = None) -> None:
            if config is None:
                return

            self._config = config.model_copy(deep=True)
            if self._dialog is not None:
                self._dialog.fill_config(self._config)
            self._refresh_summary()

        def clear(self) -> None:
            self._config = AdvancedConfig()
            if self._dialog is not None:
                self._dialog.clear_config()
            self._refresh_summary()

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)
        self.runtime_group = SettingCardGroup(self.tr("运行与消息"), self.view)
        self.log_group = SettingCardGroup(self.tr("日志与诊断"), self.view)
        self.engine_group = SettingCardGroup(self.tr("底层配置"), self.view)

        self.auto_start_card = SwitchConfigCard(
            icon=FluentIcon.PLAY,
            title=self.tr("自动启动"),
            content=self.tr("是否在启动时自动启动 bot"),
            parent=self.runtime_group,
        )
        self.offline_notice_card = SwitchConfigCard(
            icon=FluentIcon.MEGAPHONE,
            title=self.tr("掉线通知"),
            content=self.tr("当Bot状态为 离线 时, 发送通知"),
            parent=self.runtime_group,
        )
        self.parse_mult_message_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("解析合并转发消息"),
            content=self.tr("是否解析合并转发消息"),
            parent=self.runtime_group,
        )
        self.local_file_to_url_card = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr("是否将本地文件转换为URL, 如果获取不到url则使用base64字段返回文件内容"),
            value=True,
            parent=self.runtime_group,
        )
        self.file_log_card = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("文件日志"),
            content=self.tr("是否要将日志记录到文件"),
            parent=self.log_group,
        )
        self.console_log_card = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("控制台日志"),
            content=self.tr("是否启用控制台日志"),
            value=True,
            parent=self.log_group,
        )
        self.file_log_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("文件日志等级"),
            content=self.tr("设置文件日志输出等级"),
            texts=["debug", "info", "error"],
            parent=self.log_group,
        )
        self.console_level_card = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("控制台日志等级"),
            content=self.tr("设置控制台日志输出等级"),
            texts=["info", "debug", "error"],
            parent=self.log_group,
        )
        self.backend_config_card = self.BackendConfigCard(self.engine_group)

        self._config = None
        self.runtime_cards = [
            self.auto_start_card,
            self.offline_notice_card,
            self.parse_mult_message_card,
            self.local_file_to_url_card,
        ]
        self.log_cards = [
            self.file_log_card,
            self.console_log_card,
            self.file_log_level_card,
            self.console_level_card,
        ]
        self.cards = self.runtime_cards + self.log_cards + [self.backend_config_card]

        self.setWidget(self.view)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setWidgetResizable(True)
        self.view.setObjectName("BotAdvancedConfigView")

        for card in self.runtime_cards:
            self.runtime_group.addSettingCard(card)
        for card in self.log_cards:
            self.log_group.addSettingCard(card)
        self.engine_group.addSettingCard(self.backend_config_card)

        self.expand_layout.addWidget(self.runtime_group)
        self.expand_layout.addWidget(self.log_group)
        self.expand_layout.addWidget(self.engine_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

        self.file_log_card.switchButton.checkedChanged.connect(self._sync_log_level_card_state)
        self.console_log_card.switchButton.checkedChanged.connect(self._sync_log_level_card_state)
        self._sync_log_level_card_state()
        self.adjustSize()

        # P2 (Tier A): 当前 backend_type, 默认 NapCat (与 P1 行为一致)
        self._current_backend: BackendType = BackendType.NAPCAT

    # ==================== 公共方法 ====================
    def apply_backend_type(self, backend: BackendType) -> None:
        """按 backend 显隐 AdvancedConfigWidget 内的卡片 (Tier A, 幂等).

        SnowLuma 模式下仅保留 ``auto_start_card`` (Desktop 通用), 其余 8 张卡全部
        ``setVisible(False)``:

        - ``offline_notice_card``: SnowLuma 路径没有 ``NapCatQQLoginState`` 轮询 OneBot
          ``get_status``, 离线检测整链路缺失 → 掉线通知开关无意义, 隐藏避免误导用户.
        - ``parseMultMsg`` / ``enableLocalFile2Url`` / ``fileLog`` / ``consoleLog`` /
          ``fileLogLevel`` / ``consoleLogLevel`` / ``backend_config_card``:
          NapCat 注入式专有, SnowLuma 完全不读.

        持久化值零丢失: 隐藏的卡片仍在 ``self.cards`` 里, ``get_config()`` 仍会序列化;
        切回 NapCat 后自动重现.
        """
        self._current_backend = backend
        is_napcat = backend == BackendType.NAPCAT
        # NapCat-only: 都不在 SnowLuma 模式可见
        # offline_notice: SnowLuma 路径无离线检测, 整卡隐藏避免误导用户
        self.offline_notice_card.setVisible(is_napcat)
        self.parse_mult_message_card.setVisible(is_napcat)
        self.local_file_to_url_card.setVisible(is_napcat)
        self.file_log_card.setVisible(is_napcat)
        self.file_log_level_card.setVisible(is_napcat)
        self.console_log_card.setVisible(is_napcat)
        self.console_level_card.setVisible(is_napcat)
        self.backend_config_card.setVisible(is_napcat)
        # SnowLuma 模式下隐藏整个 log_group 与 engine_group 头部 (避免空 group 标题悬浮)
        # 通过隐藏 group 实现 — 仅当 group 中所有 card 都不可见时才隐藏 group.
        # 这里 SnowLuma 模式下: log_group 全空, engine_group 全空, 一并隐藏.
        self.log_group.setVisible(is_napcat)
        self.engine_group.setVisible(is_napcat)
        # auto_start 双 backend 都可见 (runtime_group 仅它一张卡 + offline_notice;
        # SnowLuma 模式下 offline_notice 隐藏, runtime_group 仍保留 auto_start)

        # 同步 log level 卡的 enable 状态 (避免在 SnowLuma 模式下 log 卡隐藏后仍保持
        # enable=False 之类的 stale 状态; setVisible 不改 isEnabled)
        if is_napcat:
            self._sync_log_level_card_state()

    def get_config(self) -> AdvancedConfig:
        """获取配置"""
        backend_config = self.backend_config_card.get_value()
        return AdvancedConfig(
            **{
                "autoStart": self.auto_start_card.get_value(),
                "offlineNotice": self.offline_notice_card.get_value(),
                "parseMultMsg": self.parse_mult_message_card.get_value(),
                "packetServer": backend_config.packetServer,
                "packetBackend": backend_config.packetBackend,
                "enableLocalFile2Url": self.local_file_to_url_card.get_value(),
                "fileLog": self.file_log_card.get_value(),
                "consoleLog": self.console_log_card.get_value(),
                "fileLogLevel": self.file_log_level_card.get_value(),
                "consoleLogLevel": self.console_level_card.get_value(),
                "o3HookMode": backend_config.o3HookMode,
                "bypass": backend_config.bypass,
            }
        )

    def fill_config(self, config: AdvancedConfig | None = None) -> None:
        """填充配置"""
        if config is None:
            return

        self._config = config
        self.auto_start_card.fill_value(self._config.autoStart)
        self.offline_notice_card.fill_value(self._config.offlineNotice)
        self.parse_mult_message_card.fill_value(self._config.parseMultMsg)
        self.local_file_to_url_card.fill_value(self._config.enableLocalFile2Url)
        self.file_log_card.fill_value(self._config.fileLog)
        self.console_log_card.fill_value(self._config.consoleLog)
        self.file_log_level_card.fill_value(self._config.fileLogLevel)
        self.console_level_card.fill_value(self._config.consoleLogLevel)
        self.backend_config_card.fill_value(self._config)
        self._sync_log_level_card_state()

    def clear_config(self) -> None:
        """清空配置"""
        for card in self.cards:
            card.clear()
        self.offline_notice_card.fill_value(
            cfg.get(cfg.bot_offline_email_notice) or cfg.get(cfg.bot_offline_web_hook_notice)
        )
        self._sync_log_level_card_state()

    def _sync_log_level_card_state(self, *_args) -> None:
        """根据日志开关同步日志等级输入的可编辑状态. """
        self.file_log_level_card.setEnabled(self.file_log_card.get_value())
        self.console_level_card.setEnabled(self.console_log_card.get_value())

    # ==================== 重写方法 ====================
    def adjustSize(self) -> None:
        """重写方法以调整控件大小适应内容高度"""
        self.resize(self.width(), self.expand_layout.heightForWidth(self.width()) + 46)
