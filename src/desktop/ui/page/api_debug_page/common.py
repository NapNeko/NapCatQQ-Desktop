# -*- coding: utf-8 -*-
from __future__ import annotations

"""API 调试页公共工具。"""

import json
from typing import Any, Callable

from PySide6.QtCore import QObject, QRunnable, Signal
from PySide6.QtWidgets import QWidget
from qfluentwidgets import ComboBox


def pretty_json(payload: Any) -> str:
    """安全格式化 JSON。"""
    try:
        return json.dumps(payload, ensure_ascii=False, indent=2)
    except TypeError:
        return str(payload)


def find_index_by_data(combo: ComboBox, key: str) -> int:
    """根据 userData 查找下标。"""
    for index in range(combo.count()):
        if str(combo.itemData(index) or "") == key:
            return index
    return -1


def refresh_widget_style(widget: QWidget) -> None:
    """在动态属性变化后刷新控件样式。"""
    style = widget.style()
    if style is None:
        return
    style.unpolish(widget)
    style.polish(widget)
    widget.update()


class CallableTask(QObject, QRunnable):
    """在 `QThreadPool` 中执行同步任务。"""

    result_ready = Signal(object)
    error_raised = Signal(str)
    finished = Signal()

    def __init__(self, func: Callable[[], Any]) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.func = func
        self.setAutoDelete(True)

    def run(self) -> None:
        try:
            self.result_ready.emit(self.func())
        except Exception as error:  # pragma: no cover - UI 侧统一处理
            self.error_raised.emit(str(error))
        finally:
            self.finished.emit()
