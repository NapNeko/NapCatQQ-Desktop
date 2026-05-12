# -*- coding: utf-8 -*-
"""[`HostKeyConfirmDialog`](src/ui/components/host_key_confirm_dialog.py) 单元测试 (P4 W1·F5.1).

覆盖:

- 三按钮分别更新 ``decision()`` 到对应 :class:`HostKeyDecision`
- 警告路径 (``is_warning=True``) 标题改红色
- ``HostKeyDialogBridge`` 主线程同步路径与跨线程阻塞路径
- ``bootstrap_host_key_dialog`` 注册到 ``register_host_key_callback`` 后,
  从 SSHClient 角度 ``get_registered_callback`` 返回桥的 ``prompt`` 方法
"""
from __future__ import annotations

# 标准库导入
import os
import threading
import time
from collections.abc import Callable

# 第三方库导入
import pytest
from PySide6.QtCore import QCoreApplication, QTimer
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
from src.core.remote.host_key_policy import (
    HostKeyDecision,
    HostKeyPrompt,
    get_registered_callback,
    register_host_key_callback,
)


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


@pytest.fixture(autouse=True)
def _reset_callback() -> None:
    """每个测试前后清空全局回调, 防止串扰."""
    register_host_key_callback(None)
    yield
    register_host_key_callback(None)


@pytest.fixture
def parent_widget() -> QWidget:
    """提供一个临时父 widget; ``MessageBoxBase`` 要求非 None 父节点."""
    w = QWidget()
    w.resize(800, 600)
    return w


@pytest.fixture
def sample_prompt() -> HostKeyPrompt:
    return HostKeyPrompt(
        hostname="example.com",
        port=22,
        key_type="ssh-ed25519",
        fingerprint_sha256="SHA256:AAAA1234567890abcdef",
        fingerprint_md5="aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99",
    )


# ==================== HostKeyConfirmDialog ====================
def test_dialog_decision_defaults_to_reject(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget)
    assert dialog.decision() is HostKeyDecision.REJECT


def test_dialog_trust_save_button_sets_decision(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget)
    dialog.yesButton.click()
    assert dialog.decision() is HostKeyDecision.TRUST_SAVE


def test_dialog_trust_once_button_sets_decision(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget)
    dialog._once_button.click()
    assert dialog.decision() is HostKeyDecision.TRUST_ONCE


def test_dialog_reject_button_sets_decision(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget)
    # 先点 TRUST_SAVE 把 decision 改掉, 再点 cancel 验证它能改回 REJECT
    dialog.yesButton.click()
    assert dialog.decision() is HostKeyDecision.TRUST_SAVE
    dialog.cancelButton.click()
    assert dialog.decision() is HostKeyDecision.REJECT


def test_dialog_warning_mode_uses_red_title(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """``is_warning=True`` 时标题用红色样式."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget, is_warning=True)
    assert "C42B1C" in dialog.title_label.styleSheet()


def test_dialog_renders_fingerprint_in_caption(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """对话框正文应包含主机名 + 端口."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(sample_prompt, parent=parent_widget)
    text = dialog.caption_label.text()
    assert "example.com" in text
    assert "22" in text


# ==================== HostKeyDialogBridge ====================
def test_bridge_prompt_on_main_thread_runs_synchronously(
    sample_prompt: HostKeyPrompt, monkeypatch: pytest.MonkeyPatch
) -> None:
    """主线程同步路径: ``prompt`` 直接弹窗, 不走 Event 阻塞."""
    from src.ui.components import host_key_confirm_dialog as dlg_mod

    # 替换对话框为 stub, 避免真实弹窗
    captured: list[bool] = []

    class _StubDialog:
        def __init__(self, *args, **kwargs) -> None:
            del args, kwargs

        def exec(self) -> bool:
            captured.append(True)
            return True

        def decision(self) -> HostKeyDecision:
            return HostKeyDecision.TRUST_SAVE

    monkeypatch.setattr(dlg_mod, "HostKeyConfirmDialog", _StubDialog)

    bridge = dlg_mod.HostKeyDialogBridge()
    decision = bridge.prompt(sample_prompt)

    assert captured == [True]
    assert decision is HostKeyDecision.TRUST_SAVE


def test_bridge_prompt_on_main_thread_returns_reject_when_dialog_rejected(
    sample_prompt: HostKeyPrompt, monkeypatch: pytest.MonkeyPatch
) -> None:
    from src.ui.components import host_key_confirm_dialog as dlg_mod

    class _StubDialog:
        def __init__(self, *args, **kwargs) -> None:
            del args, kwargs

        def exec(self) -> bool:
            return False  # 用户关闭对话框 / cancel

        def decision(self) -> HostKeyDecision:
            return HostKeyDecision.TRUST_SAVE  # 即便 stub 反咬, exec=False 路径应返回 REJECT

    monkeypatch.setattr(dlg_mod, "HostKeyConfirmDialog", _StubDialog)

    bridge = dlg_mod.HostKeyDialogBridge()
    assert bridge.prompt(sample_prompt) is HostKeyDecision.REJECT


def test_bridge_prompt_cross_thread_blocks_until_main_dispatch(
    sample_prompt: HostKeyPrompt, monkeypatch: pytest.MonkeyPatch
) -> None:
    """跨线程路径: worker 调 ``prompt`` 应阻塞直到主线程事件循环弹窗.

    本测试在工作线程调 ``prompt``, 主线程通过 ``processEvents`` 推进事件循环
    让 ``_on_request`` 槽执行. 用 stub 替换实际对话框避免渲染.
    """
    from src.ui.components import host_key_confirm_dialog as dlg_mod

    class _StubDialog:
        def __init__(self, *args, **kwargs) -> None:
            del args, kwargs

        def exec(self) -> bool:
            return True

        def decision(self) -> HostKeyDecision:
            return HostKeyDecision.TRUST_ONCE

    monkeypatch.setattr(dlg_mod, "HostKeyConfirmDialog", _StubDialog)

    bridge = dlg_mod.HostKeyDialogBridge(timeout_seconds=5.0)
    result_box: list[HostKeyDecision] = []

    def worker() -> None:
        # 强制走跨线程路径
        decision = bridge.prompt(sample_prompt)
        result_box.append(decision)

    t = threading.Thread(target=worker)
    t.start()

    # 主线程持续 processEvents, 驱动 _on_request 槽执行
    deadline = time.monotonic() + 5.0
    while t.is_alive() and time.monotonic() < deadline:
        QCoreApplication.processEvents()
        time.sleep(0.01)
    t.join(timeout=1.0)

    assert not t.is_alive(), "worker 应已结束"
    assert result_box == [HostKeyDecision.TRUST_ONCE]


def test_bridge_prompt_cross_thread_times_out_to_reject(
    sample_prompt: HostKeyPrompt, monkeypatch: pytest.MonkeyPatch
) -> None:
    """主线程不响应时, ``prompt`` 在 timeout 后返回 REJECT 而非永久挂死."""
    from src.ui.components import host_key_confirm_dialog as dlg_mod

    bridge = dlg_mod.HostKeyDialogBridge(timeout_seconds=0.5)

    # 强制 _is_main_thread 返回 False, 模拟跨线程; 同时不 processEvents,
    # 让 _on_request 永远不执行 -> bridge.prompt 应在 0.5s 后超时返回 REJECT
    monkeypatch.setattr(dlg_mod.HostKeyDialogBridge, "_is_main_thread", staticmethod(lambda: False))

    start = time.monotonic()
    decision = bridge.prompt(sample_prompt)
    elapsed = time.monotonic() - start

    assert decision is HostKeyDecision.REJECT
    assert 0.4 < elapsed < 3.0  # 在 timeout 量级内


# ==================== bootstrap ====================
def test_bootstrap_registers_callback() -> None:
    from src.ui.components.host_key_confirm_dialog import (
        bootstrap_host_key_dialog,
        reset_host_key_dialog_for_test,
    )

    assert get_registered_callback() is None
    bridge = bootstrap_host_key_dialog()
    # bound method 比较用 == (每次访问 `bridge.prompt` 生成不同对象, 但语义上相等)
    assert get_registered_callback() == bridge.prompt
    reset_host_key_dialog_for_test()
    assert get_registered_callback() is None


def test_bootstrap_is_idempotent() -> None:
    from src.ui.components.host_key_confirm_dialog import (
        bootstrap_host_key_dialog,
        reset_host_key_dialog_for_test,
    )

    first = bootstrap_host_key_dialog()
    second = bootstrap_host_key_dialog()
    assert first is second
    reset_host_key_dialog_for_test()


# ==================== Warning 路径 (主机指纹变更) ====================
@pytest.fixture
def change_prompt() -> HostKeyPrompt:
    """变更场景: 带 ``previous_*`` 字段的 prompt."""
    return HostKeyPrompt(
        hostname="example.com",
        port=22,
        key_type="ssh-ed25519",
        fingerprint_sha256="SHA256:NEW_aaabbbcccdddeeefff",
        fingerprint_md5="11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00",
        previous_key_type="ssh-ed25519",
        previous_fingerprint_sha256="SHA256:OLD_111222333444555666",
    )


def test_dialog_warning_yes_button_returns_trust_replace(
    change_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """``is_warning=True`` 时 yes 按钮决策应为 TRUST_REPLACE (而非 TRUST_SAVE)."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        change_prompt, parent=parent_widget, is_warning=True
    )
    dialog.yesButton.click()
    assert dialog.decision() is HostKeyDecision.TRUST_REPLACE


def test_dialog_warning_yes_button_text_is_replace(
    change_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """``is_warning=True`` 时 yes 按钮文案应为 "信任并替换"."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        change_prompt, parent=parent_widget, is_warning=True
    )
    assert dialog.yesButton.text() == "信任并替换"


def test_dialog_warning_hides_trust_once_button(
    change_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """``is_warning=True`` 路径不应显示 "仅本次" 按钮 (变更场景下无意义)."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        change_prompt, parent=parent_widget, is_warning=True
    )
    # warning 路径下 _once_button 应为 None
    assert dialog._once_button is None


def test_dialog_warning_renders_old_and_new_fingerprints(
    change_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """变更场景 dialog 应同时展示原指纹与新指纹."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        change_prompt, parent=parent_widget, is_warning=True
    )
    # 把对话框内所有可读文本聚合后做断言
    visible_text = ""
    for child in dialog.findChildren(object):
        text_attr = getattr(child, "text", None)
        if callable(text_attr):
            try:
                visible_text += str(text_attr()) + "\n"
            except Exception:  # noqa: BLE001
                pass
    assert "OLD_111222333444555666" in visible_text
    assert "NEW_aaabbbcccdddeeefff" in visible_text


def test_dialog_warning_reject_button_default(
    change_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """变更场景下用户点 reject 按钮应返 REJECT."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        change_prompt, parent=parent_widget, is_warning=True
    )
    dialog.cancelButton.click()
    assert dialog.decision() is HostKeyDecision.REJECT


def test_dialog_first_time_still_returns_trust_save(
    sample_prompt: HostKeyPrompt, parent_widget: QWidget
) -> None:
    """``is_warning=False`` (首次连接) 时 yes 按钮仍返 TRUST_SAVE (没改坏)."""
    from src.ui.components.host_key_confirm_dialog import HostKeyConfirmDialog

    dialog = HostKeyConfirmDialog(
        sample_prompt, parent=parent_widget, is_warning=False
    )
    dialog.yesButton.click()
    assert dialog.decision() is HostKeyDecision.TRUST_SAVE
    # 首次连接路径仍应显示 "仅本次" 按钮
    assert dialog._once_button is not None
