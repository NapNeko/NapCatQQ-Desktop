# -*- coding: utf-8 -*-
"""AgentWorker -- 后端通信桥接层.

在独立 QThread 中运行 asyncio event loop, 通过 Qt Signal 将
AgentEngine 的流式事件安全地回传到 UI 线程.
"""
from __future__ import annotations

import asyncio
import threading
from concurrent.futures import Future
from uuid import UUID

from PySide6.QtCore import QObject, QThread, Signal

from src.core.logging import LogSource, logger

from src.core.agent.stream import (
    PermissionAskEvent,
    StreamEnd,
    StreamErrorEvent,
    StreamEvent,
    TextDelta,
    ToolCallComplete,
    ToolCallDelta,
    ToolCallStart,
)


class _AsyncioThread(QThread):
    """QThread 子类, 在 run() 中运行 asyncio event loop.

    将 asyncio event loop 的创建和运行封装在 QThread.run() 中,
    确保 loop 在独立线程上下文中创建, 避免与 Qt 事件循环冲突.
    """

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self.loop: asyncio.AbstractEventLoop | None = None
        self._ready = threading.Event()

    def run(self) -> None:
        """在线程中创建并运行 asyncio event loop 直到被停止."""
        self.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self.loop)
        self._ready.set()
        try:
            self.loop.run_forever()
        finally:
            self.loop.close()
            self.loop = None

    def wait_ready(self, timeout: float = 5.0) -> bool:
        """等待 event loop 就绪, 返回是否成功."""
        return self._ready.wait(timeout)


class AgentWorker(QObject):
    """在独立线程中运行 AgentEngine.submit(), 通过信号回传事件.

    所有信号携带 msg_id 以关联到正确的 ChatBubble.

    Signals:
        text_delta: (msg_id, text) 文本增量事件.
        tool_call_start: (msg_id, tool_call_id, function_name) 工具调用开始.
        tool_call_delta: (msg_id, tool_call_id, arguments_delta) 工具调用参数增量.
        tool_call_complete: (msg_id, tool_call_id, function_name, result) 工具调用完成.
        stream_end: (msg_id, reason) 流结束事件.
        stream_error: (msg_id, StreamErrorEvent) 流错误事件.
        permission_ask: (msg_id, tool_id, pattern, description) 权限询问事件.
    """

    # --- Signals ---
    text_delta = Signal(str, str)                    # msg_id, text
    tool_call_start = Signal(str, str, str)          # msg_id, tool_call_id, function_name
    tool_call_delta = Signal(str, str, str)          # msg_id, tool_call_id, arguments_delta
    tool_call_complete = Signal(str, str, str, str)  # msg_id, tool_call_id, function_name, result
    stream_end = Signal(str, str)                    # msg_id, reason
    stream_error = Signal(str, object)               # msg_id, StreamErrorEvent
    permission_ask = Signal(str, str, str, str)      # msg_id, tool_id, pattern, description

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._thread: _AsyncioThread | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._current_future: Future | None = None
        self._current_msg_id: str = ""

        # Permission resolution state
        self._permission_barrier = threading.Event()
        self._permission_decision: str = ""

    # --- Thread lifecycle ---

    def start(self) -> None:
        """启动 worker 线程和内部 asyncio event loop.

        创建一个 _AsyncioThread(自定义 QThread), 在其 run() 方法中
        运行 asyncio event loop. 如果线程已在运行则直接返回.
        阻塞直到 event loop 就绪.
        """
        if self._thread is not None:
            return
        self._thread = _AsyncioThread(self)
        self._thread.start()
        self._thread.wait_ready()

    def stop(self) -> None:
        """停止 worker 线程和 asyncio event loop.

        线程安全地停止 event loop, 然后等待 QThread 退出.
        调用后 worker 不可再使用, 需重新 start().
        """
        if self._thread is not None:
            loop = self._thread.loop
            if loop is not None and loop.is_running():
                loop.call_soon_threadsafe(loop.stop)
            self._thread.wait()
            self._loop = None
            self._thread = None
            self._current_future = None

    @property
    def loop(self) -> asyncio.AbstractEventLoop | None:
        """获取 worker 线程中的 asyncio event loop(只读)."""
        if self._thread is not None:
            return self._thread.loop
        return self._loop

    def submit_message(
        self, session_id: UUID, user_message: str, *, msg_id: str | None = None
    ) -> None:
        """提交消息到 AgentEngine(在 worker 线程中执行).

        将用户消息提交给后端 AgentEngine.submit(), 流式事件通过
        对应的 Qt Signal 回传到 UI 线程.

        Args:
            session_id: 目标会话 ID.
            user_message: 用户输入的消息文本.
            msg_id: 可选的消息 ID, 用于信号关联. 默认使用 session_id 字符串.
        """
        self._current_msg_id = msg_id if msg_id is not None else str(session_id)

        loop = self.loop
        if loop is None:
            logger.warning("AgentWorker: loop not ready, cannot submit message.")
            return

        self._current_future = asyncio.run_coroutine_threadsafe(
            self._do_submit(session_id, user_message), loop
        )

    async def _do_submit(self, session_id: UUID, user_message: str) -> None:
        """在 worker 线程的 asyncio loop 中执行 AgentEngine.submit().

        Args:
            session_id: 目标会话 ID.
            user_message: 用户输入的消息文本.
        """
        from creart import it

        from src.core.agent.engine import AgentEngine

        engine = it(AgentEngine)

        try:
            await engine.submit(session_id, user_message, self._on_event)
        except asyncio.CancelledError:
            logger.info(f"AgentWorker: submit cancelled for session {session_id}")
            self.stream_end.emit(self._current_msg_id, "cancelled")
        except Exception as exc:
            logger.exception(f"AgentWorker: submit failed for session {session_id}", exc)
            self.stream_error.emit(self._current_msg_id, exc)

    def cancel(self) -> None:
        """取消当前流式生成.

        中止正在进行的 AgentEngine 流式调用, 释放相关资源.
        通过取消 concurrent.futures.Future 来中止底层 asyncio 协程.
        同时解除可能存在的权限等待阻塞, 避免死锁.
        """
        # Unblock any pending permission wait to avoid deadlock
        self._permission_decision = "deny"
        self._permission_barrier.set()

        future = self._current_future
        if future is None:
            return

        # Cancel the concurrent.futures.Future which will cancel the underlying coroutine
        future.cancel()
        self._current_future = None

    def resolve_permission(self, decision: str) -> None:
        """回传权限决策, 解除 worker 线程的阻塞等待.

        当用户在 PermissionDock 中做出选择后调用此方法,
        通过 threading.Event 通知 worker 线程继续执行.

        此方法从 UI 线程调用, 通过 threading.Event.set() 唤醒
        在 worker 线程中阻塞等待的 _on_event 回调.

        Args:
            decision: 权限决策, 可选值: "allow" | "deny" | "always_allow".
        """
        self._permission_decision = decision
        self._permission_barrier.set()

    def _on_event(self, event: StreamEvent) -> str | None:
        """AgentEngine on_event 回调, 在 worker 线程中执行.

        根据事件类型 emit 对应的 Qt Signal, 将事件安全地回传到 UI 线程.
        对于 PermissionAskEvent, 会阻塞 worker 线程直到 UI 回传决策.

        Args:
            event: 来自 AgentEngine/StreamProcessor 的流式事件.

        Returns:
            对于 PermissionAskEvent 返回用户决策字符串, 其他事件返回 None.
        """
        if isinstance(event, TextDelta):
            self.text_delta.emit(self._current_msg_id, event.text)

        elif isinstance(event, ToolCallStart):
            self.tool_call_start.emit(
                self._current_msg_id, event.tool_call_id, event.function_name
            )

        elif isinstance(event, ToolCallDelta):
            self.tool_call_delta.emit(
                self._current_msg_id, event.tool_call_id, event.arguments_delta
            )

        elif isinstance(event, ToolCallComplete):
            self.tool_call_complete.emit(
                self._current_msg_id,
                event.tool_call_id,
                event.function_name,
                event.arguments,
            )

        elif isinstance(event, StreamEnd):
            self.stream_end.emit(self._current_msg_id, event.reason)

        elif isinstance(event, StreamErrorEvent):
            self.stream_error.emit(self._current_msg_id, event)

        elif isinstance(event, PermissionAskEvent):
            # Clear the barrier for a fresh wait
            self._permission_barrier.clear()
            self._permission_decision = ""

            # Emit signal to UI thread to show permission dock
            self.permission_ask.emit(
                self._current_msg_id,
                event.tool_id,
                event.pattern,
                event.description,
            )

            # Block worker thread until UI resolves the permission
            self._permission_barrier.wait()

            # Return the decision to the engine
            return self._permission_decision

        return None
