# -*- coding: utf-8 -*-
"""[`MigrationDialog`](src/ui/page/bot_page/widget/migration_dialog.py) "搬运持久数据" 勾选位单测 (P4 W3 F6).

仅覆盖最关键的两点:

1. 默认状态: ``move_persistent_data_checkbox.isChecked() == True`` (P4 兑现 future flag)
2. ``get_move_persistent_data()`` 与 checkbox 状态一致 (toggle 后取值随之变化)

不验证 UI 文案 / 布局; 那些由 W4 手动验收清单覆盖.
"""
from __future__ import annotations

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget


@pytest.fixture(scope="module")
def qapp() -> QApplication:
    """模块级 QApplication; 多个 dialog 测试共享, 减少创建开销."""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app  # type: ignore[return-value]


def _build_dialog(parent: QWidget) -> "object":
    # 延迟 import: dialog 模块依赖 qfluentwidgets, 在无 QApplication 时 import 即崩
    from src.ui.page.bot_page.widget.migration_dialog import MigrationDialog

    return MigrationDialog(
        qq_id="114514",
        source_label="本地 (Windows)",
        dest_label="srv-A (Linux)",
        parent=parent,
    )


def _build_dialog_snowluma(parent: QWidget) -> "object":
    """构造 SnowLuma 后端类型的迁移对话框, 验证文案切换."""
    from src.core.runtime.backend_type import BackendType
    from src.ui.page.bot_page.widget.migration_dialog import MigrationDialog

    return MigrationDialog(
        qq_id="114514",
        source_label="本地 (Windows)",
        dest_label="srv-B (Linux SL)",
        parent=parent,
        backend_type=BackendType.SNOWLUMA,
    )


def test_persistent_data_checkbox_default_checked(qapp: QApplication) -> None:
    parent = QWidget()
    try:
        dialog = _build_dialog(parent)
        # P4 W3 F6: 默认勾选
        assert dialog.move_persistent_data_checkbox.isChecked() is True
        assert dialog.get_move_persistent_data() is True
    finally:
        parent.deleteLater()


def test_persistent_data_getter_reflects_toggle(qapp: QApplication) -> None:
    parent = QWidget()
    try:
        dialog = _build_dialog(parent)
        dialog.move_persistent_data_checkbox.setChecked(False)
        assert dialog.get_move_persistent_data() is False
        dialog.move_persistent_data_checkbox.setChecked(True)
        assert dialog.get_move_persistent_data() is True
    finally:
        parent.deleteLater()
