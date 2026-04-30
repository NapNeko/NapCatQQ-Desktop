# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
from types import ModuleType, SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget

sys.modules.setdefault("qrcode", ModuleType("qrcode"))
markdown_module = ModuleType("markdown")
markdown_module.markdown = lambda text, *args, **kwargs: text
sys.modules.setdefault("markdown", markdown_module)

# 项目内模块导入
import src.ui.page.bot_page.widget.msg_box as msg_box_module


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_add_qr_code_auto_shows_once() -> None:
    """新增二维码后应自动弹出一次，并记录对应 QQ。"""
    factory = msg_box_module.QRCodeDialogFactory()
    shown_calls: list[str | None] = []
    factory.show = lambda preferred_qq_id=None: shown_calls.append(preferred_qq_id)  # type: ignore[method-assign]

    factory.add_qr_code("2477817352", "https://example.com/qr")
    factory.add_qr_code("2477817352", "https://example.com/qr2")

    assert factory.qr_code_list == [{"qq_id": "2477817352", "qrcode_url": "https://example.com/qr2"}]
    assert shown_calls == ["2477817352"]


def test_cancelled_dialog_suppresses_future_auto_show_until_removed(monkeypatch: pytest.MonkeyPatch) -> None:
    """用户取消后，同一轮二维码更新不应再次自动弹出；移除后应恢复。"""
    ensure_qapp()
    exec_calls: list[str] = []

    class FakeDialog:
        def __init__(self, qr_code_list, parent) -> None:
            del parent
            self._qr_code_list = [info.copy() for info in qr_code_list]
            self.dismissed_by_user = True

        def set_current_qq_id(self, qq_id: str | None) -> None:
            exec_calls.append(f"focus:{qq_id}")

        def exec(self) -> int:
            exec_calls.append("exec")
            return msg_box_module.MessageBoxBase.DialogCode.Rejected

        def get_visible_qq_ids(self) -> list[str]:
            return [info["qq_id"] for info in self._qr_code_list]

        def update_qr_codes(self, qr_code_list, preferred_qq_id=None) -> None:
            del preferred_qq_id
            self._qr_code_list = [info.copy() for info in qr_code_list]

    monkeypatch.setattr(msg_box_module, "QRCodeDialog", FakeDialog)
    fake_main_window_module = ModuleType("src.ui.window.main_window")
    fake_main_window_module.MainWindow = type("MainWindow", (), {})
    monkeypatch.setitem(sys.modules, "src.ui.window.main_window", fake_main_window_module)
    monkeypatch.setattr(
        msg_box_module,
        "it",
        lambda cls: {"MainWindow": object()}[cls.__name__],
    )

    factory = msg_box_module.QRCodeDialogFactory()

    factory.add_qr_code("2477817352", "https://example.com/qr")
    factory.add_qr_code("2477817352", "https://example.com/qr2")

    assert exec_calls == ["focus:2477817352", "exec"]

    factory.remove_qr_code("2477817352")
    factory.add_qr_code("2477817352", "https://example.com/qr3")

    assert exec_calls == ["focus:2477817352", "exec", "focus:2477817352", "exec"]


def test_qr_code_dialog_accept_refreshes_current_login_state(monkeypatch: pytest.MonkeyPatch) -> None:
    """确认按钮应改为刷新二维码，而不是关闭弹窗。"""
    ensure_qapp()
    refresh_calls: list[str] = []
    success_messages: list[str] = []

    class FakeLoginState:
        def slot_request_auth_refresh(self) -> None:
            refresh_calls.append("refresh_auth")

        def slot_get_login_state(self) -> None:
            refresh_calls.append("refresh_qr")

    fake_manager = SimpleNamespace(get_login_state=lambda qq_id: FakeLoginState() if qq_id == "2477817352" else None)

    def fake_add_qr_code(self, qq_id: str, qrcode_url: str) -> None:
        del qrcode_url
        page = QWidget(self.view)
        page.setObjectName(qq_id)
        self.view.addWidget(page)

    monkeypatch.setattr(msg_box_module.QRCodeDialog, "add_qr_code", fake_add_qr_code)
    monkeypatch.setattr(
        msg_box_module,
        "it",
        lambda cls: {"ManagerNapCatQQLoginState": fake_manager}[cls.__name__],
    )
    monkeypatch.setattr(
        msg_box_module,
        "success_bar",
        lambda content, *args, **kwargs: success_messages.append(content),
    )

    parent = QWidget()
    dialog = msg_box_module.QRCodeDialog(
        [{"qq_id": "2477817352", "qrcode_url": "https://example.com/qr"}],
        parent,
    )

    assert dialog.yesButton.text() == "刷新二维码"
    assert dialog.cancelButton.text() == "取消"

    dialog.accept()

    assert refresh_calls == ["refresh_auth", "refresh_qr"]
    assert success_messages == ["已请求刷新二维码，如二维码仍无效请尝试重启 NapCat"]
    assert dialog.result() == 0
