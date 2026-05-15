# -*- coding: utf-8 -*-
"""ServerEditDialog "记住密码" 勾选项 + ServerManager keyring 集成测试 (P4 W1.F5.2).

不依赖真实 Windows Credential Manager: 所有 keyring 调用走自定义 fake.
"""
from __future__ import annotations

# 标准库导入
import os
from pathlib import Path

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
from src.core.remote import credential_store as cs_mod
from src.core.remote.credential_store import CredentialStore
from src.core.remote.models import SSHCredentials
from src.core.remote.server_manager import ServerManager
from src.core.remote.servers import ServerProfile


# ==================== fixtures ====================
def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


# ==================== fake keyring ====================
class _FakeBackend:
    __module__ = "keyring.backends.Windows"


class _FakeKeyringModule:
    def __init__(self) -> None:
        self._db: dict[tuple[str, str], str] = {}

    def get_keyring(self) -> object:
        return _FakeBackend()

    def set_password(self, service: str, username: str, password: str) -> None:
        self._db[(service, username)] = password

    def get_password(self, service: str, username: str) -> str | None:
        return self._db.get((service, username))

    def delete_password(self, service: str, username: str) -> None:
        if (service, username) not in self._db:
            class _PasswordDeleteError(Exception):
                pass
            raise _PasswordDeleteError("not found")
        del self._db[(service, username)]


@pytest.fixture
def fake_keyring(monkeypatch: pytest.MonkeyPatch) -> _FakeKeyringModule:
    """注入一个工作正常的 fake keyring + 强制走 Windows 分支."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule()
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)
    return fake


@pytest.fixture
def parent_widget() -> QWidget:
    w = QWidget()
    w.resize(800, 600)
    return w


def _make_password_profile() -> ServerProfile:
    return ServerProfile.create(
        name="test-server",
        credentials=SSHCredentials(
            host="example.com",
            username="root",
            auth_method="password",
            password=None,
        ),
    )


# ==================== ServerEditDialog ====================
def test_dialog_hides_remember_check_when_keyring_unavailable(
    parent_widget: QWidget, monkeypatch: pytest.MonkeyPatch
) -> None:
    """非 Windows / keyring 缺失时 "记住密码" 应整行隐藏 + disabled."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: False)

    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget)
    # 切到密码模式
    pwd_idx = dialog.method_combo.findData("password")
    dialog.method_combo.setCurrentIndex(pwd_idx)

    assert dialog.remember_check.isEnabled() is False
    # 行可见性: not is_key (True) and keyring_available (False) -> False
    assert dialog._auth_form.isRowVisible(dialog._auth_row_remember) is False


def test_dialog_shows_remember_check_when_keyring_available_and_password_mode(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget, credential_store=CredentialStore())
    # 切到密码模式
    pwd_idx = dialog.method_combo.findData("password")
    dialog.method_combo.setCurrentIndex(pwd_idx)

    assert dialog.remember_check.isEnabled() is True
    assert dialog._auth_form.isRowVisible(dialog._auth_row_remember) is True


def test_dialog_hides_remember_check_in_key_mode(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    """私钥模式下不应显示 "记住密码"."""
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget, credential_store=CredentialStore())
    # 默认就是 key 模式
    assert dialog.method_combo.currentData() == "key"
    assert dialog._auth_form.isRowVisible(dialog._auth_row_remember) is False


def test_wants_remember_password_false_in_key_mode(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget, credential_store=CredentialStore())
    dialog.remember_check.setChecked(True)  # 即便强制勾选, key 模式也应返回 False
    assert dialog.wants_remember_password() is False


def test_wants_remember_password_true_in_password_mode_with_check(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget, credential_store=CredentialStore())
    pwd_idx = dialog.method_combo.findData("password")
    dialog.method_combo.setCurrentIndex(pwd_idx)
    dialog.remember_check.setChecked(True)
    assert dialog.wants_remember_password() is True


def test_wants_remember_password_false_when_unchecked(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    dialog = ServerEditDialog(parent=parent_widget, credential_store=CredentialStore())
    pwd_idx = dialog.method_combo.findData("password")
    dialog.method_combo.setCurrentIndex(pwd_idx)
    # 未勾选
    assert dialog.wants_remember_password() is False


def test_dialog_loads_existing_remember_flag(
    parent_widget: QWidget, fake_keyring: _FakeKeyringModule
) -> None:
    """编辑模式下若 ``existing_remember_password=True`` 应默认勾选."""
    from src.ui.page.remote_page.server_edit_dialog import ServerEditDialog

    profile = _make_password_profile()
    dialog = ServerEditDialog(
        parent=parent_widget,
        profile=profile,
        existing_password="oldpass",
        existing_remember_password=True,
        credential_store=CredentialStore(),
    )
    assert dialog.remember_check.isChecked() is True


# ==================== ServerManager keyring 集成 ====================
def test_add_server_with_remember_writes_to_keyring(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    mgr.add_server(profile, password="p@ss", remember_password=True)

    # 内存缓存
    assert mgr.has_password(profile.id) is True
    # keyring 持久化
    assert mgr.has_remembered_password(profile.id) is True
    assert fake_keyring.get_password("napcat-desktop:ssh", profile.id) == "p@ss"


def test_add_server_without_remember_does_not_write_keyring(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    mgr.add_server(profile, password="p@ss", remember_password=False)

    # 内存有, keyring 没有
    assert mgr.has_password(profile.id) is True
    assert mgr.has_remembered_password(profile.id) is False


def test_update_server_remember_true_writes_keyring(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    mgr.add_server(profile, password="oldpass", remember_password=False)
    assert mgr.has_remembered_password(profile.id) is False

    mgr.update_server(profile, password="newpass", remember_password=True)
    assert mgr.has_remembered_password(profile.id) is True
    assert fake_keyring.get_password("napcat-desktop:ssh", profile.id) == "newpass"


def test_update_server_remember_false_deletes_keyring(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    mgr.add_server(profile, password="p@ss", remember_password=True)
    assert mgr.has_remembered_password(profile.id) is True

    mgr.update_server(profile, password="p@ss", remember_password=False)
    assert mgr.has_remembered_password(profile.id) is False


def test_remove_server_clears_keyring(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    mgr.add_server(profile, password="p@ss", remember_password=True)
    assert fake_keyring.get_password("napcat-desktop:ssh", profile.id) == "p@ss"

    mgr.remove_server(profile.id)
    assert fake_keyring.get_password("napcat-desktop:ssh", profile.id) is None


def test_preload_passwords_from_keyring_on_init(
    tmp_path: Path, fake_keyring: _FakeKeyringModule
) -> None:
    """模拟 "重启 Desktop": 第一次 init 写入 keyring, 第二次 init 应从 keyring 预加载."""
    storage = tmp_path / "servers.json"

    # 第一次 init
    mgr1 = ServerManager(storage_path=storage, credential_store=CredentialStore())
    profile = _make_password_profile()
    mgr1.add_server(profile, password="remembered", remember_password=True)

    # 第二次 init: 模拟 Desktop 重启
    mgr2 = ServerManager(storage_path=storage, credential_store=CredentialStore())
    # ServerRegistry 已从 servers.json 加载档案; ServerManager.__init__ 走 _preload
    assert mgr2.has_password(profile.id) is True
    cred = mgr2.get_runtime_credentials(profile.id)
    assert cred is not None
    assert cred.password == "remembered"


def test_preload_skips_when_keyring_unavailable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """keyring 不可用时启动不抛, _preload 静默跳过."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: False)

    storage = tmp_path / "servers.json"
    mgr = ServerManager(storage_path=storage, credential_store=CredentialStore())

    profile = _make_password_profile()
    # remember_password 即便传 True 也无效, 不抛
    mgr.add_server(profile, password="p@ss", remember_password=True)
    assert mgr.has_password(profile.id) is True
    assert mgr.has_remembered_password(profile.id) is False
