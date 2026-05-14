# -*- coding: utf-8 -*-
"""Unit tests for AgentWorker thread lifecycle management."""
from __future__ import annotations

import asyncio
import os

import pytest
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from src.ui.page.agent_page.agent_worker import AgentWorker, _AsyncioThread


def ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


class TestAgentWorkerLifecycle:
    """Tests for AgentWorker start/stop thread lifecycle."""

    def test_initial_state_no_thread(self):
        """Worker starts with no thread or loop."""
        worker = AgentWorker()
        assert worker._thread is None
        assert worker.loop is None

    def test_start_creates_thread(self):
        """start() creates a QThread and starts it."""
        worker = AgentWorker()
        try:
            worker.start()
            assert worker._thread is not None
            assert isinstance(worker._thread, _AsyncioThread)
            assert worker._thread.isRunning()
        finally:
            worker.stop()

    def test_start_idempotent(self):
        """Calling start() twice does not create a second thread."""
        worker = AgentWorker()
        try:
            worker.start()
            thread_ref = worker._thread
            worker.start()  # second call should be no-op
            assert worker._thread is thread_ref
        finally:
            worker.stop()

    def test_stop_cleans_up(self):
        """stop() terminates thread and clears references."""
        worker = AgentWorker()
        worker.start()
        worker.stop()
        assert worker._thread is None
        assert worker.loop is None

    def test_event_loop_running_after_start(self):
        """The asyncio event loop is running after start() returns."""
        worker = AgentWorker()
        try:
            worker.start()
            assert worker.loop is not None
            assert isinstance(worker.loop, asyncio.AbstractEventLoop)
            assert worker.loop.is_running()
        finally:
            worker.stop()

    def test_stop_without_start_is_safe(self):
        """stop() on a never-started worker does not raise."""
        worker = AgentWorker()
        worker.stop()  # should not raise
        assert worker._thread is None
        assert worker.loop is None

    def test_restart_after_stop(self):
        """Worker can be started again after being stopped."""
        worker = AgentWorker()
        try:
            worker.start()
            worker.stop()
            assert worker._thread is None

            # Restart
            worker.start()
            assert worker._thread is not None
            assert worker._thread.isRunning()
            assert worker.loop is not None
            assert worker.loop.is_running()
        finally:
            worker.stop()

    def test_can_schedule_coroutine_on_loop(self):
        """Verify we can schedule async work on the worker's event loop."""
        worker = AgentWorker()
        results = []

        async def _task():
            results.append("executed")

        try:
            worker.start()
            assert worker.loop is not None
            asyncio.run_coroutine_threadsafe(_task(), worker.loop).result(timeout=2)
            assert results == ["executed"]
        finally:
            worker.stop()
