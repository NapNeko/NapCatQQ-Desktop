# -*- coding: utf-8 -*-

# 标准库导入
import json
import os

# 第三方库导入
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
import src.ui.page.setup_page.sub_page.general as general_module
from src.ui.page.setup_page.sub_page.general import BotOfflineEmailDialog, BotOfflineWebHookDialog


class DummySignal:
    """最小信号替身。"""

    def __init__(self) -> None:
        self.callbacks = []

    def connect(self, callback) -> None:
        self.callbacks.append(callback)


class DummyTask:
    """最小任务替身。"""

    def __init__(self) -> None:
        self.success_signal = DummySignal()
        self.error_signal = DummySignal()


class DummyThreadPool:
    """记录 start 调用的线程池替身。"""

    def __init__(self) -> None:
        self.started = []

    def start(self, task) -> None:
        self.started.append(task)


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def create_dialog_parent() -> QWidget:
    """创建供 MessageBoxBase 使用的父级窗口。"""
    ensure_qapp()
    parent = QWidget()
    parent.resize(800, 600)
    return parent


def patch_cfg_storage(monkeypatch, initial_values: dict[str, object]) -> dict[str, object]:
    """将 cfg.get/cfg.set 定向到内存存储。"""
    store = initial_values.copy()

    def fake_get(item):
        return store.get(item.key, getattr(item, "default", ""))

    def fake_set(item, value, *args, **kwargs) -> None:
        store[item.key] = value

    monkeypatch.setattr(general_module.cfg, "get", fake_get)
    monkeypatch.setattr(general_module.cfg, "set", fake_set)
    monkeypatch.setattr(general_module, "success_bar", lambda *args, **kwargs: None)
    monkeypatch.setattr(general_module, "warning_bar", lambda *args, **kwargs: None)
    monkeypatch.setattr(general_module, "error_bar", lambda *args, **kwargs: None)
    return store


def test_email_dialog_invalid_port_does_not_save_or_start_task(monkeypatch) -> None:
    """邮件端口非法时不应保存，也不应启动测试任务。"""
    ensure_qapp()
    store = patch_cfg_storage(
        monkeypatch,
        {
            general_module.cfg.bot_offline_email_notice.key: True,
            general_module.cfg.email_receiver.key: "old_receiver@example.com",
            general_module.cfg.email_sender.key: "old_sender@example.com",
            general_module.cfg.email_token.key: "old-token",
            general_module.cfg.email_stmp_server.key: "smtp.qq.com",
            general_module.cfg.email_stmp_port.key: 465,
            general_module.cfg.email_encryption.key: "SSL",
        },
    )

    fake_pool = DummyThreadPool()
    monkeypatch.setattr(general_module.QThreadPool, "globalInstance", staticmethod(lambda: fake_pool))
    monkeypatch.setattr(
        general_module,
        "create_test_email_task",
        lambda: (_ for _ in ()).throw(AssertionError("保存失败时不应创建测试邮件任务")),
    )

    parent = create_dialog_parent()
    dialog = BotOfflineEmailDialog(parent)
    dialog.stmp_server_port_card.fill_value("not-a-number")

    original_store = store.copy()

    assert dialog.save_config() is False
    assert store == original_store

    dialog._send_test_email()
    assert fake_pool.started == []


def test_email_dialog_valid_save_allows_test_task(monkeypatch) -> None:
    """邮件配置合法时应保存并启动测试任务。"""
    ensure_qapp()
    store = patch_cfg_storage(
        monkeypatch,
        {
            general_module.cfg.bot_offline_email_notice.key: False,
            general_module.cfg.email_receiver.key: "",
            general_module.cfg.email_sender.key: "",
            general_module.cfg.email_token.key: "",
            general_module.cfg.email_stmp_server.key: "",
            general_module.cfg.email_stmp_port.key: 465,
            general_module.cfg.email_encryption.key: "SSL",
        },
    )

    fake_pool = DummyThreadPool()
    monkeypatch.setattr(general_module.QThreadPool, "globalInstance", staticmethod(lambda: fake_pool))
    monkeypatch.setattr(general_module, "create_test_email_task", lambda: DummyTask())

    parent = create_dialog_parent()
    dialog = BotOfflineEmailDialog(parent)
    dialog.enable_card.fill_value(True)
    dialog.receivers_card.fill_value("new_receiver@example.com")
    dialog.sender_card.fill_value("new_sender@example.com")
    dialog.token_card.fill_value("new-token")
    dialog.stmp_server_card.fill_value("smtp.example.com")
    dialog.stmp_server_port_card.fill_value("587")
    dialog.encryption_card.fill_value("TLS")

    assert dialog.save_config() is True
    assert store[general_module.cfg.email_stmp_port.key] == 587
    assert store[general_module.cfg.email_encryption.key] == "TLS"

    dialog._send_test_email()
    assert len(fake_pool.started) == 1


def test_webhook_dialog_invalid_json_does_not_save_or_start_task(monkeypatch) -> None:
    """WebHook JSON 非法时不应保存，也不应启动测试任务。"""
    ensure_qapp()
    store = patch_cfg_storage(
        monkeypatch,
        {
            general_module.cfg.bot_offline_web_hook_notice.key: True,
            general_module.cfg.web_hook_url.key: "https://old.example.com/webhook",
            general_module.cfg.web_hook_secret.key: "old-secret",
            general_module.cfg.web_hook_json.key: '{"msg":"old"}',
            general_module.cfg.web_hook_method.key: "POST",
        },
    )

    fake_pool = DummyThreadPool()
    monkeypatch.setattr(general_module.QThreadPool, "globalInstance", staticmethod(lambda: fake_pool))
    monkeypatch.setattr(
        general_module,
        "create_test_webhook_task",
        lambda: (_ for _ in ()).throw(AssertionError("保存失败时不应创建测试 WebHook 任务")),
    )

    parent = create_dialog_parent()
    dialog = BotOfflineWebHookDialog(parent)
    dialog.json_card.json_text_edit.setPlainText("{invalid")

    original_store = store.copy()

    assert dialog.save_config() is False
    assert store == original_store

    dialog._send_test_webhook()
    assert fake_pool.started == []


def test_webhook_dialog_valid_save_allows_test_task(monkeypatch) -> None:
    """WebHook 配置合法时应保存并启动测试任务。"""
    ensure_qapp()
    store = patch_cfg_storage(
        monkeypatch,
        {
            general_module.cfg.bot_offline_web_hook_notice.key: False,
            general_module.cfg.web_hook_url.key: "",
            general_module.cfg.web_hook_secret.key: "",
            general_module.cfg.web_hook_json.key: '{"msg":"old"}',
            general_module.cfg.web_hook_method.key: "POST",
        },
    )

    fake_pool = DummyThreadPool()
    monkeypatch.setattr(general_module.QThreadPool, "globalInstance", staticmethod(lambda: fake_pool))
    monkeypatch.setattr(general_module, "create_test_webhook_task", lambda: DummyTask())

    parent = create_dialog_parent()
    dialog = BotOfflineWebHookDialog(parent)
    dialog.enable_card.fill_value(True)
    dialog.webhook_url_card.fill_value("https://example.com/webhook")
    dialog.webhook_secret_card.fill_value("new-secret")
    dialog.json_card.fill_value('{"msg":"new"}')

    assert dialog.save_config() is True
    assert store[general_module.cfg.web_hook_url.key] == "https://example.com/webhook"
    assert json.loads(store[general_module.cfg.web_hook_json.key]) == {"msg": "new"}

    dialog._send_test_webhook()
    assert len(fake_pool.started) == 1


def test_webhook_dialog_open_with_empty_json_does_not_log_parse_error(monkeypatch) -> None:
    """打开 WebHook 配置弹窗时，空 JSON 配置不应被当成非法 JSON 记录错误。"""
    ensure_qapp()
    patch_cfg_storage(
        monkeypatch,
        {
            general_module.cfg.bot_offline_web_hook_notice.key: False,
            general_module.cfg.web_hook_url.key: "",
            general_module.cfg.web_hook_secret.key: "",
            general_module.cfg.web_hook_json.key: "",
            general_module.cfg.web_hook_method.key: "POST",
        },
    )

    from src.ui.components.code_editor import editor as editor_module

    error_logs: list[str] = []
    monkeypatch.setattr(editor_module.logger, "error", lambda message: error_logs.append(message))

    parent = create_dialog_parent()
    dialog = BotOfflineWebHookDialog(parent)

    assert dialog.json_card.json_text_edit.toPlainText() == ""
    assert error_logs == []
