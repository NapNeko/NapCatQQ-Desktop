# -*- coding: utf-8 -*-
"""[`CredentialStore`](src/core/remote/credential_store.py) 单元测试 (P4 W1.F5.2).

覆盖:

- Windows + 真实 keyring 模拟: store / load / delete 闭环
- ``is_available`` 在 keyring import 失败时为 False
- 非 Windows 平台 ``is_available`` 为 False (即使 keyring 已 import)
- ``fail.Keyring`` 哨兵后端: ``is_available`` 为 False
- ``store_password`` 失败时返回 False (不抛)
- ``delete_password`` 对不存在条目走 idempotent 成功路径

测试不依赖系统真实 Credential Manager: 通过自定义假 keyring 模块替换
``CredentialStore._keyring``, 在内存 dict 中模拟 keyring 状态.
"""
from __future__ import annotations

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.remote import credential_store as cs_mod
from src.core.remote.credential_store import CredentialStore


# ==================== 假 keyring 实现 ====================
class _FakeKeyringBackend:
    """模拟 ``keyring.backends.Windows.WinVaultKeyring`` 一类后端的最小接口."""

    __module__ = "keyring.backends.Windows"

    def __init__(self) -> None:
        self.store: dict[tuple[str, str], str] = {}


class _FakeFailBackend:
    """模拟 ``keyring.backends.fail.Keyring`` 兜底后端 (无可用后端)."""

    __module__ = "keyring.backends.fail"


class _FakePasswordDeleteError(Exception):
    """模拟 ``keyring.errors.PasswordDeleteError``."""


class _FakeKeyringModule:
    """模拟整个 ``keyring`` package: 提供 ``get_keyring`` + 三件 CRUD."""

    def __init__(
        self,
        *,
        backend: object,
        store_raises: BaseException | None = None,
        delete_raises: BaseException | None = None,
        get_raises: BaseException | None = None,
    ) -> None:
        self._backend = backend
        self._store_raises = store_raises
        self._delete_raises = delete_raises
        self._get_raises = get_raises
        self._db: dict[tuple[str, str], str] = {}

    def get_keyring(self) -> object:
        return self._backend

    def set_password(self, service: str, username: str, password: str) -> None:
        if self._store_raises is not None:
            raise self._store_raises
        self._db[(service, username)] = password

    def get_password(self, service: str, username: str) -> str | None:
        if self._get_raises is not None:
            raise self._get_raises
        return self._db.get((service, username))

    def delete_password(self, service: str, username: str) -> None:
        if self._delete_raises is not None:
            raise self._delete_raises
        if (service, username) not in self._db:
            raise _FakePasswordDeleteError("not found")
        del self._db[(service, username)]


# ==================== fixtures ====================
@pytest.fixture
def fake_keyring(monkeypatch: pytest.MonkeyPatch) -> _FakeKeyringModule:
    """注入一个工作正常的 fake keyring + 强制走 Windows 分支."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(backend=_FakeKeyringBackend())
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)
    return fake


# ==================== 可用性 ====================
def test_is_available_true_on_windows_with_working_keyring(fake_keyring: _FakeKeyringModule) -> None:
    store = CredentialStore()
    assert store.is_available() is True


def test_is_available_false_when_not_windows(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: False)
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: _FakeKeyringModule(backend=_FakeKeyringBackend()))
    store = CredentialStore()
    assert store.is_available() is False


def test_is_available_false_when_keyring_import_fails(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: None)
    store = CredentialStore()
    assert store.is_available() is False


def test_is_available_false_with_fail_sentinel_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    """``keyring.backends.fail.Keyring`` 兜底时视为不可用."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(backend=_FakeFailBackend())
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)
    store = CredentialStore()
    assert store.is_available() is False


def test_is_available_false_when_get_keyring_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)

    class _BoomKeyring:
        def get_keyring(self):
            raise RuntimeError("backend probe failed")

    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: _BoomKeyring())
    store = CredentialStore()
    assert store.is_available() is False


# ==================== CRUD 闭环 ====================
def test_store_load_delete_roundtrip(fake_keyring: _FakeKeyringModule) -> None:
    store = CredentialStore()

    assert store.store_password("srv-1", "p@ss") is True
    assert store.load_password("srv-1") == "p@ss"
    assert store.delete_password("srv-1") is True
    assert store.load_password("srv-1") is None


def test_store_password_with_empty_string(fake_keyring: _FakeKeyringModule) -> None:
    """允许保存空串 (用户主动清空 → 仍占位为空)."""
    store = CredentialStore()
    assert store.store_password("srv-2", "") is True
    assert store.load_password("srv-2") == ""


def test_store_password_with_empty_server_id_returns_false(fake_keyring: _FakeKeyringModule) -> None:
    store = CredentialStore()
    assert store.store_password("", "anything") is False


def test_load_password_with_empty_server_id_returns_none(fake_keyring: _FakeKeyringModule) -> None:
    store = CredentialStore()
    assert store.load_password("") is None


# ==================== 失败路径 ====================
def test_store_password_returns_false_when_keyring_unavailable(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: None)
    store = CredentialStore()
    assert store.store_password("srv-x", "p") is False
    assert store.load_password("srv-x") is None
    assert store.delete_password("srv-x") is False


def test_store_password_returns_false_when_set_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(
        backend=_FakeKeyringBackend(),
        store_raises=RuntimeError("vault locked"),
    )
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)

    store = CredentialStore()
    assert store.is_available() is True  # 后端可用
    assert store.store_password("srv-x", "p") is False  # 但 set 抛错被吞


def test_load_password_returns_none_when_get_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(
        backend=_FakeKeyringBackend(),
        get_raises=RuntimeError("vault locked"),
    )
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)

    store = CredentialStore()
    assert store.load_password("srv-x") is None


def test_delete_password_idempotent_for_missing_entry(fake_keyring: _FakeKeyringModule) -> None:
    """删除不存在的条目走 idempotent 成功路径 (PasswordDeleteError 被吞)."""
    store = CredentialStore()
    assert store.delete_password("never-stored") is True


def test_delete_password_returns_false_on_unexpected_error(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(
        backend=_FakeKeyringBackend(),
        delete_raises=RuntimeError("io error"),
    )
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)

    store = CredentialStore()
    assert store.delete_password("srv-x") is False


# ==================== 命名空间隔离 ====================
def test_namespace_isolation(monkeypatch: pytest.MonkeyPatch) -> None:
    """两个不同 namespace 的 store 互不可见."""
    monkeypatch.setattr(cs_mod, "_is_windows", lambda: True)
    fake = _FakeKeyringModule(backend=_FakeKeyringBackend())
    monkeypatch.setattr(cs_mod, "_try_import_keyring", lambda: fake)

    store_a = CredentialStore(namespace="napcat-desktop:ssh")
    store_b = CredentialStore(namespace="napcat-desktop:other")

    assert store_a.store_password("shared-id", "pw-a")
    assert store_b.store_password("shared-id", "pw-b")
    assert store_a.load_password("shared-id") == "pw-a"
    assert store_b.load_password("shared-id") == "pw-b"
    assert store_a.namespace == "napcat-desktop:ssh"
