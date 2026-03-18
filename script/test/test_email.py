# -*- coding: utf-8 -*-

# 标准库导入
import smtplib
from types import SimpleNamespace

# 第三方库导入
import pytest

# 项目内模块导入
import src.core.network.email as email_module


def patch_email_cfg(monkeypatch: pytest.MonkeyPatch) -> None:
    """为 email 模块注入稳定的测试配置。"""
    fake_cfg = SimpleNamespace(
        email_sender="email_sender",
        email_receiver="email_receiver",
        email_token="email_token",
        email_stmp_server="email_stmp_server",
        email_stmp_port="email_stmp_port",
        email_encryption="email_encryption",
    )
    values = {
        fake_cfg.email_sender: "sender@example.com",
        fake_cfg.email_receiver: "receiver@example.com",
        fake_cfg.email_token: "secret-token",
        fake_cfg.email_stmp_server: "smtp.example.com",
        fake_cfg.email_stmp_port: 465,
        fake_cfg.email_encryption: "SSL",
    }
    fake_cfg.get = lambda item: values[item]
    monkeypatch.setattr(email_module, "cfg", fake_cfg)
    monkeypatch.setattr(email_module.logger, "info", lambda *args, **kwargs: None)
    monkeypatch.setattr(email_module.logger, "exception", lambda *args, **kwargs: None)


def test_encryption_type_helpers_cover_all_declared_values() -> None:
    """加密枚举辅助方法应返回稳定的名称和值。"""
    assert email_module.EncryptionType.get_values() == ["SSL", "TLS", "无加密"]
    assert email_module.EncryptionType.get_names() == ["SSL", "TLS", "NONE"]
    assert email_module.EncryptionType.get_enum_by_value("TLS") == email_module.EncryptionType.TLS

    with pytest.raises(ValueError, match="No enum found"):
        email_module.EncryptionType.get_enum_by_value("SMTP")


def test_email_data_reads_dynamic_values_from_cfg(monkeypatch: pytest.MonkeyPatch) -> None:
    """EmailData 应从配置对象拉取发件参数。"""
    patch_email_cfg(monkeypatch)

    data = email_module.EmailData("<p>hello</p>", "subject")

    assert data.sender_email == "sender@example.com"
    assert data.reciver_email == "receiver@example.com"
    assert data.smtp_server == "smtp.example.com"
    assert data.smtp_port == 465
    assert data.encryption == email_module.EncryptionType.SSL
    assert data.msg_from == "NapCatQQ-Desktop Bot sender@example.com"
    assert data.msg_to == "receiver@example.com"


def test_email_run_uses_ssl_transport(monkeypatch: pytest.MonkeyPatch) -> None:
    """SSL 模式应通过 SMTP_SSL 登录并发送邮件。"""
    patch_email_cfg(monkeypatch)
    calls: list[tuple[str, object]] = []
    successes: list[str] = []

    class FakeSMTPSSL:
        def __init__(self, server: str, port: int) -> None:
            calls.append(("connect", server, port))

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb) -> None:
            return None

        def login(self, username: str, token: str) -> None:
            calls.append(("login", username, token))

        def sendmail(self, sender: str, receiver: str, payload: str) -> None:
            calls.append(("sendmail", sender, receiver, "Subject:"))
            assert "Subject:" in payload

    monkeypatch.setattr(email_module.smtplib, "SMTP_SSL", FakeSMTPSSL)

    task = email_module.Email(email_module.EmailData("<p>hello</p>", "subject"))
    task.success_signal.connect(successes.append)

    task.run()

    assert calls[0] == ("connect", "smtp.example.com", 465)
    assert ("login", "sender@example.com", "secret-token") in calls
    assert ("sendmail", "sender@example.com", "receiver@example.com", "Subject:") in calls
    assert successes == ["邮件发送成功"]


def test_email_run_uses_tls_transport(monkeypatch: pytest.MonkeyPatch) -> None:
    """TLS 模式应先 starttls 再发送邮件。"""
    patch_email_cfg(monkeypatch)
    events: list[str] = []

    fake_cfg = email_module.cfg
    fake_cfg.get = lambda item: {
        fake_cfg.email_sender: "sender@example.com",
        fake_cfg.email_receiver: "receiver@example.com",
        fake_cfg.email_token: "secret-token",
        fake_cfg.email_stmp_server: "smtp.example.com",
        fake_cfg.email_stmp_port: 587,
        fake_cfg.email_encryption: "TLS",
    }[item]

    class FakeSMTP:
        def __init__(self, server: str, port: int) -> None:
            assert (server, port) == ("smtp.example.com", 587)

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb) -> None:
            return None

        def starttls(self) -> None:
            events.append("starttls")

        def login(self, username: str, token: str) -> None:
            events.append("login")

        def sendmail(self, sender: str, receiver: str, payload: str) -> None:
            events.append("sendmail")

    monkeypatch.setattr(email_module.smtplib, "SMTP", FakeSMTP)

    email_module.Email(email_module.EmailData("<p>hello</p>", "subject")).run()

    assert events == ["starttls", "login", "sendmail"]


def test_email_run_emits_smtp_response_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """SMTPResponseException 应被转换为可见错误消息。"""
    patch_email_cfg(monkeypatch)
    errors: list[str] = []

    class FakeSMTPSSL:
        def __init__(self, server: str, port: int) -> None:
            return None

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb) -> None:
            return None

        def login(self, username: str, token: str) -> None:
            raise smtplib.SMTPResponseException(550, b"denied")

    monkeypatch.setattr(email_module.smtplib, "SMTP_SSL", FakeSMTPSSL)

    task = email_module.Email(email_module.EmailData("<p>hello</p>", "subject"))
    task.error_signal.connect(errors.append)

    task.run()

    assert errors == ["创建邮件时出现错误, 请检查是否发件成功: 550 b'denied'"]


def test_create_test_email_task_renders_template(monkeypatch: pytest.MonkeyPatch) -> None:
    """测试邮件任务应从模板生成 HTML 内容。"""
    patch_email_cfg(monkeypatch)

    class FakeBytes:
        def __init__(self, payload: bytes) -> None:
            self._payload = payload

        def data(self) -> bytes:
            return self._payload

    class FakeQFile:
        OpenModeFlag = SimpleNamespace(ReadOnly=1)

        def __init__(self, path: str) -> None:
            self.path = path

        def setOpenMode(self, mode) -> None:
            self.mode = mode

        def readAll(self) -> FakeBytes:
            return FakeBytes("<p>$disconnect_time</p>".encode("utf-8"))

        def close(self) -> None:
            self.closed = True

    monkeypatch.setattr(email_module, "QFile", FakeQFile)

    task = email_module.create_test_email_task()

    assert isinstance(task, email_module.Email)
    assert task.data.msg_subject == "测试 NapCatQQ-Desktop 发件功能"
    assert "<p>" in task.data.email_content


def test_create_offline_email_task_raises_when_template_is_empty(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """空模板内容不应创建离线通知邮件任务。"""
    patch_email_cfg(monkeypatch)

    class FakeQFluentFile:
        def __init__(self, path: str, mode) -> None:
            self.path = path

        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc_val, exc_tb) -> None:
            return None

        def readAll(self):
            return SimpleNamespace(data=lambda: b"")

    monkeypatch.setattr(email_module, "QFluentFile", FakeQFluentFile)

    with pytest.raises(ValueError, match="邮件内容不能为空"):
        email_module.create_offline_email_task(config_factory(445566, "Bot"))
