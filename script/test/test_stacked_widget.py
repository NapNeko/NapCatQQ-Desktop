# -*- coding: utf-8 -*-

from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication, QWidget

from src.desktop.ui.components.stacked_widget import TransparentStackedWidget


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_transparent_stacked_widget_uses_soft_animation() -> None:
    """切页时应触发共享动画，并在动画结束后停留到目标页面。"""
    app = ensure_qapp()
    stacked = TransparentStackedWidget()
    first = QWidget()
    second = QWidget()

    stacked.resize(320, 240)
    stacked.addWidget(first)
    stacked.addWidget(second)
    stacked.show()
    app.processEvents()

    assert stacked.currentWidget() is first

    ani_started: list[bool] = []
    ani_finished: list[bool] = []
    stacked.aniStart.connect(lambda: ani_started.append(True))
    stacked.aniFinished.connect(lambda: ani_finished.append(True))

    stacked.setCurrentWidget(second)
    QTest.qWait(450)
    app.processEvents()

    assert ani_started
    assert ani_finished
    assert stacked.currentWidget() is second


def test_transparent_stacked_widget_can_disable_animation() -> None:
    """禁用动画后应立即切换页面，且不再发出动画开始信号。"""
    app = ensure_qapp()
    stacked = TransparentStackedWidget()
    first = QWidget()
    second = QWidget()

    stacked.addWidget(first)
    stacked.addWidget(second)
    stacked.setAnimationEnabled(False)

    ani_started: list[bool] = []
    stacked.aniStart.connect(lambda: ani_started.append(True))

    stacked.setCurrentWidget(second)
    app.processEvents()

    assert not ani_started
    assert stacked.currentWidget() is second


def test_transparent_stacked_widget_ignores_duplicate_widget_additions() -> None:
    """重复添加同一页面时应保持兼容，不重复注册动画状态。"""
    ensure_qapp()
    stacked = TransparentStackedWidget()
    page = QWidget()

    first_index = stacked.addWidget(page)
    second_index = stacked.addWidget(page)

    assert first_index == 0
    assert second_index == 0
    assert stacked.count() == 1
