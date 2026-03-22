# -*- coding: utf-8 -*-

# 标准库导入
import json
import zipfile
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.logging.crash_bundle as crash_bundle_module
import src.core.logging.log_func as log_func_module
from src.core.logging.crash_bundle import (
    build_safe_config_summary,
    mask_email,
    sanitize_text_for_export,
    summarize_path,
    summarize_url,
)
from src.core.logging.log_func import Logger


def create_test_logger(log_path: Path) -> Logger:
    """创建写入指定路径的测试日志器。"""
    log_path.parent.mkdir(parents=True, exist_ok=True)
    test_logger = Logger()
    test_logger.load_config()
    test_logger.log_path = log_path
    test_logger.log_path.write_text("", encoding="utf-8")
    return test_logger


def create_runtime_config_fixture(repo_root: Path) -> dict[str, str]:
    """在临时仓库中写入带敏感数据的运行时配置。"""
    runtime_config_dir = repo_root / "runtime" / "config"
    runtime_config_dir.mkdir(parents=True, exist_ok=True)

    sensitive_values = {
        "email": "sensitive.user@example.com",
        "email_token": "email-token-secret",
        "webhook_url": "https://example.com/webhook?token=url-secret",
        "webhook_secret": "webhook-secret-token",
        "bot_token": "bot-token-secret",
        "qqid": "572381217",
        "bot_name": "SensitiveBot",
    }

    config_json = {
        "Event": {
            "BotOfflineEmailNotice": True,
            "BotOfflineWebHookNotice": True,
        },
        "General": {
            "CloseBtnAction": 0,
        },
        "Personalized": {
            "DpiScale": "Auto",
        },
        "Info": {
            "EulaAccepted": True,
            "MainWindow": True,
            "NCDVersion": "v1.7.28",
            "PlatformType": "AMD64",
            "StartTime": 1773750788.043942,
            "SystemType": "Windows",
        },
        "Personalize": {
            "ThemeMode": "Light",
            "WindowOpacity": 100,
        },
        "Email": {
            "EmailEncryption": "SSL",
            "EmailReceiver": sensitive_values["email"],
            "EmailSender": sensitive_values["email"],
            "EmailStmpPort": 465,
            "EmailStmpServer": "smtp.qq.com",
            "EmailToken": sensitive_values["email_token"],
        },
        "WebHook": {
            "WebHookJson": '{"token":"json-secret"}',
            "WebHookSecret": sensitive_values["webhook_secret"],
            "WebHookUrl": sensitive_values["webhook_url"],
        },
    }
    bot_json = [
        {
            "bot": {
                "name": sensitive_values["bot_name"],
                "QQID": int(sensitive_values["qqid"]),
                "autoRestartSchedule": {
                    "enable": True,
                    "time_unit": "h",
                    "duration": 1,
                },
                "offlineAutoRestart": False,
            },
            "connect": {
                "httpServers": [{"enable": True, "name": "http", "token": sensitive_values["bot_token"]}],
                "httpSseServers": [],
                "httpClients": [],
                "websocketServers": [],
                "websocketClients": [
                    {
                        "url": sensitive_values["webhook_url"],
                        "token": sensitive_values["bot_token"],
                    }
                ],
                "plugins": [{"name": "plugin-a"}],
            },
            "advanced": {
                "autoStart": False,
                "offlineNotice": True,
                "parseMultMsg": False,
                "enableLocalFile2Url": False,
                "fileLog": True,
                "consoleLog": False,
                "fileLogLevel": "debug",
                "consoleLogLevel": "info",
                "o3HookMode": 1,
            },
        }
    ]

    (runtime_config_dir / "config.json").write_text(
        json.dumps(config_json, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    (runtime_config_dir / "bot.json").write_text(json.dumps(bot_json, ensure_ascii=False, indent=2), encoding="utf-8")
    return sensitive_values


def test_sanitize_text_for_export_redacts_sensitive_values() -> None:
    """敏感文本导出前必须被严格脱敏。"""
    raw_text = (
        "Authorization: Bearer top-secret\n"
        "token=inline-secret\n"
        "Email: sensitive.user@example.com\n"
        'Webhook: {"secret":"webhook-secret","qq_id":572381217}\n'
        "WebHook URL: https://example.com/webhook?token=url-secret\n"
        "QQID: 572381217\n"
    )

    sanitized = sanitize_text_for_export(raw_text)

    assert "top-secret" not in sanitized
    assert "inline-secret" not in sanitized
    assert "sensitive.user@example.com" not in sanitized
    assert "https://example.com/webhook?token=url-secret" not in sanitized
    assert "572381217" not in sanitized
    assert "<redacted-secret>" in sanitized
    assert "<redacted-email>" in sanitized
    assert "<redacted-url>" in sanitized
    assert "***1217" in sanitized


def test_log_summary_helpers_keep_context_without_exposing_raw_values() -> None:
    """日志摘要辅助函数应保留调试上下文且避免输出完整敏感值。"""
    assert mask_email("sensitive.user@example.com") == "s***@example.com"
    assert summarize_url("https://example.com/webhook/path?token=secret") == "https://example.com/.../path"
    assert summarize_path("C:/Users/QIAO/Desktop/update.bat") == "...\\Desktop\\update.bat"


def test_build_safe_config_summary_excludes_sensitive_values(tmp_path: Path) -> None:
    """安全配置摘要不得包含原始敏感值。"""
    repo_root = tmp_path / "repo"
    sensitive_values = create_runtime_config_fixture(repo_root)

    summary = build_safe_config_summary(
        config_path=repo_root / "runtime" / "config" / "config.json",
        bot_config_path=repo_root / "runtime" / "config" / "bot.json",
    )
    serialized = json.dumps(summary, ensure_ascii=False)

    assert sensitive_values["email"] not in serialized
    assert sensitive_values["email_token"] not in serialized
    assert sensitive_values["webhook_url"] not in serialized
    assert sensitive_values["webhook_secret"] not in serialized
    assert sensitive_values["bot_token"] not in serialized
    assert sensitive_values["bot_name"] not in serialized
    assert sensitive_values["qqid"] not in serialized
    assert "***1217" in serialized
    assert summary["email"]["configured"] is True
    assert summary["webhook"]["configured"] is True
    assert summary["bots"][0]["connect_counts"]["plugins"] == 1


def test_emit_crash_bundle_writes_redacted_bundle(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """崩溃诊断包必须只包含脱敏后的派生文件。"""
    repo_root = tmp_path / "repo"
    data_root = tmp_path / "ProgramData" / "NapCatQQ Desktop"
    desktop_dir = tmp_path / "desktop"
    desktop_dir.mkdir(parents=True, exist_ok=True)
    sensitive_values = create_runtime_config_fixture(data_root)
    log_path = repo_root / "log" / "app.log"
    test_logger = create_test_logger(log_path)

    test_logger.info(
        (
            "WebHook debug leak "
            f"Authorization: Bearer {sensitive_values['webhook_secret']} "
            f"URL={sensitive_values['webhook_url']} "
            f"receiver={sensitive_values['email']} "
            f"QQID: {sensitive_values['qqid']}"
        )
    )

    monkeypatch.setattr(log_func_module, "resolve_app_base_path", lambda: repo_root)
    monkeypatch.setattr(log_func_module, "resolve_desktop_output_dir", lambda base_path=None: (desktop_dir, "desktop"))
    monkeypatch.setattr(crash_bundle_module, "resolve_app_data_path", lambda: data_root)

    try:
        raise RuntimeError(
            (
                f"fatal with token={sensitive_values['bot_token']} "
                f"url={sensitive_values['webhook_url']} "
                f"email={sensitive_values['email']} "
                f"QQID: {sensitive_values['qqid']}"
            )
        )
    except RuntimeError as exc:
        bundle_path = test_logger.emit_crash_bundle("test", exc, type(exc), exc.__traceback__)

    assert bundle_path is not None
    assert bundle_path.exists()

    with zipfile.ZipFile(bundle_path) as archive:
        names = set(archive.namelist())
        assert "crash_report.txt" in names
        assert "logs/app-session.redacted.log" in names
        assert "diagnostics/app_meta.json" in names
        assert "diagnostics/config_summary.json" in names
        assert "diagnostics/paths.json" in names

        bundle_text = "\n".join(
            archive.read(name).decode("utf-8", errors="replace")
            for name in archive.namelist()
            if not name.endswith("/")
        )

    for raw_value in sensitive_values.values():
        assert raw_value not in bundle_text

    assert "<redacted-secret>" in bundle_text
    assert "<redacted-email>" in bundle_text
    assert "<redacted-url>" in bundle_text
    assert "***1217" in bundle_text


def test_emit_crash_bundle_only_once_per_process(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """同一进程重复触发崩溃导出时只能生成一个 zip。"""
    repo_root = tmp_path / "repo"
    data_root = tmp_path / "ProgramData" / "NapCatQQ Desktop"
    desktop_dir = tmp_path / "desktop"
    desktop_dir.mkdir(parents=True, exist_ok=True)
    create_runtime_config_fixture(data_root)
    test_logger = create_test_logger(repo_root / "log" / "app.log")

    monkeypatch.setattr(log_func_module, "resolve_app_base_path", lambda: repo_root)
    monkeypatch.setattr(log_func_module, "resolve_desktop_output_dir", lambda base_path=None: (desktop_dir, "desktop"))
    monkeypatch.setattr(crash_bundle_module, "resolve_app_data_path", lambda: data_root)

    try:
        raise RuntimeError("first crash")
    except RuntimeError as exc:
        first_path = test_logger.emit_crash_bundle("test-first", exc, type(exc), exc.__traceback__)

    try:
        raise RuntimeError("second crash")
    except RuntimeError as exc:
        second_path = test_logger.emit_crash_bundle("test-second", exc, type(exc), exc.__traceback__)

    assert first_path is not None
    assert first_path == second_path
    assert len(list(desktop_dir.glob("NapCatQQ-Desktop-crash-*.zip"))) == 1


def test_emit_crash_bundle_publishes_user_notification(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """生成崩溃诊断包时应向 UI 广播提醒事件。"""
    repo_root = tmp_path / "repo"
    data_root = tmp_path / "ProgramData" / "NapCatQQ Desktop"
    desktop_dir = tmp_path / "desktop"
    desktop_dir.mkdir(parents=True, exist_ok=True)
    create_runtime_config_fixture(data_root)
    test_logger = create_test_logger(repo_root / "log" / "app.log")
    published_notifications: list[object] = []

    monkeypatch.setattr(log_func_module, "resolve_app_base_path", lambda: repo_root)
    monkeypatch.setattr(log_func_module, "resolve_desktop_output_dir", lambda base_path=None: (desktop_dir, "desktop"))
    monkeypatch.setattr(crash_bundle_module, "resolve_app_data_path", lambda: data_root)
    monkeypatch.setattr(
        log_func_module.crash_bundle_notification_center,
        "publish",
        lambda notification: published_notifications.append(notification),
    )

    try:
        raise RuntimeError("crash for notification")
    except RuntimeError as exc:
        bundle_path = test_logger.emit_crash_bundle("test-notice", exc, type(exc), exc.__traceback__)

    assert bundle_path is not None
    assert len(published_notifications) == 1
    notification = published_notifications[0]
    assert notification.bundle_path == bundle_path
    assert notification.trigger == "test-notice"
    assert notification.output_source == "desktop"


def test_emit_test_crash_bundle_does_not_consume_real_crash_export(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """手动测试导出不应占用正式崩溃诊断包的一次性名额。"""
    repo_root = tmp_path / "repo"
    data_root = tmp_path / "ProgramData" / "NapCatQQ Desktop"
    desktop_dir = tmp_path / "desktop"
    desktop_dir.mkdir(parents=True, exist_ok=True)
    create_runtime_config_fixture(data_root)
    test_logger = create_test_logger(repo_root / "log" / "app.log")

    monkeypatch.setattr(log_func_module, "resolve_app_base_path", lambda: repo_root)
    monkeypatch.setattr(log_func_module, "resolve_desktop_output_dir", lambda base_path=None: (desktop_dir, "desktop"))
    monkeypatch.setattr(crash_bundle_module, "resolve_app_data_path", lambda: data_root)

    test_bundle_path = test_logger.emit_test_crash_bundle()

    try:
        raise RuntimeError("real crash after manual export")
    except RuntimeError as exc:
        real_bundle_path = test_logger.emit_crash_bundle("test-real", exc, type(exc), exc.__traceback__)

    assert test_bundle_path is not None
    assert real_bundle_path is not None
    assert test_bundle_path != real_bundle_path
    assert len(list(desktop_dir.glob("NapCatQQ-Desktop-crash-*.zip"))) == 2


def test_emit_test_crash_bundle_does_not_publish_user_notification(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """手动测试导出不应触发异常崩溃提示。"""
    repo_root = tmp_path / "repo"
    data_root = tmp_path / "ProgramData" / "NapCatQQ Desktop"
    desktop_dir = tmp_path / "desktop"
    desktop_dir.mkdir(parents=True, exist_ok=True)
    create_runtime_config_fixture(data_root)
    test_logger = create_test_logger(repo_root / "log" / "app.log")
    published_notifications: list[object] = []

    monkeypatch.setattr(log_func_module, "resolve_app_base_path", lambda: repo_root)
    monkeypatch.setattr(log_func_module, "resolve_desktop_output_dir", lambda base_path=None: (desktop_dir, "desktop"))
    monkeypatch.setattr(crash_bundle_module, "resolve_app_data_path", lambda: data_root)
    monkeypatch.setattr(
        log_func_module.crash_bundle_notification_center,
        "publish",
        lambda notification: published_notifications.append(notification),
    )

    bundle_path = test_logger.emit_test_crash_bundle()

    assert bundle_path is not None
    assert published_notifications == []

