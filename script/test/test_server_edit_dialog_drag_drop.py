# -*- coding: utf-8 -*-
"""[`_PrivateKeyDropLineEdit`](src/ui/page/remote_page/server_edit_dialog.py) 拖拽测试 (P4 W1.F5.3).

走 ``evaluate_drop_paths`` 纯 Python 逻辑入口避免构造易崩的
``QDropEvent``; ``dropEvent`` 本身只是一层极薄的 url 提取 + 调用.
"""
from __future__ import annotations

# 标准库导入
import os
from pathlib import Path

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication


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


def _import_drop_line_edit():
    """旁路加载 ServerEditDialog 模块, 取出内部类."""
    from src.ui.page.remote_page.server_edit_dialog import _PrivateKeyDropLineEdit

    return _PrivateKeyDropLineEdit


# ==================== 命中规则 ====================
def test_accepts_id_rsa_filename(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    le = line_edit_cls()

    key_file = tmp_path / "id_rsa"
    key_file.write_bytes(b"binary content not pem")

    assert line_edit_cls._looks_like_private_key(key_file) is True


def test_accepts_pem_extension(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    key_file = tmp_path / "my_key.pem"
    key_file.write_bytes(b"any content")
    assert line_edit_cls._looks_like_private_key(key_file) is True


def test_accepts_ppk_extension(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    key_file = tmp_path / "putty.ppk"
    key_file.write_bytes(b"PuTTY-User-Key-File-2: ssh-rsa\n")
    assert line_edit_cls._looks_like_private_key(key_file) is True


def test_accepts_unnamed_file_with_pem_header(tmp_path: Path) -> None:
    """无后缀但首行是 ``-----BEGIN OPENSSH PRIVATE KEY-----`` 也接受."""
    line_edit_cls = _import_drop_line_edit()
    key_file = tmp_path / "untitled"
    key_file.write_bytes(b"-----BEGIN OPENSSH PRIVATE KEY-----\nbody\n")
    assert line_edit_cls._looks_like_private_key(key_file) is True


def test_rejects_random_text_file(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    txt = tmp_path / "notes.txt"
    txt.write_text("hello world", encoding="utf-8")
    assert line_edit_cls._looks_like_private_key(txt) is False


def test_rejects_directory(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    sub = tmp_path / "subdir"
    sub.mkdir()
    # is_file() 在目录上为 False; _looks_like_private_key 不会被调用,
    # 但 dropEvent 里有目录检测, 这里直接确认 looks_like 对目录返回 False
    assert line_edit_cls._looks_like_private_key(sub) is False


# ==================== evaluate_drop_paths ====================
def test_evaluate_accepts_single_pem_file(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    key_file = tmp_path / "my_key.pem"
    key_file.write_bytes(b"any content")

    accepted, hint = line_edit_cls.evaluate_drop_paths([str(key_file)])
    assert accepted == str(key_file)
    assert hint == ""


def test_evaluate_rejects_multiple_files(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    file_a = tmp_path / "id_rsa"
    file_b = tmp_path / "id_ed25519"
    file_a.write_bytes(b"x")
    file_b.write_bytes(b"y")

    accepted, hint = line_edit_cls.evaluate_drop_paths([str(file_a), str(file_b)])
    assert accepted is None
    assert "单个" in hint


def test_evaluate_rejects_empty_list() -> None:
    line_edit_cls = _import_drop_line_edit()
    accepted, hint = line_edit_cls.evaluate_drop_paths([])
    assert accepted is None
    assert hint != ""


def test_evaluate_rejects_directory(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    sub = tmp_path / "subdir"
    sub.mkdir()

    accepted, hint = line_edit_cls.evaluate_drop_paths([str(sub)])
    assert accepted is None
    assert "目录" in hint


def test_evaluate_rejects_unrecognized_file(tmp_path: Path) -> None:
    line_edit_cls = _import_drop_line_edit()
    txt = tmp_path / "notes.txt"
    txt.write_text("hello", encoding="utf-8")

    accepted, hint = line_edit_cls.evaluate_drop_paths([str(txt)])
    assert accepted is None
    assert "SSH 私钥" in hint


def test_evaluate_rejects_empty_local_path() -> None:
    """拖拽来源为 remote URL 时 ``toLocalFile`` 返回空串, 应拒绝."""
    line_edit_cls = _import_drop_line_edit()
    accepted, hint = line_edit_cls.evaluate_drop_paths([""])
    assert accepted is None
    assert "本地文件" in hint


def test_drop_line_edit_has_drops_enabled() -> None:
    """确保控件上 ``acceptDrops`` 已开启, 以便 Qt 底层接受拖拽."""
    line_edit_cls = _import_drop_line_edit()
    le = line_edit_cls()
    assert le.acceptDrops() is True
