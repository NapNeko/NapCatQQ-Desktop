# -*- coding: utf-8 -*-
from __future__ import annotations

from dataclasses import dataclass

import psutil
from PySide6.QtCore import QObject, Qt, QTimer, Signal


@dataclass(slots=True)
class OccupancySnapshot:
    cpu_percent: float
    memory_percent: float


class SystemOccupancySampler(QObject):
    """系统占用采样服务。"""

    sampleChanged = Signal(float, float)

    def __init__(self, interval_ms: int = 1200, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._initialized = False
        self._timer = QTimer(self)
        self._timer.setInterval(interval_ms)
        self._timer.setTimerType(Qt.TimerType.PreciseTimer)
        self._timer.timeout.connect(self.poll_now)
        psutil.cpu_percent(interval=None)

    def start(self) -> None:
        self._timer.start()
        self.poll_now()

    def interval_ms(self) -> int:
        return self._timer.interval()

    def poll_now(self) -> None:
        cpu_percent = psutil.cpu_percent(interval=None)
        if cpu_percent == 0 and not self._initialized:
            cpu_percent = psutil.cpu_percent(interval=0.05)

        memory_percent = psutil.virtual_memory().percent
        self._initialized = True
        self.sampleChanged.emit(cpu_percent, memory_percent)
