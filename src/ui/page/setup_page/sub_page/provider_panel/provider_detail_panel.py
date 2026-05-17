# -*- coding: utf-8 -*-
"""供应商详情面板.

右侧详情面板, 展示选中供应商的完整配置信息, 包括:
- Header 区域: 供应商名称 + 协议标签 + 外部链接按钮 + 启用开关 + 删除按钮 + 横向分割线
- API 配置区: 由 ``ProtocolFieldStack`` 直接渲染 (扁平布局, 无外层卡片包裹)
  - 标签行: "API 密钥" 加粗 + 多密钥工具按钮
  - 输入行: PasswordLineEdit + 检测按钮
  - 提示行: 极小字号 caption "多个密钥使用逗号分隔" + 可选 "获取 API Key" 超链接
  - 标签行: "API 地址" 加粗 + 自定义请求头工具按钮
  - 输入行: LineEdit
  - 预览行: 极小字号 caption "预览: ..."
- 模型区: 直接放 ``ModelListWidget``, 不再用 HeaderCardWidget 包裹

变更字段会通过 ProviderRegistry.update_provider 持久化, 自定义请求头与
多密钥配置同步保存到 Provider.api_key_ref / Provider.custom_headers.
"""
from __future__ import annotations

from enum import Enum, auto

from creart import it
from PySide6.QtCore import Qt, QTimer, QUrl, Signal
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import QFrame, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    FluentIcon,
    MessageBox,
    ScrollArea,
    StrongBodyLabel,
    SubtitleLabel,
    SwitchButton,
    TransparentToolButton,
    isDarkTheme,
)

from src.core.logging import LogSource, logger

from src.core.agent.api_check_service import ApiCheckService
from src.core.agent.api_key_pool import parse_api_keys
from src.core.agent.provider import ProviderRegistry
from src.core.config import cfg
from src.ui.components.info_bar import error_bar, success_bar

from .custom_headers_dialog import CustomHeadersDialog
from .model_list_widget import ModelListWidget
from .multi_key_dialog import MultiKeyDialog
from .protocol_field_stack import ProtocolFieldStack
from .provider_protocol_utils import build_url_preview


class CheckButtonState(Enum):
    """检测按钮状态机枚举."""

    IDLE = auto()
    LOADING = auto()
    SUCCESS = auto()
    ERROR = auto()


class ProviderDetailPanel(ScrollArea):
    """右侧供应商详情面板 -- API 配置 + 模型列表.

    Signals:
        provider_changed: 供应商状态变更时发射, 携带 provider_id.
    """

    provider_changed = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._current_provider_id: str | None = None
        self._current_website_url: str | None = None
        self._current_api_key_url: str | None = None
        self._current_protocol_type: str = "openai"
        self._api_check_service: ApiCheckService | None = None

        self._check_button_state: CheckButtonState = CheckButtonState.IDLE
        self._last_error_message: str = ""

        self._setup_ui()
        self._connect_signals()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建面板整体 UI 布局."""
        # 内容容器 - 外层布局不留左右边距, 让 Header Divider 可以贯通到尽头.
        # 各区块用内层布局加 15px 左右边距, 形成视觉缩进.
        self._content_widget = QWidget(self)
        self._content_layout = QVBoxLayout(self._content_widget)
        self._content_layout.setContentsMargins(0, 10, 0, 10)
        self._content_layout.setSpacing(8)

        self._setup_header()
        self._setup_api_section()
        self._setup_model_section()
        self._content_layout.addStretch()

        self.setWidget(self._content_widget)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        self._apply_scroll_area_theme_style()
        cfg.themeChanged.connect(self._apply_scroll_area_theme_style)

    def _apply_scroll_area_theme_style(self, *_args) -> None:
        """让滚动容器与主窗口背景保持一致, 不再使用区分色."""
        viewport = self.viewport()
        if viewport is not None:
            viewport.setStyleSheet("background: transparent;")
            viewport.setAutoFillBackground(False)
        if hasattr(self, "_content_widget") and self._content_widget is not None:
            self._content_widget.setAutoFillBackground(False)

        self.setStyleSheet(
            "ProviderDetailPanel {"
            "  background: transparent;"
            "  border: none;"
            "}"
        )

    def _setup_header(self) -> None:
        """构建 Header 区域: 名称 + 协议标签 + 外链按钮 + 开关 + 删除按钮 + 全宽分割线."""
        header_container = QWidget(self._content_widget)
        header_layout = QHBoxLayout(header_container)
        header_layout.setContentsMargins(15, 0, 15, 0)
        header_layout.setSpacing(8)

        self._name_label = SubtitleLabel("", self._content_widget)
        header_layout.addWidget(self._name_label)

        self._external_link_button = TransparentToolButton(FluentIcon.LINK, self._content_widget)
        self._external_link_button.setFixedSize(32, 32)
        self._external_link_button.setVisible(False)
        header_layout.addWidget(self._external_link_button)

        self._api_options_button = TransparentToolButton(FluentIcon.SETTING, self._content_widget)
        self._api_options_button.setFixedSize(32, 32)
        self._api_options_button.setToolTip(self.tr("API 选项"))
        header_layout.addWidget(self._api_options_button)

        header_layout.addStretch()

        self._enable_switch = SwitchButton(self._content_widget)
        header_layout.addWidget(self._enable_switch)

        self._delete_button = TransparentToolButton(FluentIcon.DELETE, self._content_widget)
        self._delete_button.setFixedSize(32, 32)
        header_layout.addWidget(self._delete_button)

        self._content_layout.addWidget(header_container)

        self._header_divider = QFrame(self._content_widget)
        self._header_divider.setFrameShape(QFrame.Shape.NoFrame)
        self._header_divider.setFixedHeight(1)
        self._content_layout.addWidget(self._header_divider)
        self._apply_header_divider_style()
        cfg.themeChanged.connect(self._apply_header_divider_style)

    def _apply_header_divider_style(self, *_args) -> None:
        """根据主题应用分割线颜色."""
        if isDarkTheme():
            color = "rgba(255, 255, 255, 0.1)"
        else:
            color = "rgba(0, 0, 0, 0.1)"
        self._header_divider.setStyleSheet(
            f"QFrame {{ background-color: {color}; border: none; }}"
        )

    # ------------------------------------------------------------------
    # API 配置 (无卡片包裹)
    # ------------------------------------------------------------------

    def _setup_api_section(self) -> None:
        """构建 API 配置区域: 直接展开 ProtocolFieldStack, 不再用 HeaderCardWidget 包裹."""
        api_wrapper = QWidget(self._content_widget)
        api_wrapper_layout = QVBoxLayout(api_wrapper)
        api_wrapper_layout.setContentsMargins(15, 0, 15, 0)
        api_wrapper_layout.setSpacing(0)

        # ProtocolFieldStack 自身已经包含完整表单 (标签 + 输入 + caption / 预览)
        self._protocol_field_stack = ProtocolFieldStack(api_wrapper)
        api_wrapper_layout.addWidget(self._protocol_field_stack)

        self._content_layout.addWidget(api_wrapper)

    def _setup_model_section(self) -> None:
        """构建模型区域: 直接展示 ModelListWidget (内部自带标题栏)."""
        model_wrapper = QWidget(self._content_widget)
        model_layout = QVBoxLayout(model_wrapper)
        model_layout.setContentsMargins(15, 0, 15, 0)
        model_layout.setSpacing(0)

        self._model_list_widget = ModelListWidget(model_wrapper)
        model_layout.addWidget(self._model_list_widget)

        self._content_layout.addWidget(model_wrapper)

    # ------------------------------------------------------------------
    # 信号绑定
    # ------------------------------------------------------------------

    def _connect_signals(self) -> None:
        """连接内部信号与槽."""
        self._enable_switch.checkedChanged.connect(self._on_enable_toggled)
        self._delete_button.clicked.connect(self._on_delete_clicked)
        self._external_link_button.clicked.connect(self._on_external_link_clicked)
        self._api_options_button.clicked.connect(self._on_api_options_clicked)
        self._model_list_widget.model_added.connect(self._on_model_changed)
        self._model_list_widget.model_removed.connect(self._on_model_changed)
        self._model_list_widget.fetch_requested.connect(self._on_fetch_requested)

        self._protocol_field_stack.connect_field_text_changed(self._on_field_text_changed)
        self._protocol_field_stack.check_clicked.connect(self._on_check_api_clicked)
        self._protocol_field_stack.multi_key_clicked.connect(self._on_multi_key_clicked)
        self._protocol_field_stack.custom_headers_clicked.connect(
            self._on_custom_headers_clicked
        )

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def load_provider(self, provider_id: str) -> None:
        """加载指定供应商的配置到面板."""
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(provider_id)
        except KeyError:
            return

        self._current_provider_id = provider_id
        self._current_protocol_type = provider.protocol_type

        # Header
        self._name_label.setText(provider.name)
        self._enable_switch.setChecked(provider.enabled)

        has_website = bool(provider.website_url)
        self._external_link_button.setVisible(has_website)
        self._current_website_url = provider.website_url if has_website else None

        # "获取 API Key" 超链接由 ProtocolFieldStack 内部的 caption 行展示
        self._current_api_key_url = provider.api_key_url or None
        self._protocol_field_stack.set_api_key_url(self._current_api_key_url)

        # API 字段
        self._protocol_field_stack.set_protocol(provider.protocol_type)
        field_values: dict[str, str] = {"api_key": provider.api_key_ref}
        if provider.api_base_url:
            field_values["api_base_url"] = str(provider.api_base_url)
        if provider.azure_config:
            field_values["resource_endpoint"] = provider.azure_config.resource_endpoint
            field_values["deployment_name"] = provider.azure_config.deployment_name
            field_values["api_version"] = provider.azure_config.api_version
        self._protocol_field_stack.set_field_values(field_values)

        # 重置检测按钮状态
        self._set_check_button_state(CheckButtonState.IDLE)

        # 更新 URL 预览 + 检测按钮启用状态
        self._update_url_preview()
        self._update_check_button_state()

        # 模型列表
        self._model_list_widget.set_provider(provider)

    # ------------------------------------------------------------------
    # 槽函数 - Header
    # ------------------------------------------------------------------

    def _on_enable_toggled(self, checked: bool) -> None:
        """启用/禁用开关切换时的处理."""
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        registry.set_enabled(self._current_provider_id, checked)
        self.provider_changed.emit(self._current_provider_id)

    def _on_external_link_clicked(self) -> None:
        """点击外部链接按钮时的处理."""
        if self._current_website_url:
            QDesktopServices.openUrl(QUrl(self._current_website_url))

    def _on_api_options_clicked(self) -> None:
        """点击 API 选项按钮时的处理 - 弹出 ApiOptionsDialog."""
        if self._current_provider_id is None:
            return
        from .api_options_dialog import ApiOptionsDialog

        dialog = ApiOptionsDialog(self.window(), self._current_provider_id)
        dialog.exec()

    # ------------------------------------------------------------------
    # 槽函数 - API 配置字段
    # ------------------------------------------------------------------

    def _on_fetch_requested(self) -> None:
        """ModelListWidget 发起获取模型列表前的回调: 先持久化字段并刷新 provider."""
        if self._current_provider_id is None:
            return

        self._save_provider_changes()

        # 刷新 ModelListWidget 持有的 provider 对象为最新版本
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return
        self._model_list_widget.update_provider_ref(provider)

    def _on_field_text_changed(self, *_args) -> None:
        """ProtocolFieldStack 内部字段文本变更时的处理."""
        if self._check_button_state == CheckButtonState.ERROR:
            self._set_check_button_state(CheckButtonState.IDLE)

        self._update_url_preview()
        self._update_check_button_state()

    def _on_check_api_clicked(self) -> None:
        """点击 API 检测按钮: 先持久化字段, 再发起检测."""
        if self._current_provider_id is None:
            return

        self._save_provider_changes()

        field_values = self._protocol_field_stack.get_field_values()
        api_key = field_values.get("api_key", "").strip()
        if self._current_protocol_type == "azure":
            api_base_url = field_values.get("resource_endpoint", "").strip()
        else:
            api_base_url = field_values.get("api_base_url", "").strip()
        if not api_key or not api_base_url:
            return

        self._set_check_button_state(CheckButtonState.LOADING)

        self._api_check_service = ApiCheckService(self)
        self._api_check_service.check_finished.connect(self._on_check_finished)
        self._api_check_service.start_check(api_base_url, api_key)

    def _on_check_finished(self, success: bool, message: str) -> None:
        """API 检测完成时的处理."""
        if success:
            self._set_check_button_state(CheckButtonState.SUCCESS)
            success_bar(
                content=message,
                title=self.tr("成功"),
                duration=3000,
                parent=self,
            )
            QTimer.singleShot(3000, self._restore_check_button_idle)
        else:
            self._last_error_message = message
            self._set_check_button_state(CheckButtonState.ERROR)
            error_bar(
                content=message,
                title=self.tr("失败"),
                duration=5000,
                parent=self,
            )

    # ------------------------------------------------------------------
    # 槽函数 - 多密钥 / 自定义请求头 对话框
    # ------------------------------------------------------------------

    def _on_multi_key_clicked(self) -> None:
        """点击 "多密钥" 工具按钮 - 弹出 MultiKeyDialog 编辑密钥列表."""
        field_values = self._protocol_field_stack.get_field_values()
        current_key = field_values.get("api_key", "")
        initial = [k for k in parse_api_keys(current_key) if k]

        dialog = MultiKeyDialog(self.window(), initial)
        if dialog.exec():
            joined = dialog.get_keys_string()
            self._protocol_field_stack.set_field_values({"api_key": joined})
            self._save_provider_changes()

    def _on_custom_headers_clicked(self) -> None:
        """点击 "自定义请求头" 工具按钮 - 弹出 CustomHeadersDialog."""
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return

        dialog = CustomHeadersDialog(self.window(), dict(provider.custom_headers or {}))
        if dialog.exec():
            new_headers = dialog.get_headers()
            registry.update_provider(
                self._current_provider_id, custom_headers=new_headers
            )
            self._save_provider_changes()
            self.provider_changed.emit(self._current_provider_id)

    # ------------------------------------------------------------------
    # 工具函数
    # ------------------------------------------------------------------

    def _update_url_preview(self) -> None:
        """更新 URL 预览 caption (写到 ProtocolFieldStack 内部)."""
        field_values = self._protocol_field_stack.get_field_values()
        if self._current_protocol_type == "azure":
            url = field_values.get("resource_endpoint", "").strip()
        else:
            url = field_values.get("api_base_url", "").strip()
        azure_api_version = field_values.get("api_version", "")
        preview = build_url_preview(url, self._current_protocol_type, azure_api_version)
        self._protocol_field_stack.set_url_preview(preview)

    def _update_check_button_state(self) -> None:
        """根据当前字段值更新检测按钮启用状态."""
        field_values = self._protocol_field_stack.get_field_values()
        api_key = field_values.get("api_key", "").strip()
        if self._current_protocol_type == "azure":
            url_field = field_values.get("resource_endpoint", "").strip()
        else:
            url_field = field_values.get("api_base_url", "").strip()

        check_btn = self._protocol_field_stack.get_current_check_button()
        if check_btn is not None and self._check_button_state == CheckButtonState.IDLE:
            check_btn.setEnabled(bool(api_key and url_field))

    def _save_provider_changes(self) -> None:
        """将当前表单字段值写回 ProviderRegistry."""
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return

        field_values = self._protocol_field_stack.get_field_values()
        updates: dict = {}

        api_key = field_values.get("api_key")
        if api_key is not None and api_key != provider.api_key_ref:
            updates["api_key_ref"] = api_key

        if self._current_protocol_type == "azure":
            from src.core.agent.provider import AzureConfig

            new_azure = AzureConfig(
                resource_endpoint=field_values.get("resource_endpoint", "").strip()
                or (provider.azure_config.resource_endpoint if provider.azure_config else ""),
                deployment_name=field_values.get("deployment_name", "").strip()
                or (provider.azure_config.deployment_name if provider.azure_config else ""),
                api_version=field_values.get("api_version", "").strip() or "2024-02-01",
            )
            if (provider.azure_config is None) or (
                new_azure.model_dump() != provider.azure_config.model_dump()
            ):
                updates["azure_config"] = new_azure
        else:
            api_base_url = field_values.get("api_base_url", "").strip()
            if api_base_url and api_base_url != str(provider.api_base_url):
                updates["api_base_url"] = api_base_url

        if updates:
            try:
                registry.update_provider(self._current_provider_id, **updates)
            except Exception:
                pass

        # 持久化配置到磁盘
        self._persist_config()

    def _persist_config(self) -> None:
        """触发 ConfigPersistence.save() 将当前配置写入磁盘。

        使用 creart 获取 PathFunc 和 ProviderRegistry 单例，
        将完整的 providers 列表和活跃状态持久化到 agent_config.json。
        如果持久化失败，记录 error 日志但不阻塞 UI。
        """
        try:
            from src.core.agent.config_persistence import ConfigPersistence
            from src.core.runtime.paths import PathFunc

            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)

            # 加载现有配置并同步 providers 列表
            config_data = persistence.load()
            registry: ProviderRegistry = it(ProviderRegistry)
            config_data.providers = registry.list_all()

            persistence.save(config_data)
        except Exception as _exc:
            logger.error("ProviderDetailPanel 持久化配置失败")

    # ------------------------------------------------------------------
    # 槽函数 - 删除 / 模型列表
    # ------------------------------------------------------------------

    def _on_delete_clicked(self) -> None:
        """点击删除按钮时的处理."""
        if self._current_provider_id is None:
            return

        dialog = MessageBox(
            self.tr("确认删除"),
            self.tr("确定要删除该供应商吗？此操作不可撤销。"),
            self.window(),
        )
        if dialog.exec():
            provider_id = self._current_provider_id
            registry = it(ProviderRegistry)
            try:
                registry.unregister(provider_id)
            except KeyError:
                return

            self._current_provider_id = None
            self.provider_changed.emit(provider_id)

    def _on_model_changed(self, model_id: str) -> None:
        """模型添加或移除时的处理."""
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return

        registry.update_provider(self._current_provider_id, models=provider.models)
        self._persist_config()
        self.provider_changed.emit(self._current_provider_id)

    # ------------------------------------------------------------------
    # 检测按钮状态机
    # ------------------------------------------------------------------

    def _set_check_button_state(self, state: CheckButtonState) -> None:
        """设置检测按钮状态并更新 UI."""
        self._check_button_state = state
        check_btn = self._protocol_field_stack.get_current_check_button()
        if check_btn is None:
            return

        if state == CheckButtonState.IDLE:
            check_btn.setText(self.tr("检测"))
            check_btn.setIcon(FluentIcon.SEND)
            self._update_check_button_state()
        elif state == CheckButtonState.LOADING:
            check_btn.setText(self.tr("检测中..."))
            check_btn.setIcon(FluentIcon.SYNC)
            check_btn.setEnabled(False)
        elif state == CheckButtonState.SUCCESS:
            check_btn.setText(self.tr("✓"))
            check_btn.setIcon(FluentIcon.ACCEPT)
            check_btn.setEnabled(False)
        elif state == CheckButtonState.ERROR:
            check_btn.setText(self.tr("重试"))
            check_btn.setIcon(FluentIcon.SEND)
            self._update_check_button_state()

    def _restore_check_button_idle(self) -> None:
        """3 秒后从 Success 状态恢复到 Idle 状态."""
        if self._check_button_state == CheckButtonState.SUCCESS:
            self._set_check_button_state(CheckButtonState.IDLE)
