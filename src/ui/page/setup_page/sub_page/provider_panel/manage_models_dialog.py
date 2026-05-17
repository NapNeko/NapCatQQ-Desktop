# -*- coding: utf-8 -*-
"""模型管理弹窗.

展示从供应商 API 获取到的模型列表, 支持搜索过滤, 能力筛选标签页,
按 group_name 分组折叠展示. 用户可逐个添加/移除模型或批量添加.

UI 结构 (参考图二设计)::

    +---------------------------------------------+
    | {供应商名}模型                          [x]  |
    +---------------------------------------------+
    | [Q 搜索模型 ID 或名称]          [筛选] [刷新] |
    | [全部] [推理] [视觉] [联网] [嵌入] [重排] [工具] |
    +---------------------------------------------+
    | v DeepSeek Chat  (1)                   [+]  |
    |   [avatar] DeepSeek Chat    [设置][编辑][+]  |
    | v deepseek  (2)                        [-]  |
    |   [avatar] deepseek-v4-flash [设置][编辑][-] |
    |   [avatar] deepseek-v4-pro   [设置][编辑][-] |
    +---------------------------------------------+
"""
from __future__ import annotations

import asyncio
from collections import OrderedDict

from creart import it
from PySide6.QtCore import QThread, QTimer, Qt, Signal
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
    BodyLabel,
    FluentIcon,
    MessageBoxBase,
    PillPushButton,
    PrimaryPushButton,
    ScrollArea,
    SearchLineEdit,
    StrongBodyLabel,
    SubtitleLabel,
    TransparentPushButton,
    TransparentToolButton,
    isDarkTheme,
)

from src.core.logging import LogSource, logger

from src.core.agent.model_fetch_service import FetchResult, ModelFetchService
from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry
from src.core.config import cfg
from src.ui.components.info_bar import error_bar, success_bar


# 能力筛选标签页映射: 标签名 -> ModelEntry 字段名 (None 表示"全部")
_CAPABILITY_TABS: list[tuple[str, str | None]] = [
    ("全部", None),
    ("推理", "supports_reasoning"),
    ("视觉", "supports_vision"),
    ("工具调用", "supports_tools"),
    ("嵌入", "supports_embedding"),
    ("重排", "supports_rerank"),
]


class _ModelFetchThread(QThread):
    """后台线程 -- 运行 ModelFetchService.fetch_models 异步方法.

    使用独立的 asyncio event loop 在 QThread 中执行异步获取,
    完成后通过信号将 FetchResult 回传主线程.

    Signals:
        fetch_finished: 获取完成时发射, 携带 FetchResult.
    """

    fetch_finished = Signal(object)  # FetchResult

    def __init__(self, provider: Provider, parent=None) -> None:
        super().__init__(parent)
        self._provider = provider

    def run(self) -> None:
        """在后台线程中运行 asyncio event loop 执行模型获取."""
        service = ModelFetchService()
        loop = asyncio.new_event_loop()
        try:
            result = loop.run_until_complete(service.fetch_models(self._provider))
        except Exception as exc:
            logger.exception(f"模型获取线程异常: {exc}", exc)
            result = FetchResult(
                success=False,
                error_message=f"获取异常: {exc}",
            )
        finally:
            loop.close()
        self.fetch_finished.emit(result)


# ----------------------------------------------------------------------
# 子组件: 可点击的分组标题条
# ----------------------------------------------------------------------


class _DialogGroupHeader(QWidget):
    """分组卡片的可点击标题条 - 整条响应点击."""

    clicked = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setMinimumHeight(32)

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        if event.button() == Qt.MouseButton.LeftButton and self.rect().contains(
            event.position().toPoint()
        ):
            self.clicked.emit()
        super().mouseReleaseEvent(event)


# ----------------------------------------------------------------------
# 子组件: 分组 Section (可折叠卡片)
# ----------------------------------------------------------------------


class _DialogGroupSection(QFrame):
    """可折叠模型分组 - 无卡片背景, 组间通过顶部分隔线区分.

    header: chevron + 组名(粗体) + 绿色数量徽标 + 右侧 "+" 按钮.
    参考 Cherry Studio 的分组设计.
    """

    add_all_in_group = Signal(str)  # 组名

    def __init__(self, group_name: str, count: int, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)
        self._group_name = group_name
        self._count = count
        self._collapsed = False
        self.setObjectName("dialogGroupSection")
        self._setup_ui()
        cfg.themeChanged.connect(self._apply_style)
        self._apply_style()

    def _setup_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)

        # 顶部分隔线
        self._divider = QFrame(self)
        self._divider.setObjectName("dialogGroupDivider")
        self._divider.setFrameShape(QFrame.Shape.NoFrame)
        self._divider.setFixedHeight(1)
        outer.addWidget(self._divider)
        outer.addSpacing(8)

        # 标题行
        self._header = _DialogGroupHeader(self)
        header_layout = QHBoxLayout(self._header)
        header_layout.setContentsMargins(8, 4, 8, 4)
        header_layout.setSpacing(8)

        # Chevron
        self._chevron = QLabel(self._header)
        self._chevron.setFixedSize(12, 12)
        self._chevron.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._render_chevron()
        header_layout.addWidget(self._chevron)

        # 组名
        self._title = StrongBodyLabel(self._group_name, self._header)
        title_font = QFont(self._title.font())
        title_font.setStyleHint(QFont.StyleHint.Monospace)
        title_font.setFamily("Consolas")
        self._title.setFont(title_font)
        header_layout.addWidget(self._title)

        # 数量徽标 - 绿色背景白字圆角
        self._count_badge = QLabel(str(self._count), self._header)
        self._count_badge.setObjectName("dialogGroupCountBadge")
        self._count_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._count_badge.setFixedHeight(18)
        self._count_badge.setMinimumWidth(18)
        badge_font = QFont(self._count_badge.font())
        badge_font.setPointSize(9)
        badge_font.setBold(True)
        self._count_badge.setFont(badge_font)
        header_layout.addWidget(self._count_badge)

        header_layout.addStretch(1)

        # 组级 +/- 按钮 - 纯文本样式
        self._group_action_btn = QLabel("+", self._header)
        self._group_action_btn.setObjectName("dialogGroupActionBtn")
        self._group_action_btn.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._group_action_btn.setFixedSize(24, 24)
        self._group_action_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        action_font = QFont(self._group_action_btn.font())
        action_font.setPointSize(14)
        action_font.setBold(True)
        self._group_action_btn.setFont(action_font)
        self._group_action_btn.mousePressEvent = lambda e: self.add_all_in_group.emit(self._group_name)
        header_layout.addWidget(self._group_action_btn)

        self._header.clicked.connect(self._toggle_collapsed)
        outer.addWidget(self._header)

        # 行体
        self._body = QWidget(self)
        self._body_layout = QVBoxLayout(self._body)
        self._body_layout.setContentsMargins(4, 4, 4, 4)
        self._body_layout.setSpacing(2)
        outer.addWidget(self._body)

    def _render_chevron(self) -> None:
        """根据折叠状态绘制 12x12 chevron 图标."""
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
        self._collapsed = not self._collapsed
        self._body.setVisible(not self._collapsed)
        self._render_chevron()

    def _apply_style(self, *_args) -> None:
        if isDarkTheme():
            divider_color = "rgba(255, 255, 255, 0.08)"
            action_color = "white"
        else:
            divider_color = "rgba(0, 0, 0, 0.08)"
            action_color = "#1F1F1F"
        self.setStyleSheet(
            f"#dialogGroupSection {{"
            f"  background: transparent;"
            f"  border: none;"
            f"}}"
        )
        self._divider.setStyleSheet(
            f"QFrame#dialogGroupDivider {{ background-color: {divider_color}; }}"
        )
        # 绿色徽标
        self._count_badge.setStyleSheet(
            "QLabel#dialogGroupCountBadge {"
            "  background: #4CAF50;"
            "  color: white;"
            "  border-radius: 9px;"
            "  padding: 0px 5px;"
            "}"
        )
        self._group_action_btn.setStyleSheet(
            f"QLabel#dialogGroupActionBtn {{ color: {action_color}; }}"
        )
        self._render_chevron()

    def add_row(self, row: QWidget) -> None:
        self._body_layout.addWidget(row)

    def update_group_action_icon(self, all_added: bool) -> None:
        """根据组内模型是否全部已添加, 切换按钮文本."""
        if all_added:
            self._group_action_btn.setText("\u2212")  # minus sign
            self._group_action_btn.setToolTip("移除该组全部模型")
        else:
            self._group_action_btn.setText("+")
            self._group_action_btn.setToolTip("添加该组全部模型")


# ----------------------------------------------------------------------
# 子组件: 模型行
# ----------------------------------------------------------------------


class _DialogModelRow(QWidget):
    """单个模型行 - 头像 + 名称 + 彩色徽章按钮(设置/编辑) + 添加/移除按钮.

    设置和编辑按钮使用圆形彩色背景图标徽章, 与外面 ModelListWidget 的
    _CapabilityBadge 风格一致. 已添加的模型行带有浅绿色背景高亮.
    """

    add_clicked = Signal(str)
    remove_clicked = Signal(str)

    def __init__(
        self, model: ModelEntry, is_added: bool, parent: QWidget | None = None
    ) -> None:
        super().__init__(parent=parent)
        self._model = model
        self._is_added = is_added
        self.setObjectName("dialogModelRow")
        self._setup_ui()
        cfg.themeChanged.connect(self._apply_style)
        self._apply_style()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 5, 8, 5)
        layout.setSpacing(10)

        # 头像
        self._avatar = QLabel(self)
        self._avatar.setFixedSize(36, 36)
        self._avatar.setPixmap(self._build_avatar(self._model.model_id))
        layout.addWidget(self._avatar)

        # 模型名 (mono)
        self._name_label = BodyLabel(self._model.model_id, self)
        mono_font = QFont(self._name_label.font())
        mono_font.setStyleHint(QFont.StyleHint.Monospace)
        mono_font.setFamily("Consolas")
        self._name_label.setFont(mono_font)
        self._name_label.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred
        )
        if self._model.display_name and self._model.display_name != self._model.model_id:
            self._name_label.setToolTip(self._model.display_name)
        layout.addWidget(self._name_label, 1)

        # 设置徽章 - 蓝色圆形背景
        self._settings_badge = self._create_badge(
            FluentIcon.SETTING, "#1565C0", "rgba(33, 150, 243, 0.15)"
        )
        self._settings_badge.setToolTip("模型参数")
        self._settings_badge.setCursor(Qt.CursorShape.PointingHandCursor)
        layout.addWidget(self._settings_badge)

        # 编辑徽章 - 橙色圆形背景
        self._edit_badge = self._create_badge(
            FluentIcon.EDIT, "#E65100", "rgba(255, 152, 0, 0.18)"
        )
        self._edit_badge.setToolTip("编辑模型")
        self._edit_badge.setCursor(Qt.CursorShape.PointingHandCursor)
        layout.addWidget(self._edit_badge)

        # 添加/移除按钮 - 纯文本 +/-
        self._action_label = QLabel(self)
        self._action_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._action_label.setFixedSize(24, 24)
        self._action_label.setCursor(Qt.CursorShape.PointingHandCursor)
        action_font = QFont(self._action_label.font())
        action_font.setPointSize(14)
        action_font.setBold(True)
        self._action_label.setFont(action_font)

        if self._is_added:
            self._action_label.setText("\u2212")  # minus
            self._action_label.setToolTip("移除模型")
            self._action_label.mousePressEvent = (
                lambda e: self.remove_clicked.emit(self._model.model_id)
            )
        else:
            self._action_label.setText("+")
            self._action_label.setToolTip("添加模型")
            self._action_label.mousePressEvent = (
                lambda e: self.add_clicked.emit(self._model.model_id)
            )
        layout.addWidget(self._action_label)

    def _create_badge(self, icon: FluentIcon, icon_color: str, bg_color: str) -> QLabel:
        """创建圆形彩色背景图标徽章 (26x26)."""
        badge = QLabel(self)
        badge.setFixedSize(26, 26)
        badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        badge.setStyleSheet(
            f"QLabel {{ background: {bg_color}; border-radius: 13px; }}"
        )
        # 渲染图标
        pix_size = 14
        pixmap = QPixmap(pix_size, pix_size)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        try:
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            qicon = icon.icon(color=QColor(icon_color))
            qicon.paint(painter, 0, 0, pix_size, pix_size)
        finally:
            painter.end()
        badge.setPixmap(pixmap)
        return badge

    def _apply_style(self, *_args) -> None:
        if self._is_added:
            if isDarkTheme():
                bg = "rgba(76, 175, 80, 0.12)"
            else:
                bg = "rgba(76, 175, 80, 0.08)"
        else:
            bg = "transparent"
        self.setStyleSheet(
            f"#dialogModelRow {{"
            f"  background: {bg};"
            f"  border-radius: 6px;"
            f"}}"
        )

    @staticmethod
    def _build_avatar(seed: str) -> QPixmap:
        """生成 28x28 圆形首字母头像, 颜色由 seed 哈希决定."""
        size = 28
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
            font.setPointSize(10)
            font.setBold(True)
            painter.setFont(font)
            painter.drawText(pixmap.rect(), Qt.AlignmentFlag.AlignCenter, initial)
        finally:
            painter.end()
        return pixmap


# ----------------------------------------------------------------------
# 主组件: ManageModelsDialog
# ----------------------------------------------------------------------


class ManageModelsDialog(MessageBoxBase):
    """模型管理弹窗 -- 搜索/筛选/分组折叠/添加/移除模型.

    参考图二设计: 顶部搜索栏 + 筛选标签 + 可折叠分组卡片 + 模型行.

    Args:
        parent: 父窗口.
        fetched_models: 从 ModelFetchService 获取到的模型列表.
        current_models: 供应商当前已有的模型列表.
        provider_id: 供应商 ID.
    """

    model_list_changed = Signal()

    def __init__(
        self,
        parent: QWidget,
        fetched_models: list[ModelEntry],
        current_models: list[ModelEntry],
        provider_id: str,
    ) -> None:
        super().__init__(parent=parent)
        self._fetched_models = fetched_models
        self._current_models = current_models
        self._current_model_ids: set[str] = {m.model_id for m in current_models}
        self._provider_id = provider_id

        # 当前筛选状态
        self._search_text: str = ""
        self._active_capability: str | None = None  # None 表示"全部"

        # 防抖定时器
        self._debounce_timer = QTimer(self)
        self._debounce_timer.setSingleShot(True)
        self._debounce_timer.setInterval(300)
        self._debounce_timer.timeout.connect(self._apply_filter)

        # 分组 section 引用
        self._group_sections: list[_DialogGroupSection] = []

        self._setup_ui()
        self._apply_filter()

    # ------------------------------------------------------------------
    # UI 构建
    # ------------------------------------------------------------------

    def _setup_ui(self) -> None:
        """构建弹窗 UI."""
        self.widget.setMinimumWidth(680)
        self.widget.setMinimumHeight(560)

        # 隐藏 MessageBoxBase 默认的确认/取消按钮及底部色块
        self.yesButton.setVisible(False)
        self.cancelButton.setVisible(False)
        self.buttonGroup.setVisible(False)

        # 标题行: 供应商名 + 关闭按钮
        self._setup_title_bar()
        self.viewLayout.addSpacing(12)

        # 搜索栏 + 工具按钮
        self._setup_search_toolbar()
        self.viewLayout.addSpacing(8)

        # 能力筛选标签页
        self._setup_capability_filter()
        self.viewLayout.addSpacing(12)

        # 模型列表区域 (可滚动)
        self._setup_model_list()

    def _setup_title_bar(self) -> None:
        """标题行: 供应商名称 + 关闭按钮."""
        title_row = QHBoxLayout()
        title_row.setContentsMargins(0, 0, 0, 0)
        title_row.setSpacing(8)

        # 获取供应商名称
        provider_name = ""
        try:
            registry = it(ProviderRegistry)
            provider = registry.get(self._provider_id)
            provider_name = provider.name
        except (KeyError, Exception):
            provider_name = self._provider_id

        title = SubtitleLabel(f"{provider_name}模型", self)
        title_row.addWidget(title)
        title_row.addStretch(1)

        close_btn = TransparentToolButton(FluentIcon.CLOSE, self)
        close_btn.setFixedSize(28, 28)
        close_btn.setToolTip("关闭")
        close_btn.clicked.connect(self.reject)
        title_row.addWidget(close_btn)

        self.viewLayout.addLayout(title_row)

    def _setup_search_toolbar(self) -> None:
        """搜索输入框 + 筛选按钮 + 刷新按钮."""
        toolbar = QHBoxLayout()
        toolbar.setContentsMargins(0, 0, 0, 0)
        toolbar.setSpacing(8)

        self._search_edit = SearchLineEdit(self)
        self._search_edit.setPlaceholderText("搜索模型 ID 或名称")
        self._search_edit.setMaxLength(100)
        self._search_edit.setClearButtonEnabled(True)
        self._search_edit.textChanged.connect(self._on_search_changed)
        toolbar.addWidget(self._search_edit, 1)

        # 筛选按钮 (切换筛选标签行的显隐)
        self._filter_toggle_btn = TransparentToolButton(FluentIcon.FILTER, self)
        self._filter_toggle_btn.setFixedSize(32, 32)
        self._filter_toggle_btn.setToolTip("筛选")
        self._filter_toggle_btn.clicked.connect(self._toggle_filter_row)
        toolbar.addWidget(self._filter_toggle_btn)

        # 刷新按钮
        self._refresh_btn = TransparentToolButton(FluentIcon.SYNC, self)
        self._refresh_btn.setFixedSize(32, 32)
        self._refresh_btn.setToolTip("刷新模型列表")
        self._refresh_btn.clicked.connect(self._on_refresh)
        toolbar.addWidget(self._refresh_btn)

        self.viewLayout.addLayout(toolbar)

    def _setup_capability_filter(self) -> None:
        """能力筛选标签页 -- PillPushButton 行."""
        self._filter_row_widget = QWidget(self)
        filter_row = QHBoxLayout(self._filter_row_widget)
        filter_row.setSpacing(6)
        filter_row.setContentsMargins(0, 0, 0, 0)

        self._filter_pills: dict[str | None, PillPushButton] = {}

        for label, field_name in _CAPABILITY_TABS:
            pill = PillPushButton(self._filter_row_widget)
            pill.setText(label)
            pill.setCheckable(True)
            if field_name is None:
                pill.setChecked(True)
            pill.clicked.connect(
                lambda checked, fn=field_name: self._on_filter_changed(fn)
            )
            self._filter_pills[field_name] = pill
            filter_row.addWidget(pill)

        filter_row.addStretch()
        self.viewLayout.addWidget(self._filter_row_widget)

    def _setup_model_list(self) -> None:
        """模型列表区域 -- 按 group_name 分组展示."""
        self._scroll_area = ScrollArea(self)
        self._scroll_area.setWidgetResizable(True)
        self._scroll_area.setMinimumHeight(340)
        self._scroll_area.setHorizontalScrollBarPolicy(
            Qt.ScrollBarPolicy.ScrollBarAlwaysOff
        )

        self._list_container = QWidget()
        self._list_layout = QVBoxLayout(self._list_container)
        self._list_layout.setContentsMargins(0, 0, 0, 0)
        self._list_layout.setSpacing(8)
        self._list_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        self._scroll_area.setWidget(self._list_container)
        self._scroll_area.enableTransparentBackground()
        self.viewLayout.addWidget(self._scroll_area, 1)

        # 空状态提示
        self._empty_label = BodyLabel("无匹配结果", self._list_container)
        self._empty_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._empty_label.setVisible(False)
        self._list_layout.addWidget(self._empty_label)

    # ------------------------------------------------------------------
    # 搜索与筛选逻辑
    # ------------------------------------------------------------------

    def _toggle_filter_row(self) -> None:
        """切换筛选标签行的显隐."""
        visible = self._filter_row_widget.isVisible()
        self._filter_row_widget.setVisible(not visible)

    def _on_search_changed(self, text: str) -> None:
        """搜索文本变化 -- 启动 300ms 防抖定时器."""
        self._search_text = text.strip()
        self._debounce_timer.start()

    def _on_filter_changed(self, capability: str | None) -> None:
        """能力筛选标签切换 -- 互斥选中."""
        self._active_capability = capability
        for field_name, pill in self._filter_pills.items():
            pill.setChecked(field_name == capability)
        self._apply_filter()

    def _apply_filter(self) -> None:
        """应用搜索 + 能力筛选, 重建模型列表."""
        filtered = self._get_filtered_models()
        self._rebuild_model_list(filtered)

    def _get_filtered_models(self) -> list[ModelEntry]:
        """根据当前搜索文本和能力筛选条件过滤模型列表."""
        result = self._fetched_models

        # 搜索过滤: 大小写不敏感子串匹配 model_id 和 display_name
        if self._search_text:
            query = self._search_text.lower()
            result = [
                m
                for m in result
                if query in m.model_id.lower()
                or query in m.display_name.lower()
            ]

        # 能力筛选
        if self._active_capability is not None:
            result = [
                m
                for m in result
                if getattr(m, self._active_capability, False)
            ]

        return result


    # ------------------------------------------------------------------
    # 模型列表渲染
    # ------------------------------------------------------------------

    def _rebuild_model_list(self, models: list[ModelEntry]) -> None:
        """按 group_name 分组重建模型列表 UI (可折叠卡片风格)."""
        self._clear_model_list()

        if not models:
            self._empty_label.setVisible(True)
            return

        self._empty_label.setVisible(False)

        # 按 group_name 分组
        grouped: OrderedDict[str, list[ModelEntry]] = OrderedDict()
        for model in models:
            group = model.group_name or model.model_id
            grouped.setdefault(group, []).append(model)

        # 排序: 组间按 group_name 大小写不敏感升序, 组内按 model_id 升序
        sorted_groups = sorted(grouped.items(), key=lambda x: x[0].lower())

        for group_name, entries in sorted_groups:
            entries_sorted = sorted(entries, key=lambda m: m.model_id.lower())

            # 创建分组 section
            section = _DialogGroupSection(
                group_name, len(entries_sorted), self._list_container
            )
            section.add_all_in_group.connect(self._on_add_all_in_group)
            self._group_sections.append(section)

            # 判断组内是否全部已添加
            all_added = all(
                m.model_id in self._current_model_ids for m in entries_sorted
            )
            section.update_group_action_icon(all_added)

            # 组内模型行
            for entry in entries_sorted:
                is_added = entry.model_id in self._current_model_ids
                row = _DialogModelRow(entry, is_added, section)
                row.add_clicked.connect(self._on_add_model)
                row.remove_clicked.connect(self._on_remove_model)
                section.add_row(row)

            self._list_layout.addWidget(section)

    def _clear_model_list(self) -> None:
        """清除模型列表中的所有动态内容."""
        # 清除 section 引用
        for section in self._group_sections:
            section.setParent(None)
            section.deleteLater()
        self._group_sections.clear()

        # 清除 layout 中的所有 widget (除了 empty_label)
        while self._list_layout.count() > 0:
            item = self._list_layout.takeAt(0)
            widget = item.widget()
            if widget is not None and widget is not self._empty_label:
                widget.setParent(None)
                widget.deleteLater()

        # 重新添加 empty_label (隐藏状态)
        self._empty_label.setVisible(False)
        self._list_layout.addWidget(self._empty_label)

    # ------------------------------------------------------------------
    # 模型添加/移除操作
    # ------------------------------------------------------------------

    def _on_add_model(self, model_id: str) -> None:
        """添加单个模型到供应商的 models 列表.

        Args:
            model_id: 要添加的模型 ID.
        """
        target: ModelEntry | None = next(
            (m for m in self._fetched_models if m.model_id == model_id), None
        )
        if target is None:
            return

        if model_id in self._current_model_ids:
            return

        # 通过 ProviderRegistry 持久化
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._provider_id)
        except KeyError:
            return

        updated_models = list(provider.models) + [target]
        registry.update_provider(self._provider_id, models=updated_models)

        # 更新本地状态
        self._current_model_ids.add(model_id)
        self._current_models.append(target)

        # 刷新列表显示
        self._apply_filter()

        # 通知外部模型列表已变更
        self.model_list_changed.emit()

    def _on_remove_model(self, model_id: str) -> None:
        """从供应商的 models 列表中移除单个模型.

        Args:
            model_id: 要移除的模型 ID.
        """
        if model_id not in self._current_model_ids:
            return

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._provider_id)
        except KeyError:
            return

        updated_models = [m for m in provider.models if m.model_id != model_id]
        registry.update_provider(self._provider_id, models=updated_models)

        # 更新本地状态
        self._current_model_ids.discard(model_id)
        self._current_models = [m for m in self._current_models if m.model_id != model_id]

        # 刷新列表显示
        self._apply_filter()

        # 通知外部模型列表已变更
        self.model_list_changed.emit()

    def _on_add_all_in_group(self, group_name: str) -> None:
        """添加/移除指定分组内的所有模型.

        如果组内全部已添加则执行移除, 否则执行添加.

        Args:
            group_name: 分组名称.
        """
        # 找出该组的所有模型
        group_models = [
            m for m in self._fetched_models
            if (m.group_name or m.model_id) == group_name
        ]
        if not group_models:
            return

        all_added = all(m.model_id in self._current_model_ids for m in group_models)

        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._provider_id)
        except KeyError:
            return

        if all_added:
            # 移除该组所有模型
            group_ids = {m.model_id for m in group_models}
            updated_models = [m for m in provider.models if m.model_id not in group_ids]
            registry.update_provider(self._provider_id, models=updated_models)

            self._current_model_ids -= group_ids
            self._current_models = [
                m for m in self._current_models if m.model_id not in group_ids
            ]
        else:
            # 添加该组未添加的模型
            to_add = [m for m in group_models if m.model_id not in self._current_model_ids]
            updated_models = list(provider.models) + to_add
            registry.update_provider(self._provider_id, models=updated_models)

            for model in to_add:
                self._current_model_ids.add(model.model_id)
                self._current_models.append(model)

        self._apply_filter()
        self.model_list_changed.emit()

    # ------------------------------------------------------------------
    # 刷新操作
    # ------------------------------------------------------------------

    def _on_refresh(self) -> None:
        """刷新模型列表 -- 重新调用 ModelFetchService 获取最新数据."""
        registry = it(ProviderRegistry)
        try:
            provider = registry.get(self._provider_id)
        except KeyError:
            return

        self._set_loading_state(True)

        self._fetch_thread = _ModelFetchThread(provider, self)
        self._fetch_thread.fetch_finished.connect(self._on_refresh_finished)
        self._fetch_thread.start()

    def _on_refresh_finished(self, result: FetchResult) -> None:
        """刷新完成回调 -- 处理获取结果.

        Args:
            result: ModelFetchService 返回的 FetchResult.
        """
        self._set_loading_state(False)

        if not result.success:
            error_bar(
                content=result.error_message,
                title="刷新失败",
                duration=5000,
                parent=self,
            )
            return

        # 成功: 更新 _fetched_models 并刷新显示
        self._fetched_models = result.models
        self._apply_filter()

    def _set_loading_state(self, loading: bool) -> None:
        """设置/取消加载状态 -- 禁用/启用控件.

        Args:
            loading: True 进入加载状态, False 退出加载状态.
        """
        self._search_edit.setEnabled(not loading)
        for pill in self._filter_pills.values():
            pill.setEnabled(not loading)
        self._refresh_btn.setEnabled(not loading)

    # ------------------------------------------------------------------
    # 公共方法
    # ------------------------------------------------------------------

    def get_filtered_models(self) -> list[ModelEntry]:
        """返回当前搜索和能力筛选后的模型列表 (供外部使用)."""
        return self._get_filtered_models()
