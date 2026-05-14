# -*- coding: utf-8 -*-
"""协议类型条件渲染组件.

根据 protocol_type 切换显示对应的配置字段组, 使用 QStackedWidget
预先创建所有页面并通过索引切换, 避免动态创建/销毁控件.

页面索引映射:
- 0: OpenAI 字段组 (API 密钥 + API 地址)
- 1: Anthropic 字段组 (API 密钥 + API 地址 + anthropic-version)
- 2: Azure 字段组 (API 密钥 + resource_endpoint + deployment_name + api_version)
- 3: Gemini 字段组 (API 密钥 + API 地址)
"""
from __future__ import annotations

from PySide6.QtCore import QSize
from PySide6.QtWidgets import QFormLayout, QSizePolicy, QStackedWidget, QWidget
from qfluentwidgets import BodyLabel, LineEdit, PasswordLineEdit


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

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)

        # 存储各页面的字段控件引用
        self._pages: dict[str, dict[str, LineEdit | PasswordLineEdit]] = {}

        # 让容器宽度可扩展, 高度跟随当前页面
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)

        self._build_openai_page()
        self._build_anthropic_page()
        self._build_azure_page()
        self._build_gemini_page()

        # 切换页面时通知布局重新计算尺寸, 避免空白预留
        self.currentChanged.connect(self._on_current_changed)

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
        然后强制刷新自身布局.
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
    # 页面构建
    # ------------------------------------------------------------------

    def _build_openai_page(self) -> None:
        """构建 OpenAI 字段页面 (索引 0): API 密钥 + API 地址."""
        page = QWidget(self)
        layout = QFormLayout(page)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(12)

        api_key_edit = PasswordLineEdit(page)
        api_key_edit.setPlaceholderText(self.tr("输入 API 密钥..."))
        layout.addRow(BodyLabel(self.tr("API 密钥"), page), api_key_edit)

        api_base_url_edit = LineEdit(page)
        api_base_url_edit.setPlaceholderText(self.tr("输入 API 地址..."))
        layout.addRow(BodyLabel(self.tr("API 地址"), page), api_base_url_edit)

        self.addWidget(page)
        self._pages["openai"] = {
            "api_key": api_key_edit,
            "api_base_url": api_base_url_edit,
        }

    def _build_anthropic_page(self) -> None:
        """构建 Anthropic 字段页面 (索引 1): API 密钥 + API 地址 + anthropic-version."""
        page = QWidget(self)
        layout = QFormLayout(page)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(12)

        api_key_edit = PasswordLineEdit(page)
        api_key_edit.setPlaceholderText(self.tr("输入 API 密钥..."))
        layout.addRow(BodyLabel(self.tr("API 密钥"), page), api_key_edit)

        api_base_url_edit = LineEdit(page)
        api_base_url_edit.setPlaceholderText(self.tr("输入 API 地址..."))
        layout.addRow(BodyLabel(self.tr("API 地址"), page), api_base_url_edit)

        anthropic_version_edit = LineEdit(page)
        anthropic_version_edit.setPlaceholderText(self.tr("例如: 2023-06-01"))
        layout.addRow(BodyLabel(self.tr("anthropic-version"), page), anthropic_version_edit)

        self.addWidget(page)
        self._pages["anthropic"] = {
            "api_key": api_key_edit,
            "api_base_url": api_base_url_edit,
            "anthropic_version": anthropic_version_edit,
        }

    def _build_azure_page(self) -> None:
        """构建 Azure 字段页面 (索引 2): API 密钥 + resource_endpoint + deployment_name + api_version."""
        page = QWidget(self)
        layout = QFormLayout(page)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(12)

        api_key_edit = PasswordLineEdit(page)
        api_key_edit.setPlaceholderText(self.tr("输入 API 密钥..."))
        layout.addRow(BodyLabel(self.tr("API 密钥"), page), api_key_edit)

        resource_endpoint_edit = LineEdit(page)
        resource_endpoint_edit.setPlaceholderText(
            self.tr("例如: https://your-resource.openai.azure.com")
        )
        layout.addRow(BodyLabel(self.tr("Resource Endpoint"), page), resource_endpoint_edit)

        deployment_name_edit = LineEdit(page)
        deployment_name_edit.setPlaceholderText(self.tr("输入部署名称..."))
        layout.addRow(BodyLabel(self.tr("Deployment Name"), page), deployment_name_edit)

        api_version_edit = LineEdit(page)
        api_version_edit.setPlaceholderText(self.tr("例如: 2024-02-01"))
        api_version_edit.setText("2024-02-01")
        layout.addRow(BodyLabel(self.tr("API Version"), page), api_version_edit)

        self.addWidget(page)
        self._pages["azure"] = {
            "api_key": api_key_edit,
            "resource_endpoint": resource_endpoint_edit,
            "deployment_name": deployment_name_edit,
            "api_version": api_version_edit,
        }

    def _build_gemini_page(self) -> None:
        """构建 Gemini 字段页面 (索引 3): API 密钥 + API 地址."""
        page = QWidget(self)
        layout = QFormLayout(page)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(12)

        api_key_edit = PasswordLineEdit(page)
        api_key_edit.setPlaceholderText(self.tr("输入 API 密钥..."))
        layout.addRow(BodyLabel(self.tr("API 密钥"), page), api_key_edit)

        api_base_url_edit = LineEdit(page)
        api_base_url_edit.setPlaceholderText(self.tr("输入 API 地址..."))
        layout.addRow(BodyLabel(self.tr("API 地址"), page), api_base_url_edit)

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

        切换时自动将当前页面的 API 密钥值同步到目标页面,
        确保 API 密钥在协议切换时不丢失.

        未知协议类型回退到 openai 页面 (索引 0).

        Args:
            protocol_type: 协议类型字符串.
        """
        # 获取当前页面的 API 密钥值
        current_api_key = self._get_current_api_key()

        # 确定目标页面索引
        target_index = _PROTOCOL_INDEX_MAP.get(protocol_type, 0)
        target_protocol = protocol_type if protocol_type in _PROTOCOL_INDEX_MAP else "openai"

        # 切换页面
        self.setCurrentIndex(target_index)

        # 将 API 密钥同步到目标页面
        if current_api_key:
            target_fields = self._pages[target_protocol]
            if "api_key" in target_fields:
                target_fields["api_key"].setText(current_api_key)

    def get_field_values(self) -> dict[str, str]:
        """获取当前活动页面的所有字段值.

        Returns:
            字段名到值的字典, 如 {"api_key": "sk-xxx", "api_base_url": "https://..."}.
        """
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})
        return {key: widget.text() for key, widget in fields.items()}

    def set_field_values(self, values: dict[str, str]) -> None:
        """设置当前活动页面的字段值.

        对于当前页面中存在的字段, 从 values 字典中取值填充.
        不存在于当前页面的键会被忽略.

        同时将 api_key 值同步到所有页面, 确保切换协议时密钥不丢失.

        Args:
            values: 字段名到值的字典.
        """
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})

        for key, widget in fields.items():
            if key in values:
                widget.setText(values[key])

        # 将 api_key 同步到所有页面
        if "api_key" in values:
            self._sync_api_key_to_all(values["api_key"])

    # ------------------------------------------------------------------
    # 内部方法
    # ------------------------------------------------------------------

    def _get_current_protocol(self) -> str:
        """获取当前活动页面对应的协议类型字符串.

        Returns:
            当前协议类型字符串.
        """
        current_index = self.currentIndex()
        for protocol, index in _PROTOCOL_INDEX_MAP.items():
            if index == current_index:
                return protocol
        return "openai"

    def _get_current_api_key(self) -> str:
        """获取当前活动页面的 API 密钥值.

        Returns:
            当前 API 密钥文本, 若无则返回空字符串.
        """
        current_protocol = self._get_current_protocol()
        fields = self._pages.get(current_protocol, {})
        api_key_widget = fields.get("api_key")
        if api_key_widget is not None:
            return api_key_widget.text()
        return ""

    def _sync_api_key_to_all(self, api_key: str) -> None:
        """将 API 密钥值同步到所有页面的 api_key 字段.

        Args:
            api_key: 要同步的 API 密钥值.
        """
        for fields in self._pages.values():
            api_key_widget = fields.get("api_key")
            if api_key_widget is not None:
                api_key_widget.setText(api_key)

    def connect_field_text_changed(self, slot) -> None:
        """将所有页面中所有字段的 textChanged 信号连接到指定槽函数.

        用于外部组件监听字段变更以实时更新 URL 预览和按钮状态.

        Args:
            slot: 要连接的槽函数（无参数或接受 str 参数）.
        """
        for fields in self._pages.values():
            for widget in fields.values():
                widget.textChanged.connect(slot)
