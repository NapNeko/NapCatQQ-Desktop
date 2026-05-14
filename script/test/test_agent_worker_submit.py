# -*- coding: utf-8 -*-
"""Unit tests for AgentWorker.submit_message and _on_event signal dispatching."""
from __future__ import annotations

import asyncio
import os
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

import pytest
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


# Ensure QApplication exists before importing modules that use Signal
ensure_qapp()

from src.core.agent.stream import (
    PermissionAskEvent,
    StreamEnd,
    StreamErrorEvent,
    TextDelta,
    ToolCallComplete,
    ToolCallDelta,
    ToolCallStart,
)
from src.ui.page.agent_page.agent_worker import AgentWorker


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


class TestOnEventDispatching:
    """Tests for _on_event callback dispatching stream events to Qt signals."""

    def test_text_delta_emits_signal(self):
        """TextDelta event emits text_delta signal with msg_id and text."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-001"

        received = []
        worker.text_delta.connect(lambda mid, txt: received.append((mid, txt)))

        event = TextDelta(text="Hello")
        worker._on_event(event)

        assert received == [("msg-001", "Hello")]

    def test_tool_call_start_emits_signal(self):
        """ToolCallStart event emits tool_call_start signal."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-002"

        received = []
        worker.tool_call_start.connect(lambda *args: received.append(args))

        event = ToolCallStart(tool_call_id="tc-1", function_name="file_read")
        worker._on_event(event)

        assert received == [("msg-002", "tc-1", "file_read")]

    def test_tool_call_delta_emits_signal(self):
        """ToolCallDelta event emits tool_call_delta signal."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-003"

        received = []
        worker.tool_call_delta.connect(lambda *args: received.append(args))

        event = ToolCallDelta(tool_call_id="tc-1", arguments_delta='{"path":')
        worker._on_event(event)

        assert received == [("msg-003", "tc-1", '{"path":')]

    def test_tool_call_complete_emits_signal(self):
        """ToolCallComplete event emits tool_call_complete signal."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-004"

        received = []
        worker.tool_call_complete.connect(lambda *args: received.append(args))

        event = ToolCallComplete(
            tool_call_id="tc-1",
            function_name="file_read",
            arguments='{"path": "/tmp/test.py"}',
        )
        worker._on_event(event)

        assert received == [("msg-004", "tc-1", "file_read", '{"path": "/tmp/test.py"}')]

    def test_stream_end_emits_signal(self):
        """StreamEnd event emits stream_end signal."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-005"

        received = []
        worker.stream_end.connect(lambda *args: received.append(args))

        event = StreamEnd(reason="stop")
        worker._on_event(event)

        assert received == [("msg-005", "stop")]

    def test_stream_error_emits_signal(self):
        """StreamErrorEvent emits stream_error signal with event object."""
        worker = AgentWorker()
        worker._current_msg_id = "msg-006"

        received = []
        worker.stream_error.connect(lambda *args: received.append(args))

        event = StreamErrorEvent(status_code=500, message="Internal Server Error")
        worker._on_event(event)

        assert len(received) == 1
        assert received[0][0] == "msg-006"
        assert received[0][1] is event

    def test_permission_ask_emits_signal_and_blocks(self):
        """PermissionAskEvent emits permission_ask signal and blocks until resolved."""
        import threading as _threading

        from PySide6.QtCore import Qt

        worker = AgentWorker()
        worker._current_msg_id = "msg-007"

        received = []
        signal_emitted = _threading.Event()

        def on_permission(*args):
            received.append(args)
            signal_emitted.set()

        # Use DirectConnection so the slot runs in the emitting thread
        worker.permission_ask.connect(on_permission, Qt.ConnectionType.DirectConnection)

        event = PermissionAskEvent(
            tool_id="shell_exec",
            pattern="shell_*",
            description="Execute shell command",
        )

        # Run _on_event in a separate thread since it blocks on permission_barrier
        result_holder = []

        def call_on_event():
            result = worker._on_event(event)
            result_holder.append(result)

        t = _threading.Thread(target=call_on_event)
        t.start()

        # Wait for the signal to be emitted
        assert signal_emitted.wait(timeout=3.0), "Signal was not emitted within timeout"

        assert received == [("msg-007", "shell_exec", "shell_*", "Execute shell command")]

        # Resolve the permission to unblock the thread
        worker.resolve_permission("allow")
        t.join(timeout=2)

        assert result_holder == ["allow"]


class TestSubmitMessage:
    """Tests for submit_message scheduling coroutine on worker loop."""

    def test_submit_stores_msg_id(self):
        """submit_message stores msg_id for signal correlation."""
        worker = AgentWorker()
        worker.start()
        try:
            session_id = uuid4()

            # Replace _do_submit with a no-op coroutine to avoid actual API calls
            async def noop_submit(sid, msg):
                pass

            worker._do_submit = noop_submit

            worker.submit_message(session_id, "hello", msg_id="test-msg-123")
            assert worker._current_msg_id == "test-msg-123"
        finally:
            worker.stop()

    def test_submit_without_thread_is_noop(self):
        """submit_message does nothing if thread is not started."""
        worker = AgentWorker()
        session_id = uuid4()
        # Should not raise
        worker.submit_message(session_id, "hello", msg_id="test-msg")
        assert worker._current_msg_id == "test-msg"
        assert worker._current_future is None

    def test_submit_schedules_coroutine(self):
        """submit_message schedules _do_submit on the worker's event loop."""
        worker = AgentWorker()
        worker.start()
        try:
            session_id = uuid4()

            # Track if _do_submit was called
            called_with = []
            original_do_submit = worker._do_submit

            async def mock_do_submit(sid, msg):
                called_with.append((sid, msg))

            worker._do_submit = mock_do_submit

            worker.submit_message(session_id, "test message", msg_id="m1")

            # Wait for the coroutine to complete
            assert worker._current_future is not None
            worker._current_future.result(timeout=5)

            assert called_with == [(session_id, "test message")]
        finally:
            worker.stop()


class TestCancel:
    """Tests for cancel() method."""

    def test_cancel_without_future_is_safe(self):
        """cancel() does not raise when no future is pending."""
        worker = AgentWorker()
        worker.cancel()  # should not raise

    def test_cancel_cancels_pending_future(self):
        """cancel() cancels the current asyncio future."""
        worker = AgentWorker()
        worker.start()
        try:
            # Create a long-running coroutine
            async def long_task():
                await asyncio.sleep(100)

            worker._current_future = asyncio.run_coroutine_threadsafe(
                long_task(), worker._thread.loop
            )
            assert not worker._current_future.done()

            worker.cancel()
            assert worker._current_future is None
        finally:
            worker.stop()
