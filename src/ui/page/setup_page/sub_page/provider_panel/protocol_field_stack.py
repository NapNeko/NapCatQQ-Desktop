# -*- coding: utf-8 -*-
"""协议类型条件渲染组件.

根据 protocol_type 切换显示对应的配置字段组, 使用 QStackedWidget
预先创建所有页面并通过索引切换, 避免动态创建/销毁控件.

每个页面采用 "标签 + 工具按钮" 行 / 输入行 / 小字 caption 的纵向布局,
不再使用 QFormLayout 把标签放左侧:

::

    API 密钥                                       [⋯ 多密钥]
    [ PasswordLineEdit                          ] [检测]
                                  多个密钥使用逗号分隔  获取 API Key

    API 地址                                       [⚙ 自定义请求头]
    [ LineEdit                                                  ]
    预览: https://api.openai.com/v1/chat/completions

页面索引映射:
- 0: OpenAI 字段组 (API 密钥 + API 地址)
- 1: Anthropic 字段组 (API 密钥 + API 地址 + anthropic-version)
- 2: Azure 字段组 (API 密钥 + resource_endpoint + deployment_name + api_version)
- 3: Gemini 字段组 (API 密钥 + API 地址)

向外暴露的信号:
- ``check_clicked``: 用户点击 "检测" 按钮.
- ``multi_key_clicked``: 用户点击 "多密钥配置" 工具按钮.
- ``custom_headers_clicked``: 用户点击 "自定义请求头" 工具按钮.
"""
from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QSizePolicy,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    CaptionLabel,
    FluentIcon,
    HyperlinkLabel,
    LineEdit,
    PasswordLineEdit,
    PrimaryPushButton,
    StrongBodyLabel,
    TransparentToolButton,
    isDarkTheme,
    setFont,
)


# 协议类型到页面索引的映射
_PROTOCOL_INDEX_MAP: dict[str, int] = {
    "openai": 0,
    "anthropic": 1,
    "azure": 2,
    "gemini": 3,
}


class ProtocolFieldStack(QStackedWidget):
    """根据 protocol_type 切换显示对应的配置字段组.

    预先创建 OpenAI / Anthropic / Azure / Gemini 四个字段页面,
    通过 set_protocol() 切换当前可见页面. 切换时自动保留 API 密钥值.

    自适应高度: 默认 QStackedWidget 会用所有子页面中最大高度作为自身
    sizeHint, 导致 OpenAI 页面下方残留 Azure 页面预留出来的空白. 这里
    重写 sizeHint / minimumSizeHint, 使容器只跟随当前可见页面的尺寸.
    """

    # 共享信号: 任意页面上对应按钮被点击时发射
    check_clicked = Signal()
    multi_key_clicked = Signal()
    custom_headers_clicked = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)

        # 字段控件引用 (各页面通用字段)
        self._pages: dict[str, dict[str, LineEdit | PasswordLineEdit]] = {}
        # 每个页面的 "检测" 按钮 (用于状态机更新文字/图标)
        self._check_buttons: dict[str, PrimaryPushButton] = {}
        # 每个页面的 URL 预览 caption
        self._preview_labels: dict[str, CaptionLabel] = {}
        # 每个页面的 "获取 API Key" 超链接 (None 表示该页没创建)
        self._api_key_links: dict[str, HyperlinkLabel] = {}
        # 所有应用了 "低调样式" 的输入框, 用于主题切换时统一刷新 QSS
        self._subtle_inputs: list[LineEdit | PasswordLineEdit] = []

        # 让容器宽度可扩展, 高度跟随当前页面
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)

        self._build_openai_page()
        self._build_anthropic_page()
        self._build_azure_page()
        self._build_gemini_page()

        # 切换页面时通知布局重新计算尺寸, 避免空白预留
        self.currentChanged.connect(self._on_current_changed)

        # 主题切换时刷新输入框 QSS, 保持暗色/亮色下都低调
        from src.core.config import cfg

        cfg.themeChanged.connect(self._on_theme_changed)

    def _on_theme_changed(self, *_args) -> None:
        """主题切换时重新应用低调输入框样式."""
        qss = self._subtle_input_qss()
        for edit in self._subtle_inputs:
            edit.setStyleSheet(qss)

    # ------------------------------------------------------------------
    # 自适应当前页面的尺寸
    # ------------------------------------------------------------------

    def sizeHint(self) -> QSize:  # noqa: N802 - Qt 命名固定
        """让容器尺寸跟随当前可见页面."""
        widget = self.currentWidget()
        if widget is not None:
            return widget.sizeHint()
        return super().sizeHint()

    def minimumSizeHint(self) -> QSize:  # noqa: N802 - Qt 命名固定
        """最小尺寸也跟随当前页面."""
        widget = self.currentWidget()
        if widget is not None:
            return widget.minimumSizeHint()
        return super().minimumSizeHint()

    def _on_current_changed(self, _index: int) -> None:
        """切换页面后, 把非当前页面的 sizePolicy 设成 Ignored,
        当前页面设回 Preferred, 这样布局只用当前页面计算高度.
        """
        current = self.currentWidget()
        for i in range(self.count()):
            page = self.widget(i)
            if page is None:
                continue
            policy = page.sizePolicy()
            if page is current:
                policy.setVerticalPolicy(QSizePolicy.Policy.Preferred)
            else:
                policy.setVerticalPolicy(QSizePolicy.Policy.Ignored)
            page.setSizePolicy(policy)

        self.adjustSize()
        self.updateGeometry()

    # ------------------------------------------------------------------
    # 页面构建辅助
    # ------------------------------------------------------------------

    # 输入框统一样式: 整圈细边框 + 圆角, 透明底色,
    # hover / focus 时仅边框颜色变化, 不喧宾夺主.
    def _subtle_input_qss(self) -> str:
        """根据当前主题生成低调输入框 QSS."""
        if isDarkTheme():
            base_border = "rgba(255, 255, 255, 0.16)"
            hover_border = "rgba(255, 255, 255, 0.28)"
            hover_bg = "rgba(255, 255, 255, 0.04)"
            focus_bg = "rgba(255, 255, 255, 0.06)"
        else:
            base_border = "rgba(0, 0, 0, 0.12)"
            hover_border = "rgba(0, 0, 0, 0.22)"
            hover_bg = "rgba(0, 0, 0, 0.02)"
            focus_bg = "rgba(0, 0, 0, 0.03)"
        return (
            "LineEdit, PasswordLineEdit {"
            "  background-color: transparent;"
            f"  border: 1px solid {base_border};"
            "  border-radius: 6px;"
            "  padding: 5px 8px;"
            "}"
            "LineEdit:hover, PasswordLineEdit:hover {"
            f"  background-color: {hover_bg};"
            f"  border: 1px solid {hover_border};"
            "}"
            "LineEdit:focus, PasswordLineEdit:focus {"
            f"  background-color: {focus_bg};"
            "  border: 1px solid #0078d4;"
            "}"
        )

    def _make_page_layout(self, page: QWidget) -> QVBoxLayout:
        """生成统一规格的页面布局 (无外边距, 块间距 8px)."""
        layout = QVBoxLayout(page)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)
        return layout

    def _apply_subtle_style(self, edit: LineEdit | PasswordLineEdit) -> None:
        """应用低调样式到输入框: 透明底 + 细底边. 同时注册以便主题变更刷新."""
        edit.setStyleSheet(self._subtle_input_qss())
        self._subtle_inputs.append(edit)

    def _build_label_row(
        self,
        page: QWidget,
        label_text: str,
        tool_btn: TransparentToolButton | None,
    ) -> QHBoxLayout:
        """构建 "标签 + (右侧工具按钮)" 这一行."""
        row = QHBoxLayout()
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(6)

        label = StrongBodyLabel(label_text, page)
        row.addWidget(label, 0, Qt.AlignmentFlag.AlignVCenter)
        row.addStretch(1)
        if tool_btn is not None:
            row.addWidget(tool_btn, 0, Qt.AlignmentFlag.AlignVCenter)
        return row

    def _build_api_key_block(
        self, page: QWidget, protocol_key: str, parent_layout: QVBoxLayout
    ) -> PasswordLineEdit:
        """构建 "API 密钥" 块 (标签行 + 输入行 + 可选超链接).

        不再展示 "多个密钥用逗号分隔" 提示, 这一信息由多密钥工具按钮的 tooltip 承载.
        """
        # 多密钥工具按钮 (放在标签行右上角)
        multi_key_btn = TransparentToolButton(FluentIcon.MORE, page)
        multi_key_btn.setFixedSize(28, 28)
        multi_key_btn.setToolTip(self.tr("配置多个 API 密钥 (使用时随机轮转)"))
        multi_key_btn.clicked.connect(self.multi_key_clicked)

        parent_layout.addLayout(self._build_label_row(page, self.tr("API 密钥"), multi_key_btn))

        # 输入行: PasswordLineEdit + 检测按钮
        input_row = QHBoxLayout()
        input_row.setContentsMargins(0, 0, 0, 0)
        input_row.setSpacing(6)

        api_key_edit = PasswordLineEdit(page)
        api_key_edit.setPlaceholderText(self.tr("输入 API 密钥..."))
        self._apply_subtle_style(api_key_edit)
        input_row.addWidget(api_key_edit, 1)

        check_btn = PrimaryPushButton(self.tr("检测"), page)
        check_btn.setIcon(FluentIcon.SEND)
        check_btn.setEnabled(False)
        check_btn.clicked.connect(self.check_clicked)
        input_row.addWidget(check_btn, 0)

        parent_layout.addLayout(input_row)

        # "获取 API Key" 超链接 - 默认隐藏, 调用方按需显示
        api_key_link = HyperlinkLabel(page)
        api_key_link.setText(self.tr("获取 API Key"))
        api_key_link.setVisible(False)
        link_row = QHBoxLayout()
        link_row.setContentsMargins(0, 0, 0, 0)
        link_row.setSpacing(0)
        link_row.addStretch(1)
        link_row.addWidget(api_key_link, 0, Qt.AlignmentFlag.AlignRight)
        parent_layout.addLayout(link_row)

        self._check_buttons[protocol_key] = check_btn
        self._api_key_links[protocol_key] = api_key_link
        return api_key_edit

    def _build_url_block(
        self,
        page: QWidget,
        protocol_key: str,
        parent_layout: QVBoxLayout,
        label_text: str,
        placeholder: str,
        with_custom_headers_button: bool = True,
    ) -> LineEdit:
        """构建 "API 地址" 类块 (标签行 + 输入行 + 预览 caption).

        Args:
            with_custom_headers_button: 是否在标签行右上角放 "自定义请求头" 工具按钮.
                同一个页面只在第一个 URL 块上挂这个按钮, 避免按钮重复.
        """
        headers_btn: TransparentToolButton | None = None
        if with_custom_headers_button:
            headers_btn = TransparentToolButton(FluentIcon.SETTING, page)
            headers_btn.setFixedSize(28, 28)
            headers_btn.setToolTip(self.tr("配置自定义请求头"))
            headers_btn.clicked.connect(self.custom_headers_clicked)

        parent_layout.addLayout(self._build_label_row(page, label_text, headers_btn))

        url_edit = LineEdit(page)
        url_edit.setPlaceholderText(placeholder)
        url_edit.setClearButtonEnabled(True)
        self._apply_subtle_style(url_edit)
        parent_layout.addWidget(url_edit)

        # 仅第一个 URL 块持有预览 caption (Azure 多个字段时挂在 endpoint 行下方)
        if with_custom_headers_button:
            preview_label = CaptionLabel("", page)
            setFont(preview_label, 11)
            preview_label.setObjectName("urlPreviewLabel")
            preview_label.setStyleSheet(
                "QLabel#urlPreviewLabel { color: rgba(128, 128, 128, 0.85); }"
            )
            parent_layout.addWidget(preview_label)
            self._preview_labels[protocol_key] = preview_label

        return url_edit

    # ------------------------------------------------------------------
    # 页面构建
    # ------------------------------------------------------------------

    def _build_openai_page(self) -> None:
        """OpenAI: API 密钥 + API 地址."""
        page = QWidget(self)
        layout = self._make_page_layout(page)

        api_key_edit = self._build_api_key_block(page, "openai", layout)
        api_base_url_edit = self._build_url_block(
            page,
            "openai",
            layout,
            self.tr("API 地址"),
            self.tr("https://api.openai.com/v1"),
        )

        self.addWidget(page)
        self._pages["openai"] = {
            "api_key": api_key_edit,
            "api_base_url": api_base_url_edit,
        }

    def _build_anthropic_page(self) -> None:
        """Anthropic: API 密钥 + API 地址 + anthropic-version."""
        page = QWidget(self)
        layout = self._make_page_layout(page)

        api_key_edit = self._build_api_key_block(page, "anthropic", layout)
        api_base_url_edit = self._build_url_block(
            page,
            "anthropic",
            layout,
            self.tr("API 地址"),
            self.tr("https://api.anthropic.com/v1"),
        )

        # anthropic-version 单独一块, 用普通字段, 不需要 caption / 工具按钮
        layout.addLayout(self._build_label_row(page, self.tr("anthropic-version"), None))
        anthropic_version_edit = LineEdit(page)
        anthropic_version_edit.setPlaceholderText(self.tr("例如: 2023-06-01"))
        self._apply_subtle_style(anthropic_version_edit)
        layout.addWidget(anthropic_version_edit)

        self.addWidget(page)
        self._pages["anthropic"] = {
            "api_key": api_key_edit,
            "api_base_url": api_base_url_edit,
            "anthropic_version": anthropic_version_edit,
        }

    def _build_azure_page(self) -> None:
        """Azure: API 密钥 + resource_endpoint + deployment_name + api_version."""
        page = QWidget(self)
        layout = self._make_page_layout(page)

        api_key_edit = self._build_api_key_block(page, "azure", layout)
        resource_endpoint_edit = self._build_url_block(
            page,
            "azure",
            layout,
            self.tr("Resource Endpoint"),
            self.tr("例如: https://your-resource.openai.azure.com"),
        )

        layout.addLayout(self._build_label_row(page, self.tr("Deployment Name"), None))
        deployment_name_edit = LineEdit(page)
        deployment_name_edit.setPlaceholderText(self.tr("输入部署名称..."))
        self._apply_subtle_style(deployment_name_edit)
        layout.addWidget(deployment_name_edit)

        layout.addLayout(self._build_label_row(page, self.tr("API Version"), None))
        api_version_edit = LineEdit(page)
        api_version_edit.setPlaceholderText(self.tr("例如: 2024-02-01"))
        api_version_edit.setText("2024-02-01")
        self._apply_subtle_style(api_version_edit)
        layout.addWidget(api_version_edit)

        self.addWidget(page)
        self._pages["azure"] = {
            "api_key": api_key_edit,
            "resource_endpoint": resource_endpoint_edit,
            "deployment_name": deployment_name_edit,
            "api_version": api_version_edit,
        }

    def _build_gemini_page(self) -> None:
        """Gemini: API 密钥 + API 地址."""
        page = QWidget(self)
        layout = self._make_page_layout(page)

        api_key_edit = self._build_api_key_block(page, "gemini", layout)
        api_base_url_edit = self._build_url_block(
            page,
            "gemini",
            layout,
            self.tr("API 地址"),
            self.tr("https://generativelanguage.googleapis.com/v1beta"),
        )

        self.addWidget(page)
        self._pages["gemini"] = {
            "api_key": api_key_edit,
            "api_base_url": api_base_url_edit,
        }

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def set_protocol(self, protocol_type: str) -> None:
        """切换到指定协议类型的字段页面.

        切换时自动将当前页面的 API 密钥值同步到目标页面.

        Args:
            protocol_type: 协议类型字符串.
        """
        current_api_key = self._get_current_api_key()

        target_index = _PROTOCOL_INDEX_MAP.get(protocol_type, 0)
        target_protocol = protocol_type if protocol_type in _PROTOCOL_INDEX_MAP else "openai"

        self.setCurrentIndex(target_index)

        if current_api_key:
            target_fields = self._pages[target_protocol]
            if "api_key" in target_fields:
                target_fields["api_key"].setText(current_api_key)

    def get_field_values(self) -> dict[str, str]:
        """获取当前活动页面的所有字段值."""
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})
        return {key: widget.text() for key, widget in fields.items()}

    def set_field_values(self, values: dict[str, str]) -> None:
        """设置当前活动页面的字段值, api_key 会同步到所有协议页面."""
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})

        for key, widget in fields.items():
            if key in values:
                widget.setText(values[key])

        if "api_key" in values:
            self._sync_api_key_to_all(values["api_key"])

    def get_current_check_button(self) -> PrimaryPushButton | None:
        """返回当前可见页面的检测按钮."""
        return self._check_buttons.get(self._get_current_protocol())

    def set_url_preview(self, text: str) -> None:
        """更新当前页面的 URL 预览 caption."""
        label = self._preview_labels.get(self._get_current_protocol())
        if label is not None:
            label.setText(text)

    def set_api_key_url(self, url: str | None) -> None:
        """配置 "获取 API Key" 超链接.

        所有页面共享一份 url, 调 ``None`` 隐藏链接, 否则同步显示并指向 url.
        """
        for link in self._api_key_links.values():
            if url:
                link.setUrl(url)
                link.setVisible(True)
            else:
                link.setVisible(False)

    # ------------------------------------------------------------------
    # 内部方法
    # ------------------------------------------------------------------

    def _get_current_protocol(self) -> str:
        """获取当前活动页面对应的协议类型字符串."""
        current_index = self.currentIndex()
        for protocol, index in _PROTOCOL_INDEX_MAP.items():
            if index == current_index:
                return protocol
        return "openai"

    def _get_current_api_key(self) -> str:
        """获取当前活动页面的 API 密钥值."""
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})
        api_key_widget = fields.get("api_key")
        if api_key_widget is not None:
            return api_key_widget.text()
        return ""

    def _sync_api_key_to_all(self, api_key: str) -> None:
        """将 API 密钥值同步到所有页面的 api_key 字段."""
        for fields in self._pages.values():
            api_key_widget = fields.get("api_key")
            if api_key_widget is not None:
                api_key_widget.setText(api_key)

    def connect_field_text_changed(self, slot) -> None:
        """将所有页面所有字段的 textChanged 信号连接到指定槽函数."""
        for fields in self._pages.values():
            for widget in fields.values():
                widget.textChanged.connect(slot)
