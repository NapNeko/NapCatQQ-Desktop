# -*- coding: utf-8 -*-
"""供应商列表条目组件.

自定义列表条目控件, 替代纯文本 QListWidgetItem, 展示:
[Avatar 20×20] [Name (elided)] [ON Label]
"""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QFontMetrics, QPixmap
from PySide6.QtWidgets import QHBoxLayout, QLabel, QWidget

from src.core.agent.provider import Provider


class ProviderListItemWidget(QWidget):
    """供应商列表条目 - 图标 + 名称 + ON 标签.

    布局: [Avatar 20×20] [Name (elided)] [ON Label]
    通过 QListWidget 自身的 InternalMove 拖拽能力实现排序,
    不再额外渲染拖拽手柄, 简化视觉.
    """

    def __init__(self, provider: Provider, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._provider = provider
        self._setup_ui()
        self._populate(provider)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建条目 UI 布局."""
        layout = QHBoxLayout(self)
        # 左右各 8px 内边距, 上下 0 让 widget 填满 item 高度, 由 layout 自动居中子控件
        layout.setContentsMargins(8, 0, 8, 0)
        layout.setSpacing(8)

        # Avatar - 20×20 像素
        self._avatar_label = QLabel(self)
        self._avatar_label.setFixedSize(20, 20)
        layout.addWidget(self._avatar_label, 0, Qt.AlignmentFlag.AlignVCenter)

        # Name - 省略号截断, 占据剩余空间
        self._name_label = _ElidedLabel(self)
        self._name_label.setMinimumWidth(20)
        layout.addWidget(self._name_label, 1, Qt.AlignmentFlag.AlignVCenter)

        # ON Label - 绿色 "ON" 文本
        self._on_label = QLabel("ON", self)
        self._on_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        on_font = self._on_label.font()
        on_font.setPointSize(8)
        on_font.setBold(True)
        self._on_label.setFont(on_font)
        self._on_label.setStyleSheet("QLabel { color: #4CAF50; padding: 1px 3px; }")
        layout.addWidget(self._on_label, 0, Qt.AlignmentFlag.AlignVCenter)

    # ------------------------------------------------------------------
    # 数据填充
    # ------------------------------------------------------------------

    def _populate(self, provider: Provider) -> None:
        """根据 Provider 数据填充各子控件.

        Args:
            provider: 供应商数据实例.
        """
        # Avatar: 尝试使用注册图标, 否则使用首字母圆形头像
        avatar_pixmap = self._get_provider_icon(provider)
        self._avatar_label.setPixmap(avatar_pixmap)

        # Name
        self._name_label.set_text(provider.name)

        # ON Label
        self._on_label.setVisible(provider.enabled)

    def _get_provider_icon(self, provider: Provider) -> QPixmap:
        """获取供应商图标.

        通过 ProviderIcon 从 Qt 资源系统加载图标.
        若未命中则自动回退到首字母圆形头像.

        Args:
            provider: 供应商数据实例.

        Returns:
            20×20 的 QPixmap 图标.
        """
        from src.ui.common.icon import ProviderIcon

        return ProviderIcon.get_icon(provider.provider_id, provider.name)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def update_enabled_state(self, enabled: bool) -> None:
        """更新 ON 标签显示状态.

        Args:
            enabled: True 时显示绿色 "ON" 标签, False 时隐藏.
        """
        self._on_label.setVisible(enabled)


class _ElidedLabel(QLabel):
    """支持省略号截断的 QLabel.

    当文本超出可用宽度时, 自动以省略号截断显示.
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._full_text = ""

    def set_text(self, text: str) -> None:
        """设置完整文本, 显示时自动截断.

        Args:
            text: 完整的文本内容.
        """
        self._full_text = text
        self._update_elided_text()

    def resizeEvent(self, event) -> None:
        """窗口大小变化时重新计算省略号截断."""
        super().resizeEvent(event)
        self._update_elided_text()

    def _update_elided_text(self) -> None:
        """根据当前宽度计算并设置省略号截断文本."""
        metrics = QFontMetrics(self.font())
        elided = metrics.elidedText(
            self._full_text, Qt.TextElideMode.ElideRight, self.width()
        )
        self.setText(elided)
