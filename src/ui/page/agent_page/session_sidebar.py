# -*- coding: utf-8 -*-
"""
会话管理侧边栏模块.

提供会话列表展示, 新建会话, 切换会话, 删除会话等功能.
库无等价组件, 需自建.
"""
from __future__ import annotations

from typing import TYPE_CHECKING
from uuid import UUID

from creart import it
from PySide6.QtCore import QPoint, Qt, Signal
from PySide6.QtWidgets import QHBoxLayout, QListWidgetItem, QVBoxLayout, QWidget
from qfluentwidgets import (
    Action,
    FluentIcon as FIF,
    LineEdit,
    ListWidget,
    MessageBoxBase,
    RoundMenu,
    SubtitleLabel,
    TransparentToolButton,
)

if TYPE_CHECKING:
    from src.core.agent.session import SessionManager


class _SearchSessionDialog(MessageBoxBase):
    """搜索会话弹窗."""

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self.search_edit = LineEdit(self)
        self.search_edit.setPlaceholderText("输入关键词搜索会话...")
        self.search_edit.setClearButtonEnabled(True)
        self.viewLayout.addWidget(self.search_edit)
        self.yesButton.setText("搜索")
        self.cancelButton.setText("取消")
        self.widget.setMinimumWidth(320)

    @property
    def keyword(self) -> str:
        return self.search_edit.text().strip()


class SessionSidebar(QWidget):
    """会话管理侧边栏.

    带有标题栏和会话列表的侧边栏面板.
    通过右侧 padding 和背景色差异与聊天区域形成视觉层次.

    Signals:
        session_selected: 用户选择会话时发射, 携带会话 UUID.
        session_created: 新建会话后发射, 携带新会话 UUID.
    """

    session_selected = Signal(object)  # UUID
    session_created = Signal(object)  # UUID

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("sessionSidebar")

        # --- 主布局 ---
        self._layout = QVBoxLayout(self)
        self._layout.setContentsMargins(12, 16, 0, 12)
        self._layout.setSpacing(12)

        # --- 标题栏: "会话" + 搜索按钮 + 新建按钮 ---
        header_layout = QHBoxLayout()
        header_layout.setContentsMargins(0, 0, 12, 0)
        header_layout.setSpacing(4)

        self._title_label = SubtitleLabel("会话", self)
        header_layout.addWidget(self._title_label)
        header_layout.addStretch(1)

        self._search_btn = TransparentToolButton(FIF.SEARCH, self)
        self._search_btn.setToolTip("搜索会话")
        self._search_btn.setFixedSize(32, 32)
        header_layout.addWidget(self._search_btn)

        self._new_session_btn = TransparentToolButton(FIF.ADD, self)
        self._new_session_btn.setToolTip("新建会话")
        self._new_session_btn.setFixedSize(32, 32)
        header_layout.addWidget(self._new_session_btn)

        self._layout.addLayout(header_layout)

        # --- 会话列表 ---
        self._list_widget = ListWidget(self)
        self._list_widget.setObjectName("sessionListWidget")
        self._layout.addWidget(self._list_widget)

        # Enable custom context menu on the list widget
        self._list_widget.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)

        # Connect signals
        self._list_widget.currentItemChanged.connect(self._on_current_item_changed)
        self._list_widget.customContextMenuRequested.connect(self._on_context_menu_requested)
        self._new_session_btn.clicked.connect(self._on_new_session)
        self._search_btn.clicked.connect(self._on_search_clicked)

    def refresh_list(self) -> None:
        """刷新会话列表.

        从 SessionManager.list_sessions() 获取最新会话摘要,
        按 last_updated 降序排列后更新列表控件.
        如果列表为空, 自动创建默认会话.
        如果当前未选中任何会话, 自动选中最近的一项.
        """
        from src.core.agent.session import SessionManager

        manager: SessionManager = it(SessionManager)
        sessions = manager.list_sessions()

        # Ensure descending order by last_updated
        sessions.sort(key=lambda s: s.last_updated, reverse=True)

        # Handle empty session list by auto-creating default session
        if not sessions:
            new_session = manager.create("新对话")
            sessions = manager.list_sessions()
            sessions.sort(key=lambda s: s.last_updated, reverse=True)
            # Emit session_created for the auto-created session
            self._populate_list(sessions)
            self.session_created.emit(new_session.session_id)
            # Select the newly created session
            self.select_session(new_session.session_id)
            return

        self._populate_list(sessions)

        # 如果当前没有选中项, 自动选中最近的一项以触发 session_selected
        if self._list_widget.currentItem() is None and self._list_widget.count() > 0:
            self._list_widget.setCurrentRow(0)

    def _populate_list(self, sessions) -> None:
        """填充会话列表控件.

        Args:
            sessions: 已排序的 SessionSummary 列表.
        """
        self._list_widget.clear()

        for summary in sessions:
            # Build display text: agent_name + last message preview
            preview = getattr(summary, "last_message_preview", None) or ""
            if preview:
                # Truncate preview to 28 chars
                truncated = preview[:28] + "…" if len(preview) > 28 else preview
                display_text = f"{summary.agent_name}\n{truncated}"
            else:
                display_text = summary.agent_name

            item = QListWidgetItem(display_text)
            item.setData(Qt.ItemDataRole.UserRole, summary.session_id)
            # 条目高度
            from PySide6.QtCore import QSize
            item.setSizeHint(QSize(0, 40))
            self._list_widget.addItem(item)

    def _on_search_text_changed(self, text: str) -> None:
        """根据搜索文本过滤会话列表条目."""
        search_lower = text.strip().lower()
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item is None:
                continue
            visible = not search_lower or search_lower in (item.text() or "").lower()
            item.setHidden(not visible)

    def _on_search_clicked(self) -> None:
        """弹出搜索对话框, 根据关键词过滤并选中匹配的会话."""
        dialog = _SearchSessionDialog(self.window())
        if dialog.exec():
            keyword = dialog.keyword.lower()
            if not keyword:
                return
            # 找到第一个匹配的条目并选中
            for i in range(self._list_widget.count()):
                item = self._list_widget.item(i)
                if item and keyword in (item.text() or "").lower():
                    self._list_widget.setCurrentItem(item)
                    break

    def select_session(self, session_id: UUID) -> None:
        """选中指定会话.

        Args:
            session_id: 要选中的会话 UUID.
        """
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if item and item.data(Qt.ItemDataRole.UserRole) == session_id:
                self._list_widget.setCurrentItem(item)
                break

    def _on_new_session(self) -> None:
        """Handle 'New Session' button click.

        Creates a new session via SessionManager, refreshes the list,
        selects the new session, and emits session_created signal.
        """
        from src.core.agent.session import SessionManager

        manager: SessionManager = it(SessionManager)
        new_session = manager.create("新对话")

        # Refresh list to include the new session
        self.refresh_list()

        # Select the newly created session
        self.select_session(new_session.session_id)

        # Emit session_created signal
        self.session_created.emit(new_session.session_id)

    def _on_context_menu_requested(self, pos: QPoint) -> None:
        """Show context menu with delete action on right-click.

        Args:
            pos: The position where the context menu was requested.
        """
        item = self._list_widget.itemAt(pos)
        if item is None:
            return

        session_id = item.data(Qt.ItemDataRole.UserRole)
        if session_id is None:
            return

        menu = RoundMenu(parent=self)
        delete_action = Action(FIF.DELETE, "Delete", parent=menu)
        delete_action.triggered.connect(lambda: self._on_delete_session(session_id))
        menu.addAction(delete_action)

        # Show menu at global position
        menu.exec(self._list_widget.mapToGlobal(pos))

    def _on_delete_session(self, session_id: UUID) -> None:
        """Delete a session and switch to the most recent remaining.

        Args:
            session_id: The UUID of the session to delete.
        """
        from src.core.agent.session import SessionManager

        manager: SessionManager = it(SessionManager)
        manager.delete(session_id)

        # Refresh list (this handles empty list by auto-creating default)
        self.refresh_list()

        # Select the first item (most recent due to descending sort)
        if self._list_widget.count() > 0:
            first_item = self._list_widget.item(0)
            if first_item is not None:
                self._list_widget.setCurrentItem(first_item)

    def _on_current_item_changed(
        self, current: QListWidgetItem | None, _previous: QListWidgetItem | None
    ) -> None:
        """Handle list selection change and emit session_selected signal."""
        if current is not None:
            session_id = current.data(Qt.ItemDataRole.UserRole)
            if session_id is not None:
                self.session_selected.emit(session_id)
