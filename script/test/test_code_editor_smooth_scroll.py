# -*- coding: utf-8 -*-

import os

from PySide6.QtCore import QPoint
from PySide6.QtWidgets import QApplication

from src.ui.components.code_editor.exhibit import CodeExibit, UpdateLogExhibit


class FakeWheelEvent:
    def __init__(self, angle_y: int = -120, pixel_y: int = 0) -> None:
        self._angle_delta = QPoint(0, angle_y)
        self._pixel_delta = QPoint(0, pixel_y)
        self.accepted = False

    def angleDelta(self) -> QPoint:
        return self._angle_delta

    def pixelDelta(self) -> QPoint:
        return self._pixel_delta

    def accept(self) -> None:
        self.accepted = True


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_code_exhibit_uses_internal_smooth_scroll() -> None:
    ensure_qapp()
    widget = CodeExibit(None)
    widget.resize(320, 120)
    widget.setPlainText("\n".join(f"line {index}" for index in range(200)))
    widget.show()
    QApplication.processEvents()

    event = FakeWheelEvent()

    handled = widget._handle_smooth_wheel_event(event)

    assert handled is True
    assert event.accepted is True
    assert getattr(widget, "scrollDelegate", None) is None
    widget.close()


def test_update_log_exhibit_uses_internal_smooth_scroll() -> None:
    ensure_qapp()
    widget = UpdateLogExhibit(None)
    widget.resize(320, 120)
    widget.setHtml("<br>".join(f"line {index}" for index in range(200)))
    widget.show()
    QApplication.processEvents()

    event = FakeWheelEvent()

    handled = widget._handle_smooth_wheel_event(event)

    assert handled is True
    assert event.accepted is True
    widget.close()
