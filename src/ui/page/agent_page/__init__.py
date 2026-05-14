# -*- coding: utf-8 -*-
"""
此模块用于展示软件内 Agent 聊天页面, 提供与 AI Agent 的对话交互界面
"""
from __future__ import annotations

# 标准库导入
import logging
from abc import ABC
from typing import TYPE_CHECKING, Self
from uuid import UUID

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    AgentChatPanel,
    ChatMessage,
    ChatRole,
    InfoBar,
    InfoBarPosition,
    PermissionDock,
    ToolCallStatus,
)

from src.ui.common.style_sheet import PageStyleSheet
from .agent_worker import AgentWorker
from .session_sidebar import SessionSidebar

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow

logger = logging.getLogger(__name__)


class AgentChatPage(QWidget):
    """Agent 聊天页面"""

    def __init__(self) -> None:
        super().__init__()
        self._current_session_id: UUID | None = None
        self._current_msg_id: str = ""
        self._initialized: bool = False

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化(轻量级) -- 仅设置导航所需的最小属性.

        重量级组件创建和信号连接延迟到首次 showEvent 时执行,
        避免影响应用启动性能.
        """
        self.setObjectName("agent_page")
        self.setParent(parent)
        return self

    def showEvent(self, event) -> None:  # noqa: N802 - Qt 重写
        """首次显示时执行完整初始化."""
        super().showEvent(event)
        if not self._initialized:
            self._initialized = True
            self._do_full_initialize()

    def _do_full_initialize(self) -> None:
        """执行完整的组件创建, 布局和信号连接."""

        # --- 创建组件 ---
        self._sidebar = SessionSidebar(self)
        self._panel = AgentChatPanel(self)
        self._permission_dock = PermissionDock(self)
        self._worker = AgentWorker(self)

        # --- 配置组件 ---
        self._sidebar.setFixedWidth(240)
        self._panel.setMaxContentWidth(800)
        self._panel.setInputPlaceholder("输入消息...")

        # --- 布局 ---
        # 右侧聊天区域: AgentChatPanel + PermissionDock
        right_layout = QVBoxLayout()
        right_layout.setContentsMargins(0, 0, 0, 0)
        right_layout.setSpacing(0)
        right_layout.addWidget(self._panel, 1)
        right_layout.addWidget(self._permission_dock, 0)

        # 主布局: 左侧 SessionSidebar + 右侧聊天区域
        main_layout = QHBoxLayout(self)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)
        main_layout.addWidget(self._sidebar, 0)
        main_layout.addLayout(right_layout, 1)

        # --- 信号连接 ---
        self._connect_signals()

        # --- 启动 Worker ---
        self._worker.start()

        # --- 应用主题样式 ---
        PageStyleSheet.AGENT.apply(self)

        # --- 加载会话列表 ---
        # 必须在信号连接之后调用, 这样自动选中首个会话时
        # session_selected 才能触发 _on_session_selected 加载消息.
        self._sidebar.refresh_list()

    # ------------------------------------------------------------------
    # Signal wiring
    # ------------------------------------------------------------------

    def _connect_signals(self) -> None:
        """连接所有组件间的信号/槽."""
        # --- 6.2: Send flow ---
        self._panel.sendRequested.connect(self._on_send_requested)

        # --- 6.3: Stream events ---
        self._worker.text_delta.connect(self._on_text_delta)
        self._worker.tool_call_start.connect(self._on_tool_call_start)
        self._worker.tool_call_delta.connect(self._on_tool_call_delta)
        self._worker.tool_call_complete.connect(self._on_tool_call_complete)
        self._worker.stream_end.connect(self._on_stream_end)
        self._worker.stream_error.connect(self._on_stream_error)

        # Stop generation button
        self._panel.chatView().stopRequested.connect(self._worker.cancel)

        # --- 6.4: Permission flow ---
        self._worker.permission_ask.connect(self._on_permission_ask)
        self._permission_dock.approved.connect(
            lambda: self._worker.resolve_permission("allow")
        )
        self._permission_dock.rejected.connect(
            lambda: self._worker.resolve_permission("deny")
        )
        self._permission_dock.alwaysAllowed.connect(
            lambda: self._worker.resolve_permission("always_allow")
        )

        # --- 6.5: Session selection ---
        self._sidebar.session_selected.connect(self._on_session_selected)

    # ------------------------------------------------------------------
    # 6.2: Send flow handlers
    # ------------------------------------------------------------------

    def _on_send_requested(self) -> None:
        """处理用户发送消息请求."""
        text = self._panel.inputEdit().text()
        if not text.strip():
            return

        # 添加用户消息到视图
        user_msg = ChatMessage(ChatRole.USER, content=text)
        self._panel.addMessage(user_msg)

        # 开始 Agent 响应(创建空 AGENT bubble + 启动 generation 状态)
        self._current_msg_id = self._panel.beginAgentResponse(status_text="思考中...")

        # 禁用输入
        self._panel.setInputEnabled(False)

        # 提交到后端
        if self._current_session_id is not None:
            self._worker.submit_message(
                self._current_session_id, text, msg_id=self._current_msg_id
            )

    # ------------------------------------------------------------------
    # 6.3: Stream event handlers
    # ------------------------------------------------------------------

    def _on_text_delta(self, msg_id: str, text: str) -> None:
        """处理文本增量事件."""
        bubble = self._panel.chatView().bubble(msg_id)
        if bubble:
            bubble.appendDelta(text)

    def _on_tool_call_start(
        self, msg_id: str, tool_call_id: str, func_name: str
    ) -> None:
        """处理工具调用开始事件."""
        self._panel.chatView().addToolCall(
            msg_id, func_name, metadata={"id": tool_call_id}
        )

    def _on_tool_call_delta(
        self, msg_id: str, tool_call_id: str, arguments_delta: str
    ) -> None:
        """处理工具调用参数增量事件."""
        bubble = self._panel.chatView().bubble(msg_id)
        if bubble:
            tool_card = bubble.toolCallCard(tool_call_id)
            if tool_card:
                tool_card.appendArgumentsDelta(arguments_delta)

    def _on_tool_call_complete(
        self, msg_id: str, tool_call_id: str, func_name: str, result: str
    ) -> None:
        """处理工具调用完成事件."""
        bubble = self._panel.chatView().bubble(msg_id)
        if bubble:
            tool_card = bubble.toolCallCard(tool_call_id)
            if tool_card:
                tool_card.setResult(result)
                tool_card.setStatus(ToolCallStatus.SUCCESS)

    def _on_stream_end(self, msg_id: str, reason: str) -> None:
        """处理流结束事件."""
        self._panel.chatView().endGeneration(msg_id)
        self._panel.setInputEnabled(True)

    def _on_stream_error(self, msg_id: str, error: object) -> None:
        """处理流错误事件, 显示错误通知并重新启用输入."""
        error_message = str(error) if error else "未知错误"
        InfoBar.error(
            title="生成错误",
            content=error_message,
            parent=self,
            position=InfoBarPosition.TOP,
            duration=5000,
        )
        self._panel.chatView().endGeneration(msg_id)
        self._panel.setInputEnabled(True)

    # ------------------------------------------------------------------
    # 6.4: Permission flow handlers
    # ------------------------------------------------------------------

    def _on_permission_ask(
        self, msg_id: str, tool_id: str, pattern: str, description: str
    ) -> None:
        """处理权限询问事件, 展开 PermissionDock 并显示请求."""
        self._permission_dock.setRequest(tool_id, description, patterns=[pattern])
        self._permission_dock.setCollapsed(False)

    # ------------------------------------------------------------------
    # 6.5: Session selection handlers
    # ------------------------------------------------------------------

    def _on_session_selected(self, session_id: object) -> None:
        """处理会话选择事件, 加载会话消息到 AgentChatView."""
        from src.core.agent.session import SessionManager

        self._current_session_id = session_id  # type: ignore[assignment]

        try:
            manager: SessionManager = it(SessionManager)
            session = manager.get(session_id)  # type: ignore[arg-type]
        except Exception as exc:
            logger.warning("Failed to load session %s: %s", session_id, exc)
            return

        # 清空当前视图并加载会话消息
        chat_view = self._panel.chatView()
        chat_view.clear()

        if session.messages:
            for msg in session.messages:
                chat_message = self._to_chat_message(msg)
                self._panel.addMessage(chat_message)
        else:
            # 空会话时添加一条 AI 欢迎消息, 避免界面空旷
            welcome_msg = ChatMessage(
                ChatRole.AGENT,
                content="你好！我是 NapCat Agent，有什么可以帮你的吗？",
            )
            self._panel.addMessage(welcome_msg)

    def _to_chat_message(self, msg) -> ChatMessage:
        """将后端 Message 转换为库 ChatMessage.

        Args:
            msg: 后端 Message 对象.

        Returns:
            转换后的 ChatMessage 实例.
        """
        role_map = {
            "user": ChatRole.USER,
            "assistant": ChatRole.AGENT,
            "system": ChatRole.SYSTEM,
            "tool": ChatRole.SYSTEM,
        }
        return ChatMessage(
            role=role_map.get(msg.role, ChatRole.SYSTEM),
            content=msg.content,
            timestamp=msg.timestamp,
            id=str(msg.id) if hasattr(msg, "id") else "",
        )


class AgentChatPageCreator(AbstractCreator, ABC):
    """Agent 聊天页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.agent_page",
            identify="AgentChatPage",
            humanized_name="Agent 聊天页面",
            description="NapCat Desktop Agent 聊天页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Agent 聊天页面模块是否可用"""
        return exists_module("src.ui.page.agent_page")

    @staticmethod
    def create(create_type):
        """创建 Agent 聊天页面实例"""
        return create_type()


add_creator(AgentChatPageCreator)
