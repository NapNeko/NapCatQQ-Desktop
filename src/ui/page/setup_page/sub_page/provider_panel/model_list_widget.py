# -*- coding: utf-8 -*-
"""模型列表管理组件.

展示供应商的模型列表, 支持分组折叠, 删除模型, 健康度检查, 获取模型列表
和手动添加. 顶部标题栏布局::

    [模型 标签] [数量徽标] [健康度检查] [- 拉伸 -] [获取模型列表 | +]

中部按 model_id 解析的组名分组渲染, 每个分组是一张完整的圆角卡片,
包含 (header chevron + 组名) 与多个模型行. 模型行布局::

    [Avatar] [model_id (mono)] [- 拉伸 -] [能力徽章 ×4] [设置] [删除]

不再通过 HeaderCardWidget 包裹, ModelListWidget 自身即为视觉单元.
"""
from __future__ import annotations

from collections import OrderedDict

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QColor, QFont, QMouseEvent, QPainter, QPixmap
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    CaptionLabel,
    FluentIcon,
    MessageBox,
    SplitPushButton,
    StrongBodyLabel,
    TransparentToolButton,
    isDarkTheme,
)

from src.core.agent.model_fetch_service import FetchResult
from src.core.agent.provider import ModelEntry, Provider
from src.core.config import cfg
from src.ui.common.style_sheet import WidgetStyleSheet
from src.ui.components.info_bar import error_bar, warning_bar

from .add_model_dialog import AddModelDialog
from .manage_models_dialog import _ModelFetchThread, ManageModelsDialog


# ----------------------------------------------------------------------
# 工具函数
# ----------------------------------------------------------------------


def _parse_group_name(model_id: str) -> str:
    """从 model_id 解析模型族/分组名.

    剥离尾部版本相关段 (纯数字段或以数字开头的版本段). 示例::

        claude-haiku-4-5-20251001 -> claude-haiku
        claude-opus-4-6           -> claude-opus
        gpt-4o                    -> gpt
        deepseek-chat             -> deepseek-chat
        gemini-2.0-pro            -> gemini

    Args:
        model_id: 模型 ID.

    Returns:
        分组名. 若无可保留前缀则回退到完整 model_id.
    """
    parts = model_id.split("-")
    prefix: list[str] = []
    for part in parts:
        if not part:
            continue
        if part.isdigit():
            break
        if part[0].isdigit():
            break
        prefix.append(part)
    return "-".join(prefix) if prefix else model_id


def _detect_capability(model_id: str, capability: str, model: ModelEntry) -> bool:
    """根据 ModelEntry 显式字段判断模型能力.

    优先使用 ModelEntry 上的 supports_* 字段, 不再依赖 model_id 关键字推断.

    Args:
        model_id: 模型 ID (保留参数兼容性).
        capability: 能力类型 (vision / web / reasoning / tools).
        model: ModelEntry 实例.

    Returns:
        True 表示模型支持该能力.
    """
    if capability == "vision":
        return bool(model.supports_vision)
    if capability == "web":
        return bool(model.supports_web) or bool(model.supports_streaming)
    if capability == "reasoning":
        return bool(model.supports_reasoning)
    if capability == "tools":
        return bool(model.supports_tools)
    return False


# ----------------------------------------------------------------------
# 子组件: 能力徽章
# ----------------------------------------------------------------------


class _CapabilityBadge(QLabel):
    """能力徽章 - 22x18 圆角小图标, 浅色背景表示能力支持.

    当 ``supported`` 为 True 时使用品类强调色, 否则呈灰度禁用态.
    """

    _PALETTE: dict[str, tuple[str, str]] = {
        "vision": ("rgba(76, 175, 80, 0.15)", "rgba(76, 175, 80, 0.22)"),
        "web": ("rgba(33, 150, 243, 0.15)", "rgba(33, 150, 243, 0.22)"),
        "reasoning": ("rgba(156, 39, 176, 0.15)", "rgba(156, 39, 176, 0.25)"),
        "tools": ("rgba(255, 152, 0, 0.18)", "rgba(255, 152, 0, 0.25)"),
    }

    _ICON_COLOR: dict[str, str] = {
        "vision": "#2E7D32",
        "web": "#1565C0",
        "reasoning": "#7B1FA2",
        "tools": "#E65100",
    }

    _ICON: dict[str, FluentIcon] = {
        "vision": FluentIcon.VIEW,
        "web": FluentIcon.GLOBE,
        "reasoning": FluentIcon.BRIGHTNESS,
        "tools": FluentIcon.DEVELOPER_TOOLS,
    }

    _TOOLTIP: dict[str, str] = {
        "vision": "Vision",
        "web": "Streaming",
        "reasoning": "Reasoning",
        "tools": "Tools",
    }

    def __init__(self, capability: str, supported: bool, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._capability = capability
        self._supported = supported
        self.setFixedSize(22, 18)
        self.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.setToolTip(self._TOOLTIP.get(capability, ""))
        self._render_icon()
        self._apply_style()
        cfg.themeChanged.connect(self._apply_style)

    def _render_icon(self) -> None:
        icon = self._ICON.get(self._capability)
        if icon is None:
            return
        size = 11
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        try:
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            color = QColor(self._ICON_COLOR[self._capability]) if self._supported else QColor("#9E9E9E")
            qicon = icon.icon(color=color)
            qicon.paint(painter, 0, 0, size, size)
        finally:
            painter.end()
        self.setPixmap(pixmap)

    def _apply_style(self, *_args) -> None:
        bg_light, bg_dark = self._PALETTE.get(
            self._capability,
            ("rgba(128, 128, 128, 0.15)", "rgba(128, 128, 128, 0.22)"),
        )
        bg = bg_dark if isDarkTheme() else bg_light
        if not self._supported:
            bg = "rgba(128, 128, 128, 0.18)" if isDarkTheme() else "rgba(0, 0, 0, 0.06)"
        self.setStyleSheet(
            f"QLabel {{ background: {bg}; border-radius: 5px; padding: 0; }}"
        )


# ----------------------------------------------------------------------
# 子组件: 模型行
# ----------------------------------------------------------------------


class _ModelRowWidget(QWidget):
    """单个模型行 - 头像 + 名称 + 能力徽章 + 设置 + 删除.

    行本身不再绘制独立卡片 (背景与边框都不画), 直接落在所属
    ``_ModelGroupSection`` 卡片内部, 以避免双重视觉边界. 行间通过
    section 内 layout spacing 自然分隔.
    """

    settings_clicked = Signal(str)
    delete_clicked = Signal(str)

    def __init__(self, model: ModelEntry, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._model = model
        self.setObjectName("modelRowWidget")
        self.setProperty("model_id", model.model_id)
        self.setProperty("display_name", model.display_name)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 6, 12, 6)
        layout.setSpacing(10)

        # 头像
        self._avatar = QLabel(self)
        self._avatar.setFixedSize(22, 22)
        self._avatar.setPixmap(self._build_avatar(self._model.model_id))
        layout.addWidget(self._avatar)

        # 模型名 (mono)
        self._name_label = QLabel(self._model.model_id, self)
        mono_font = QFont(self._name_label.font())
        mono_font.setStyleHint(QFont.StyleHint.Monospace)
        mono_font.setFamily("Consolas")
        self._name_label.setFont(mono_font)
        self._name_label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        if self._model.display_name and self._model.display_name != self._model.model_id:
            self._name_label.setToolTip(self._model.display_name)
        else:
            self._name_label.setToolTip(self._model.model_id)
        layout.addWidget(self._name_label, 1)

        # 能力徽章
        self._badge_container = QWidget(self)
        badge_layout = QHBoxLayout(self._badge_container)
        badge_layout.setContentsMargins(0, 0, 0, 0)
        badge_layout.setSpacing(4)
        for cap in ("vision", "web", "reasoning", "tools"):
            badge_layout.addWidget(
                _CapabilityBadge(
                    cap,
                    _detect_capability(self._model.model_id, cap, self._model),
                    self,
                )
            )
        layout.addWidget(self._badge_container)

        layout.addSpacing(4)

        # 设置按钮
        self._settings_btn = TransparentToolButton(FluentIcon.SETTING, self)
        self._settings_btn.setFixedSize(26, 26)
        self._settings_btn.setToolTip(self.tr("查看模型参数"))
        self._settings_btn.clicked.connect(
            lambda: self.settings_clicked.emit(self._model.model_id)
        )
        layout.addWidget(self._settings_btn)

        # 删除按钮
        self._delete_btn = TransparentToolButton(FluentIcon.REMOVE, self)
        self._delete_btn.setFixedSize(26, 26)
        self._delete_btn.setToolTip(self.tr("移除模型"))
        self._delete_btn.clicked.connect(
            lambda: self.delete_clicked.emit(self._model.model_id)
        )
        layout.addWidget(self._delete_btn)

    @staticmethod
    def _build_avatar(seed: str) -> QPixmap:
        """生成 22×22 圆形首字母头像, 颜色由 seed 哈希决定."""
        size = 22
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        try:
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            hue = hash(seed) % 360
            painter.setBrush(QColor.fromHsv(hue, 130, 220))
            painter.setPen(Qt.PenStyle.NoPen)
            painter.drawEllipse(0, 0, size, size)

            initial = seed[0].upper() if seed else "?"
            painter.setPen(QColor(255, 255, 255))
            font = QFont()
            font.setPointSize(9)
            font.setBold(True)
            painter.setFont(font)
            painter.drawText(pixmap.rect(), Qt.AlignmentFlag.AlignCenter, initial)
        finally:
            painter.end()
        return pixmap


# ----------------------------------------------------------------------
# 子组件: 分组 Section
# ----------------------------------------------------------------------


class _GroupHeader(QWidget):
    """分组卡片的可点击标题条 - 整条响应点击, 通过鼠标释放发射 ``clicked``."""

    clicked = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setMinimumHeight(28)

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:  # noqa: N802 - Qt 命名固定
        if event.button() == Qt.MouseButton.LeftButton and self.rect().contains(event.position().toPoint()):
            self.clicked.emit()
        super().mouseReleaseEvent(event)


class _ModelGroupSection(QFrame):
    """可折叠模型分组 - 单一圆角卡片, 包含 header (chevron + 组名) + 行体.

    整个 section 是一张完整卡片, 内部 header 与 body 共享同一背景.
    点击 chevron 按钮触发折叠. 不再在 ``QFrame.mouseReleaseEvent``
    上做事件覆盖 (那样和 IconWidget 的真实 API 冲突), 而是用一个
    透明工具按钮承担点击动作.
    """

    def __init__(self, group_name: str, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._group_name = group_name
        self._collapsed = False
        self.setObjectName("modelGroupSection")
        self._setup_ui()
        cfg.themeChanged.connect(self._apply_style)
        self._apply_style()

    def _setup_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 8, 0, 8)
        outer.setSpacing(0)

        # 标题行 - 整条可点击, chevron 仅作视觉指示
        self._header = _GroupHeader(self)
        header_layout = QHBoxLayout(self._header)
        header_layout.setContentsMargins(12, 2, 12, 2)
        header_layout.setSpacing(8)

        self._chevron = QLabel(self._header)
        self._chevron.setFixedSize(12, 12)
        self._chevron.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._render_chevron()
        header_layout.addWidget(self._chevron)

        self._title = StrongBodyLabel(self._group_name, self._header)
        title_font = QFont(self._title.font())
        title_font.setStyleHint(QFont.StyleHint.Monospace)
        title_font.setFamily("Consolas")
        self._title.setFont(title_font)
        header_layout.addWidget(self._title, 1)

        self._header.clicked.connect(self._toggle_collapsed)
        outer.addWidget(self._header)

        # 头部与行体之间的分割线 - 横向贴边
        self._divider = QFrame(self)
        self._divider.setObjectName("modelGroupDivider")
        self._divider.setFrameShape(QFrame.Shape.NoFrame)
        self._divider.setFixedHeight(1)
        outer.addSpacing(6)
        outer.addWidget(self._divider)
        outer.addSpacing(6)

        # 行体 - 带水平内边距
        self._body = QWidget(self)
        self._body_layout = QVBoxLayout(self._body)
        self._body_layout.setContentsMargins(8, 0, 8, 0)
        self._body_layout.setSpacing(2)
        outer.addWidget(self._body)

    def _render_chevron(self) -> None:
        """根据折叠状态绘制 12×12 chevron 图标."""
        size = 12
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        try:
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            color = QColor("#FFFFFF") if isDarkTheme() else QColor("#1F1F1F")
            color.setAlphaF(0.7)
            icon = (
                FluentIcon.CHEVRON_RIGHT_MED
                if self._collapsed
                else FluentIcon.CHEVRON_DOWN_MED
            )
            qicon = icon.icon(color=color)
            qicon.paint(painter, 0, 0, size, size)
        finally:
            painter.end()
        self._chevron.setPixmap(pixmap)

    def _toggle_collapsed(self) -> None:
        """切换折叠状态, 同时更新 chevron 与分割线显隐."""
        self._collapsed = not self._collapsed
        self._body.setVisible(not self._collapsed)
        self._divider.setVisible(not self._collapsed)
        self._render_chevron()

    def _apply_style(self, *_args) -> None:
        if isDarkTheme():
            border = "rgba(255, 255, 255, 0.08)"
            bg = "rgba(255, 255, 255, 0.04)"
            divider = "rgba(255, 255, 255, 0.08)"
        else:
            border = "rgba(0, 0, 0, 0.08)"
            bg = "rgba(0, 0, 0, 0.025)"
            divider = "rgba(0, 0, 0, 0.08)"
        self.setStyleSheet(
            f"#modelGroupSection {{"
            f"  background: {bg};"
            f"  border: 0.5px solid {border};"
            f"  border-radius: 8px;"
            f"}}"
        )
        self._divider.setStyleSheet(
            f"QFrame#modelGroupDivider {{ background-color: {divider}; border: none; }}"
        )
        # 主题切换后 chevron 颜色随之刷新
        self._render_chevron()

    def add_row(self, row: _ModelRowWidget) -> None:
        self._body_layout.addWidget(row)


# ----------------------------------------------------------------------
# 主组件: ModelListWidget
# ----------------------------------------------------------------------


class ModelListWidget(QWidget):
    """模型列表组件 - 标题栏 + 分组列表.

    顶部布局::

        [模型] [数量徽标] [健康度]   -- 拉伸 --   [获取模型列表 | +]

    中部按 model_id 解析的组名分组渲染. ``SplitPushButton`` 的 ``clicked``
    (左侧主按钮) 触发 "获取模型列表" 占位流程, ``dropDownClicked``
    (右侧 +) 触发 ``AddModelDialog``.
    """

    model_added = Signal(str)
    model_removed = Signal(str)
    fetch_requested = Signal()  # 获取模型列表前发射, 通知父面板先持久化配置

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._provider: Provider | None = None
        self._row_widgets: list[_ModelRowWidget] = []
        self._group_widgets: list[_ModelGroupSection] = []
        self._setup_ui()
        self._connect_signals()
        WidgetStyleSheet.MODEL_LIST_WIDGET.apply(self)

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        self._layout = QVBoxLayout(self)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._layout.setSpacing(10)

        # 顶部标题栏
        header = QHBoxLayout()
        header.setContentsMargins(0, 0, 0, 0)
        header.setSpacing(8)

        self._title_label = StrongBodyLabel(self.tr("模型"), self)
        header.addWidget(self._title_label)

        # 数量徽标 - 极小圆角 badge
        self._count_badge = CaptionLabel("0", self)
        self._count_badge.setObjectName("modelCountBadge")
        self._count_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._count_badge.setFixedHeight(16)
        font = self._count_badge.font()
        font.setPointSize(8)
        self._count_badge.setFont(font)
        header.addWidget(self._count_badge)

        # 健康度检查
        self._health_btn = TransparentToolButton(FluentIcon.HEART, self)
        self._health_btn.setFixedSize(28, 28)
        self._health_btn.setToolTip(self.tr("模型健康度检查"))
        header.addWidget(self._health_btn)

        header.addStretch(1)

        # 分割按钮 -- 左 "获取模型列表", 右 "+" 添加 (透明低调风格)
        self._split_btn = SplitPushButton(
            FluentIcon.SYNC, self.tr("获取模型列表"), self
        )
        self._split_btn.setDropIcon(FluentIcon.ADD)
        self._split_btn.dropButton.setToolTip(self.tr("手动添加模型"))
        self._split_btn.button.setToolTip(self.tr("从供应商拉取模型列表"))
        self._apply_split_btn_style()
        cfg.themeChanged.connect(self._apply_split_btn_style)
        header.addWidget(self._split_btn)

        self._layout.addLayout(header)

        # 分组列表容器
        self._list_container = QWidget(self)
        self._list_layout = QVBoxLayout(self._list_container)
        self._list_layout.setContentsMargins(0, 0, 0, 0)
        self._list_layout.setSpacing(10)
        self._layout.addWidget(self._list_container)

    def _connect_signals(self) -> None:
        self._health_btn.clicked.connect(self._on_health_check_clicked)
        self._split_btn.clicked.connect(self._on_fetch_models_clicked)
        self._split_btn.dropDownClicked.connect(self._on_add_model_clicked)

    def _apply_split_btn_style(self, *_args) -> None:
        """为 SplitPushButton 应用透明低调风格, 去除默认边框和背景."""
        if isDarkTheme():
            normal_bg = "rgba(255, 255, 255, 0.0419)"
            hover_bg = "rgba(255, 255, 255, 0.0698)"
            pressed_bg = "rgba(255, 255, 255, 0.0326)"
            border = "rgba(255, 255, 255, 0.053)"
            text_color = "white"
        else:
            normal_bg = "rgba(0, 0, 0, 0.03)"
            hover_bg = "rgba(0, 0, 0, 0.06)"
            pressed_bg = "rgba(0, 0, 0, 0.03)"
            border = "rgba(0, 0, 0, 0.06)"
            text_color = "black"

        btn_qss = (
            f"#splitPushButton {{"
            f"  background: {normal_bg};"
            f"  border: 1px solid {border};"
            f"  border-top-left-radius: 5px;"
            f"  border-bottom-left-radius: 5px;"
            f"  border-top-right-radius: 0;"
            f"  border-bottom-right-radius: 0;"
            f"  color: {text_color};"
            f"  padding: 5px 12px 5px 36px;"
            f"}}"
            f"#splitPushButton:hover {{"
            f"  background: {hover_bg};"
            f"}}"
            f"#splitPushButton:pressed {{"
            f"  background: {pressed_bg};"
            f"}}"
        )
        drop_qss = (
            f"SplitDropButton {{"
            f"  background: {normal_bg};"
            f"  border: 1px solid {border};"
            f"  border-left: none;"
            f"  border-top-right-radius: 5px;"
            f"  border-bottom-right-radius: 5px;"
            f"  border-top-left-radius: 0;"
            f"  border-bottom-left-radius: 0;"
            f"  padding: 0px 4px;"
            f"  min-width: 24px;"
            f"  max-width: 24px;"
            f"}}"
            f"SplitDropButton:hover {{"
            f"  background: {hover_bg};"
            f"}}"
            f"SplitDropButton:pressed {{"
            f"  background: {pressed_bg};"
            f"}}"
        )
        self._split_btn.button.setStyleSheet(btn_qss)
        self._split_btn.dropButton.setStyleSheet(drop_qss)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def set_provider(self, provider: Provider) -> None:
        """加载供应商的模型列表."""
        self._provider = provider
        self._rebuild_list()

    def update_provider_ref(self, provider: Provider) -> None:
        """仅更新内部 provider 引用, 不重建列表 UI.

        用于在获取模型列表前同步最新的 provider 配置 (如 api_key_ref 变更),
        避免不必要的 UI 重建.
        """
        self._provider = provider

    def refresh(self) -> None:
        if self._provider is not None:
            self._rebuild_list()

    # ------------------------------------------------------------------
    # 内部方法
    # ------------------------------------------------------------------

    def _clear_list(self) -> None:
        for row in self._row_widgets:
            row.setParent(None)
            row.deleteLater()
        self._row_widgets.clear()

        for group in self._group_widgets:
            group.setParent(None)
            group.deleteLater()
        self._group_widgets.clear()

    def _rebuild_list(self) -> None:
        self._clear_list()

        if self._provider is None:
            self._count_badge.setText("0")
            return

        self._count_badge.setText(str(len(self._provider.models)))

        grouped: "OrderedDict[str, list[ModelEntry]]" = OrderedDict()
        for model in self._provider.models:
            group_name = _parse_group_name(model.model_id)
            grouped.setdefault(group_name, []).append(model)

        for group_name, entries in grouped.items():
            section = _ModelGroupSection(group_name, self._list_container)
            self._group_widgets.append(section)
            for entry in entries:
                row = _ModelRowWidget(entry, section)
                row.delete_clicked.connect(self._on_remove_model)
                row.settings_clicked.connect(self._on_settings_clicked)
                section.add_row(row)
                self._row_widgets.append(row)
            self._list_layout.addWidget(section)

    # ------------------------------------------------------------------
    # 槽函数
    # ------------------------------------------------------------------

    def _on_health_check_clicked(self) -> None:
        """点击健康度检查 - 弹出 MessageBox 展示概要信息."""
        if self._provider is None:
            return

        if not self._provider.models:
            content = self.tr("当前供应商暂无模型.")
        else:
            lines: list[str] = []
            lines.append(self.tr("供应商: {}").format(self._provider.name))
            lines.append(self.tr("模型总数: {}").format(len(self._provider.models)))
            lines.append("")
            for model in self._provider.models:
                caps = []
                if _detect_capability(model.model_id, "vision", model):
                    caps.append("Vision")
                if _detect_capability(model.model_id, "tools", model):
                    caps.append("Tools")
                if _detect_capability(model.model_id, "reasoning", model):
                    caps.append("Reasoning")
                if _detect_capability(model.model_id, "web", model):
                    caps.append("Streaming")
                cap_text = ", ".join(caps) if caps else self.tr("基础")
                lines.append(f"• {model.model_id} — {cap_text}")
            content = "\n".join(lines)

        dialog = MessageBox(self.tr("模型健康度检查"), content, self.window())
        dialog.cancelButton.setVisible(False)
        dialog.exec()

    def _on_fetch_models_clicked(self) -> None:
        """获取模型列表 — 调用 ModelFetchService 拉取模型, 成功后打开 ManageModelsDialog."""
        if self._provider is None:
            return

        # 通知父面板先持久化配置, 确保 provider 数据是最新的
        self.fetch_requested.emit()

        # 进入加载状态: 禁用按钮 + 更改文本
        self._set_fetch_loading(True)

        # 启动后台获取线程
        self._fetch_thread = _ModelFetchThread(self._provider, self)
        self._fetch_thread.fetch_finished.connect(self._on_fetch_finished)
        self._fetch_thread.start()

    def _on_fetch_finished(self, result: FetchResult) -> None:
        """模型获取完成回调 — 处理成功/失败结果.

        Args:
            result: ModelFetchService 返回的 FetchResult.
        """
        # 恢复按钮状态
        self._set_fetch_loading(False)

        if not result.success:
            # 失败: 显示错误 InfoBar
            error_bar(
                content=result.error_message,
                title=self.tr("获取模型列表失败"),
                duration=5000,
                parent=self,
            )
            return

        # 成功: 打开 ManageModelsDialog
        if self._provider is None:
            return

        dialog = ManageModelsDialog(
            parent=self.window(),
            fetched_models=result.models,
            current_models=list(self._provider.models),
            provider_id=self._provider.provider_id,
        )
        # 当弹窗内模型列表变更时, 刷新本组件的展示
        dialog.model_list_changed.connect(self._on_manage_dialog_changed)
        dialog.exec()

    def _on_manage_dialog_changed(self) -> None:
        """ManageModelsDialog 的 model_list_changed 信号处理 -- 刷新模型列表展示.

        ManageModelsDialog 直接通过 ProviderRegistry 增删模型,
        需要从 registry 重新获取最新的 provider 对象再重建列表.
        """
        if self._provider is None:
            return

        from creart import it
        from src.core.agent.provider import ProviderRegistry

        registry = it(ProviderRegistry)
        try:
            self._provider = registry.get(self._provider.provider_id)
        except KeyError:
            pass
        self._rebuild_list()

    def _set_fetch_loading(self, loading: bool) -> None:
        """设置/取消获取模型列表的加载状态.

        Args:
            loading: True 进入加载状态, False 退出加载状态.
        """
        self._split_btn.setEnabled(not loading)
        if loading:
            self._split_btn.button.setText(self.tr("获取中..."))
        else:
            self._split_btn.button.setText(self.tr("获取模型列表"))

    def _on_add_model_clicked(self) -> None:
        """点击 SplitPushButton 右侧 +, 弹出 AddModelDialog."""
        if self._provider is None:
            return
        dialog = AddModelDialog(self.window())
        if dialog.exec():
            data = dialog.get_data()
            new_model = ModelEntry(
                model_id=data["model_id"],
                display_name=data["display_name"],
                max_tokens=data["max_tokens"],
            )
            self._provider.models.append(new_model)
            self.model_added.emit(data["model_id"])
            self._rebuild_list()

    def _on_settings_clicked(self, model_id: str) -> None:
        """点击设置按钮 - 弹出 EditModelDialog 编辑模型配置."""
        if self._provider is None:
            return
        target: ModelEntry | None = next(
            (m for m in self._provider.models if m.model_id == model_id), None
        )
        if target is None:
            return

        from .edit_model_dialog import EditModelDialog

        dialog = EditModelDialog(target, self.window())
        dialog.saved.connect(lambda updated: self._apply_model_update(model_id, updated))
        dialog.exec()

    def _apply_model_update(self, model_id: str, updated: ModelEntry) -> None:
        """将 EditModelDialog 保存的 ModelEntry 写回 provider.models 列表."""
        if self._provider is None:
            return
        self._provider.models = [
            updated if m.model_id == model_id else m for m in self._provider.models
        ]
        self.model_added.emit(model_id)  # 复用信号通知外部持久化
        self._rebuild_list()

    def _on_remove_model(self, model_id: str) -> None:
        if self._provider is None:
            return
        if len(self._provider.models) <= 1:
            warning_bar(
                content=self.tr("至少保留一个模型"),
                title="",
                duration=3000,
                parent=self,
            )
            return
        self._provider.models = [
            m for m in self._provider.models if m.model_id != model_id
        ]
        self.model_removed.emit(model_id)
        self._rebuild_list()
