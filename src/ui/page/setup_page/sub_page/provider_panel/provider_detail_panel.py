# -*- coding: utf-8 -*-
"""供应商详情面板.

右侧详情面板, 展示选中供应商的完整配置信息, 包括:
- Header 区域: 供应商名称 + 协议标签 + 外部链接按钮 + 启用开关 + 删除按钮
- API 配置区域: API 密钥输入框 + 检测按钮 + API 地址输入框 + URL 预览
- 模型区域: 嵌入 ModelListWidget
"""
from __future__ import annotations

from enum import Enum, auto

from creart import it
from PySide6.QtCore import Qt, QTimer, QUrl, Signal
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import QFrame, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    CaptionLabel,
    FluentIcon,
    HyperlinkLabel,
    IndeterminateProgressRing,
    InfoBar,
    MessageBox,
    PrimaryPushButton,
    ScrollArea,
    SubtitleLabel,
    SwitchButton,
    TransparentToolButton,
)

from src.core.agent.api_check_service import ApiCheckService
from src.core.agent.provider import ProviderRegistry
from src.core.config import cfg
from src.ui.components.setting_subtitle import SettingSubtitle

from .model_list_widget import ModelListWidget
from .protocol_field_stack import ProtocolFieldStack
from .provider_protocol_utils import build_url_preview, get_protocol_label
from .setting_group import SettingGroup


class CheckButtonState(Enum):
    """检测按钮状态机枚举."""

    IDLE = auto()
    LOADING = auto()
    SUCCESS = auto()
    ERROR = auto()


class ProviderDetailPanel(ScrollArea):
    """右侧供应商详情面板 -- API 配置 + 模型列表.

    展示选中供应商的详细配置, 支持启用/禁用切换, API 连通性检测,
    模型列表管理和供应商删除操作.

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

        # 检测按钮状态机
        self._check_button_state: CheckButtonState = CheckButtonState.IDLE
        self._warning_icon: TransparentToolButton | None = None
        self._last_error_message: str = ""
        self._check_spinner: IndeterminateProgressRing | None = None

        self._setup_ui()
        self._connect_signals()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建面板整体 UI 布局."""
        # 内容容器
        self._content_widget = QWidget(self)
        self._content_layout = QVBoxLayout(self._content_widget)
        self._content_layout.setContentsMargins(15, 18, 15, 18)
        self._content_layout.setSpacing(16)

        self._setup_header()
        self._setup_api_section()
        self._setup_model_section()
        self._content_layout.addStretch()

        # ScrollArea 设置
        self.setWidget(self._content_widget)
        self.setWidgetResizable(True)

        # 隐藏垂直滚动条但保留滚动功能
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        # 应用主题自适应背景色
        self._apply_scroll_area_theme_style()
        cfg.themeChanged.connect(self._apply_scroll_area_theme_style)

    def _apply_scroll_area_theme_style(self, *_args) -> None:
        """让滚动容器与主窗口背景保持一致, 不再使用区分色.

        设置透明背景, 既适配亮色和暗色主题, 也避免与左侧列表面板
        产生灰色突兀的视觉割裂感.
        """
        # 同时取消视口背景, 否则 ScrollArea 内仍会出现默认底色
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
        """构建 Header 区域: 供应商名称 + 协议标签 + 外部链接按钮 + 开关 + 删除按钮 + Divider."""
        header_layout = QHBoxLayout()
        header_layout.setContentsMargins(0, 0, 0, 0)
        header_layout.setSpacing(12)

        # 供应商名称
        self._name_label = SubtitleLabel("", self._content_widget)
        header_layout.addWidget(self._name_label)

        # 协议标签
        self._protocol_label = CaptionLabel("", self._content_widget)
        header_layout.addWidget(self._protocol_label)

        # 外部链接按钮（仅在 website_url 非空时显示）
        self._external_link_button = TransparentToolButton(FluentIcon.LINK, self._content_widget)
        self._external_link_button.setFixedSize(32, 32)
        self._external_link_button.setVisible(False)
        header_layout.addWidget(self._external_link_button)

        header_layout.addStretch()

        # 启用/禁用开关
        self._enable_switch = SwitchButton(self._content_widget)
        header_layout.addWidget(self._enable_switch)

        # 删除按钮
        self._delete_button = TransparentToolButton(FluentIcon.DELETE, self._content_widget)
        self._delete_button.setFixedSize(32, 32)
        header_layout.addWidget(self._delete_button)

        self._content_layout.addLayout(header_layout)

        # 水平 Divider 分隔线
        self._header_divider = QFrame(self._content_widget)
        self._header_divider.setFrameShape(QFrame.Shape.HLine)
        self._header_divider.setFrameShadow(QFrame.Shadow.Sunken)
        self._content_layout.addWidget(self._header_divider)

    def _setup_api_section(self) -> None:
        """构建 API 配置区域: SettingSubtitle + SettingGroup 卡片 + ProtocolFieldStack."""
        # 分区标题
        self._api_subtitle = SettingSubtitle(self.tr("API 配置"), self._content_widget)
        self._content_layout.addWidget(self._api_subtitle)

        # 标题与 SettingGroup 之间间距 8px
        self._content_layout.addSpacing(8)

        # SettingGroup 卡片容器
        self._api_setting_group = SettingGroup(self._content_widget)
        api_group_layout = self._api_setting_group._layout

        # ProtocolFieldStack 集成到 SettingGroup 内部
        self._protocol_field_stack = ProtocolFieldStack(self._api_setting_group)
        api_group_layout.addWidget(self._protocol_field_stack)

        # Help_Text: "多个密钥用逗号分隔" + 可选 "获取 API Key" 超链接
        help_text_layout = QHBoxLayout()
        help_text_layout.setContentsMargins(0, 0, 0, 0)
        help_text_layout.setSpacing(8)

        self._api_key_help_label = CaptionLabel(
            self.tr("多个密钥用逗号分隔"), self._api_setting_group
        )
        self._api_key_help_label.setObjectName("apiKeyHelpLabel")
        self._api_key_help_label.setStyleSheet(
            "QLabel#apiKeyHelpLabel { opacity: 0.6; }"
        )
        help_text_layout.addWidget(self._api_key_help_label)

        # "获取 API Key" 超链接（默认隐藏，当 api_key_url 非空时显示）
        # HyperlinkLabel 的 __init__(parent=None) 不支持位置参数 text,
        # 需要先构造再调用 setText / setUrl
        self._api_key_url_link = HyperlinkLabel(self._api_setting_group)
        self._api_key_url_link.setText(self.tr("获取 API Key"))
        self._api_key_url_link.setVisible(False)
        help_text_layout.addWidget(self._api_key_url_link)

        help_text_layout.addStretch()
        api_group_layout.addLayout(help_text_layout)

        # 检测按钮（放在 SettingGroup 内部）
        check_button_layout = QHBoxLayout()
        check_button_layout.setContentsMargins(0, 0, 0, 0)
        check_button_layout.setSpacing(8)

        # Spinner 动画（Loading 状态时显示）
        # 默认创建后即开始旋转, 这里需要立即停止以避免不必要的重绘和 QPainter 警告
        self._check_spinner = IndeterminateProgressRing(self._api_setting_group, start=False)
        self._check_spinner.setFixedSize(20, 20)
        self._check_spinner.setStrokeWidth(3)
        self._check_spinner.setVisible(False)
        check_button_layout.addWidget(self._check_spinner)

        check_button_layout.addStretch()

        # 警告三角图标占位（Error 状态时动态添加到此布局）
        self._warning_icon_layout = check_button_layout

        self._check_api_button = PrimaryPushButton(self.tr("检测"), self._api_setting_group)
        self._check_api_button.setEnabled(False)
        check_button_layout.addWidget(self._check_api_button)

        api_group_layout.addLayout(check_button_layout)

        # URL 预览（实时更新，调用 build_url_preview 纯函数）
        self._url_preview_label = CaptionLabel("", self._api_setting_group)
        api_group_layout.addWidget(self._url_preview_label)

        self._content_layout.addWidget(self._api_setting_group)

    def _setup_model_section(self) -> None:
        """构建模型区域: SettingSubtitle + SettingGroup 卡片包裹 ModelListWidget."""
        # 分区标题
        self._model_subtitle = SettingSubtitle(self.tr("模型列表"), self._content_widget)
        self._content_layout.addWidget(self._model_subtitle)

        # 标题与 SettingGroup 之间间距 8px
        self._content_layout.addSpacing(8)

        # SettingGroup 卡片容器包裹 ModelListWidget
        self._model_setting_group = SettingGroup(self._content_widget)
        model_group_layout = self._model_setting_group._layout

        self._model_list_widget = ModelListWidget(self._model_setting_group)
        model_group_layout.addWidget(self._model_list_widget)

        self._content_layout.addWidget(self._model_setting_group)

    def _connect_signals(self) -> None:
        """连接内部信号与槽."""
        self._enable_switch.checkedChanged.connect(self._on_enable_toggled)
        self._delete_button.clicked.connect(self._on_delete_clicked)
        self._external_link_button.clicked.connect(self._on_external_link_clicked)
        self._check_api_button.clicked.connect(self._on_check_api_clicked)
        self._model_list_widget.model_added.connect(self._on_model_changed)
        self._model_list_widget.model_removed.connect(self._on_model_changed)

        # 连接 ProtocolFieldStack 内部字段的 textChanged 信号
        # 用于实时更新 URL 预览和检测按钮状态
        self._protocol_field_stack.connect_field_text_changed(self._on_field_text_changed)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def load_provider(self, provider_id: str) -> None:
        """加载指定供应商的配置到面板.

        从 ProviderRegistry 获取 Provider 实例并填充所有 UI 控件.

        Args:
            provider_id: 要加载的供应商 ID.
        """
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(provider_id)
        except KeyError:
            return

        self._current_provider_id = provider_id
        self._current_protocol_type = provider.protocol_type

        # 填充 Header
        self._name_label.setText(provider.name)
        self._protocol_label.setText(get_protocol_label(provider.protocol_type))
        self._enable_switch.setChecked(provider.enabled)

        # 外部链接按钮：仅在 website_url 非空时显示
        has_website = bool(provider.website_url)
        self._external_link_button.setVisible(has_website)
        self._current_website_url = provider.website_url if has_website else None

        # "获取 API Key" 超链接：仅在 api_key_url 非空时显示
        has_api_key_url = bool(provider.api_key_url)
        self._api_key_url_link.setVisible(has_api_key_url)
        if has_api_key_url:
            self._api_key_url_link.setUrl(provider.api_key_url)
            self._current_api_key_url = provider.api_key_url
        else:
            self._current_api_key_url = None

        # 填充 API 配置 (通过 ProtocolFieldStack)
        self._protocol_field_stack.set_protocol(provider.protocol_type)
        field_values: dict[str, str] = {"api_key": provider.api_key_ref}
        if hasattr(provider, "api_base_url") and provider.api_base_url:
            field_values["api_base_url"] = str(provider.api_base_url)
        if provider.azure_config:
            field_values["resource_endpoint"] = provider.azure_config.resource_endpoint
            field_values["deployment_name"] = provider.azure_config.deployment_name
            field_values["api_version"] = provider.azure_config.api_version
        self._protocol_field_stack.set_field_values(field_values)

        # 更新 URL 预览
        self._update_url_preview()

        # 加载模型列表
        self._model_list_widget.set_provider(provider)

        # 更新检测按钮状态
        self._update_check_button_state()

    # ------------------------------------------------------------------
    # 槽函数
    # ------------------------------------------------------------------

    def _on_enable_toggled(self, checked: bool) -> None:
        """启用/禁用开关切换时的处理.

        调用 registry.set_enabled() 更新状态, 并发射 provider_changed 信号.

        Args:
            checked: 开关是否选中(启用).
        """
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        registry.set_enabled(self._current_provider_id, checked)
        self.provider_changed.emit(self._current_provider_id)

    def _on_external_link_clicked(self) -> None:
        """点击外部链接按钮时的处理.

        通过 QDesktopServices 在系统默认浏览器中打开 website_url.
        """
        if self._current_website_url:
            QDesktopServices.openUrl(QUrl(self._current_website_url))

    def _on_field_text_changed(self, *_args) -> None:
        """ProtocolFieldStack 内部字段文本变更时的处理.

        实时更新 URL 预览和检测按钮状态.
        如果当前处于 Error 状态, 移除警告三角图标并恢复 Idle 状态.
        """
        # 如果处于 Error 状态, 用户修改输入框时移除警告图标
        if self._check_button_state == CheckButtonState.ERROR:
            self._remove_warning_icon()
            self._check_button_state = CheckButtonState.IDLE

        self._update_url_preview()
        self._update_check_button_state()

    def _on_check_api_clicked(self) -> None:
        """点击 API 检测按钮时的处理.

        创建 ApiCheckService 实例, 连接信号并启动检测.
        进入 Loading 状态: 显示 spinner + 禁用按钮.
        """
        if self._current_provider_id is None:
            return

        field_values = self._protocol_field_stack.get_field_values()
        api_key = field_values.get("api_key", "").strip()
        api_base_url = field_values.get("api_base_url", "").strip()

        if not api_key or not api_base_url:
            return

        # 进入 Loading 状态
        self._set_check_button_state(CheckButtonState.LOADING)

        # 创建检测服务
        self._api_check_service = ApiCheckService(self)
        self._api_check_service.check_finished.connect(self._on_check_finished)
        self._api_check_service.start_check(api_base_url, api_key)

    def _on_check_finished(self, success: bool, message: str) -> None:
        """API 检测完成时的处理.

        Success: 显示绿色对勾图标, 3 秒后恢复默认.
        Error: 显示 InfoBar 错误提示 + 密钥输入框右侧警告三角图标.

        Args:
            success: 检测是否成功.
            message: 检测结果消息.
        """
        if success:
            # 进入 Success 状态
            self._set_check_button_state(CheckButtonState.SUCCESS)
            InfoBar.success(
                self.tr("成功"),
                message,
                duration=3000,
                parent=self.window(),
            )
            # 3 秒后恢复 Idle 状态
            QTimer.singleShot(3000, self._restore_check_button_idle)
        else:
            # 进入 Error 状态
            self._last_error_message = message
            self._set_check_button_state(CheckButtonState.ERROR)
            InfoBar.error(
                self.tr("失败"),
                message,
                duration=5000,
                parent=self.window(),
            )

    def _update_url_preview(self) -> None:
        """更新 URL 预览标签.

        调用 build_url_preview() 纯函数生成预览文本,
        根据当前协议类型和 API 地址实时更新.
        """
        field_values = self._protocol_field_stack.get_field_values()
        url = field_values.get("api_base_url", "").strip()
        azure_api_version = field_values.get("api_version", "")
        preview = build_url_preview(url, self._current_protocol_type, azure_api_version)
        self._url_preview_label.setText(preview)

    def _update_check_button_state(self) -> None:
        """更新 API 检测按钮的启用/禁用状态.

        仅当 api_key 和 api_base_url 均非空(strip 后)时启用按钮.
        """
        field_values = self._protocol_field_stack.get_field_values()
        api_key = field_values.get("api_key", "").strip()
        api_base_url = field_values.get("api_base_url", "").strip()
        self._check_api_button.setEnabled(bool(api_key and api_base_url))

    def _on_delete_clicked(self) -> None:
        """点击删除按钮时的处理.

        弹出确认对话框, 确认后调用 registry.unregister() 注销供应商.
        """
        if self._current_provider_id is None:
            return

        # 弹出确认对话框
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

    def _on_reset_url_clicked(self) -> None:
        """点击重置 URL 按钮时的处理.

        将 API 地址重置为 ProviderRegistry 中存储的原始值.
        """
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return

        # 通过 ProtocolFieldStack 设置 api_base_url 字段
        field_values = self._protocol_field_stack.get_field_values()
        field_values["api_base_url"] = str(provider.api_base_url)
        self._protocol_field_stack.set_field_values(field_values)

    def _on_model_changed(self, model_id: str) -> None:
        """模型添加或移除时的处理.

        保存供应商配置并发射 provider_changed 信号.

        Args:
            model_id: 变更的模型 ID.
        """
        if self._current_provider_id is None:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._current_provider_id)
        except KeyError:
            return

        # 更新 provider 的 models 到 registry
        registry.update_provider(self._current_provider_id, models=provider.models)
        self.provider_changed.emit(self._current_provider_id)

    # ------------------------------------------------------------------
    # 检测按钮状态机
    # ------------------------------------------------------------------

    def _set_check_button_state(self, state: CheckButtonState) -> None:
        """设置检测按钮状态并更新 UI.

        状态转换:
        - IDLE: 默认文本 "检测", 根据输入内容启用/禁用
        - LOADING: 显示 spinner + 按钮文本 "检测中..." + 禁用按钮
        - SUCCESS: 显示绿色对勾图标 + 禁用按钮
        - ERROR: 恢复按钮默认 + 添加警告三角图标

        Args:
            state: 目标状态.
        """
        self._check_button_state = state

        if state == CheckButtonState.IDLE:
            # 恢复默认状态
            self._check_api_button.setText(self.tr("检测"))
            self._check_api_button.setIcon(FluentIcon.SEND)
            self._check_spinner.stop()
            self._check_spinner.setVisible(False)
            self._update_check_button_state()

        elif state == CheckButtonState.LOADING:
            # 显示 spinner + 禁用按钮
            self._check_api_button.setText(self.tr("检测中..."))
            self._check_api_button.setEnabled(False)
            self._check_spinner.setVisible(True)
            self._check_spinner.start()
            # 移除之前的警告图标（如果从 Error 状态再次点击检测）
            self._remove_warning_icon()

        elif state == CheckButtonState.SUCCESS:
            # 显示绿色对勾图标
            self._check_spinner.stop()
            self._check_spinner.setVisible(False)
            self._check_api_button.setText(self.tr("✓"))
            self._check_api_button.setIcon(FluentIcon.ACCEPT)
            self._check_api_button.setEnabled(False)

        elif state == CheckButtonState.ERROR:
            # 恢复按钮默认文本 + 添加警告三角图标
            self._check_spinner.stop()
            self._check_spinner.setVisible(False)
            self._check_api_button.setText(self.tr("检测"))
            self._check_api_button.setIcon(FluentIcon.SEND)
            self._update_check_button_state()
            self._add_warning_icon()

    def _restore_check_button_idle(self) -> None:
        """3 秒后从 Success 状态恢复到 Idle 状态.

        仅在当前仍处于 Success 状态时执行恢复,
        避免用户在 3 秒内进行其他操作导致状态冲突.
        """
        if self._check_button_state == CheckButtonState.SUCCESS:
            self._set_check_button_state(CheckButtonState.IDLE)

    def _add_warning_icon(self) -> None:
        """在检测按钮左侧添加警告三角图标.

        点击图标显示错误详情弹窗.
        """
        if self._warning_icon is not None:
            return  # 已存在, 不重复添加

        self._warning_icon = TransparentToolButton(
            FluentIcon.INFO, self._api_setting_group
        )
        self._warning_icon.setFixedSize(28, 28)
        # 使用橙色/警告色样式
        self._warning_icon.setStyleSheet(
            "TransparentToolButton { color: #d83b01; }"
        )
        self._warning_icon.setToolTip(self.tr("检测失败，点击查看详情"))
        self._warning_icon.clicked.connect(self._on_warning_icon_clicked)

        # 插入到检测按钮左侧（在 stretch 之后, 按钮之前）
        button_index = self._warning_icon_layout.indexOf(self._check_api_button)
        if button_index >= 0:
            self._warning_icon_layout.insertWidget(button_index, self._warning_icon)

    def _remove_warning_icon(self) -> None:
        """移除警告三角图标.

        当用户修改输入框内容时调用, 清除 Error 状态的视觉指示.
        """
        if self._warning_icon is not None:
            self._warning_icon.setVisible(False)
            self._warning_icon_layout.removeWidget(self._warning_icon)
            self._warning_icon.deleteLater()
            self._warning_icon = None

    def _on_warning_icon_clicked(self) -> None:
        """点击警告三角图标时显示错误详情弹窗.

        弹窗内容为最近一次检测失败的 message 文本.
        """
        if not self._last_error_message:
            return

        dialog = MessageBox(
            self.tr("API 检测失败"),
            self._last_error_message,
            self.window(),
        )
        dialog.exec()
