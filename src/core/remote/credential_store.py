# -*- coding: utf-8 -*-
"""[`CredentialStore`](src/core/remote/credential_store.py): SSH 密码 keyring 抽象 (P4 F5.2).

设计目标
--------

P3 之前, 用户在 [`ServerEditDialog`](src/ui/page/remote_page/server_edit_dialog.py)
勾选"记住密码"后, 密码以明文落到 ``ServerProfile`` 配置文件 (
``%APPDATA%/NapCatQQ-Desktop/...``). 这与 §6.2 安全基线冲突.

P4 F5.2 让密码存到系统 keyring (Windows: Credential Manager), 配置文件中
仅留 ``password_source: "keyring"`` 标记. 仅支持 Windows; 非 Windows 平台
``CredentialStore`` 自动 ``is_available() -> False``, ServerEditDialog 应
据此隐藏 "记住密码" 勾选项.

关键约束
--------

- **不**修改 ``SSHCredentials`` 数据形状; 上层调用 ``store_password`` /
  ``load_password`` 自行决定何时读 keyring 何时读配置.
- ``store_password`` 返回 ``bool`` 而非抛异常: 写失败 (例如用户禁用了 Credential
  Manager) 时直接降级到配置文件存储, UI 层用 InfoBar 提示一次即可.
- 模块对 ``keyring`` import 失败鲁棒 (开发环境 / CI 容器可能缺包),
  导致 ``is_available() -> False``; 测试以 ``KeyringDisabled`` 哨兵覆盖该路径.
- 不在本模块写日志; 由调用方决定日志级别.
"""
from __future__ import annotations

# 标准库导入
import sys
from typing import Final


_DEFAULT_NAMESPACE: Final[str] = "napcat-desktop:ssh"


class _KeyringUnavailable(Exception):
    """内部用: keyring 不可用时短路抛出, 由 ``CredentialStore`` 包成 ``False`` 返回."""


def _try_import_keyring():
    """运行期 import ``keyring``; 失败时返回 None.

    分离出独立函数, 便于测试以 monkeypatch 注入"keyring 不可用"场景.
    """
    try:
        import keyring  # type: ignore[import-not-found]

        return keyring
    except Exception:  # noqa: BLE001 - 任何 import 链异常都视为不可用
        return None


def _is_windows() -> bool:
    """当前是否运行在 Windows; 抽出来方便测试 monkeypatch."""
    return sys.platform == "win32"


class CredentialStore:
    """对 ``keyring`` 库的薄封装, 仅在 Windows 启用.

    用法::

        store = CredentialStore()
        if store.is_available():
            store.store_password("server-uuid", "p@ss")
            pw = store.load_password("server-uuid")
            store.delete_password("server-uuid")
    """

    def __init__(self, *, namespace: str = _DEFAULT_NAMESPACE) -> None:
        """构造一个 keyring 包装实例.

        Args:
            namespace: keyring service name (Credential Manager 中显示为 "Internet 或网络地址").
        """
        self._namespace = namespace
        self._keyring = _try_import_keyring() if _is_windows() else None

    # ==================== 可用性 ====================
    def is_available(self) -> bool:
        """当前环境是否可以使用 keyring.

        Returns:
            ``True`` 当且仅当: 运行在 Windows + 成功 import keyring +
            ``keyring.get_keyring()`` 不抛异常 + 不是 ``fail.Keyring`` 哨兵.
        """
        if self._keyring is None:
            return False
        try:
            backend = self._keyring.get_keyring()
        except Exception:  # noqa: BLE001
            return False
        # keyring.backends.fail.Keyring 是 keyring 在所有后端都不可用时返回的哨兵,
        # 调用其 set/get 会抛 ``RuntimeError("No recommended backend was available.")``.
        # 仅看 ``__module__`` 是否落在 ``keyring.backends.fail`` 即可识别该哨兵
        # (无论实际类名是 ``Keyring`` 还是测试 fake 重命名).
        backend_module = (type(backend).__module__ or "").lower()
        if backend_module.endswith("backends.fail") or backend_module == "keyring.backends.fail":
            return False
        return True

    @property
    def namespace(self) -> str:
        """当前 keyring service name (主要用于测试与诊断)."""
        return self._namespace

    # ==================== CRUD ====================
    def store_password(self, server_id: str, password: str) -> bool:
        """把 ``password`` 写到 keyring.

        Args:
            server_id: 通常是 [`ServerProfile.id`](src/core/remote/servers.py)
                (UUID4); 作为 keyring username.
            password: 明文密码 (内部由 keyring 后端按平台规则加密落盘).

        Returns:
            ``True`` 写入成功; ``False`` 表示 keyring 不可用或写入抛错,
            调用方应回退到配置文件存储并 InfoBar 提示一次.
        """
        if not server_id:
            return False
        if not self.is_available():
            return False
        try:
            self._keyring.set_password(self._namespace, server_id, password or "")
        except Exception:  # noqa: BLE001
            return False
        return True

    def load_password(self, server_id: str) -> str | None:
        """从 keyring 读 ``server_id`` 的密码.

        Returns:
            字符串 (可能为空串); ``None`` 表示 keyring 不可用 / 没有该条目 /
            读取异常. 调用方区分"密码不存在"和"keyring 不可用"应同时检查
            ``is_available()`` + 这里返回值.
        """
        if not server_id:
            return None
        if not self.is_available():
            return None
        try:
            return self._keyring.get_password(self._namespace, server_id)
        except Exception:  # noqa: BLE001
            return None

    def delete_password(self, server_id: str) -> bool:
        """从 keyring 删除 ``server_id`` 的密码.

        Returns:
            ``True`` 表示删除完成 (条目原本存在或已经不存在都算成功);
            ``False`` 表示 keyring 不可用或删除时抛了非 ``PasswordDeleteError`` 的异常.
        """
        if not server_id:
            return False
        if not self.is_available():
            return False
        try:
            self._keyring.delete_password(self._namespace, server_id)
        except Exception as exc:  # noqa: BLE001
            # keyring 删除不存在的条目会抛 PasswordDeleteError, 视为 idempotent 成功.
            # 用 endswith 让测试 fake (``_FakePasswordDeleteError``) 也能命中.
            cls_name = type(exc).__name__
            if cls_name.endswith("PasswordDeleteError"):
                return True
            return False
        return True


__all__: tuple[str, ...] = ("CredentialStore",)
