# -*- coding: utf-8 -*-

# 标准库导入
import json

# 第三方库导入
import pytest

# 项目内模块导入
import src.desktop.core.network.webhook as webhook_module
from src.desktop.core.config.config_model import (
    AdvancedConfig,
    AutoRestartScheduleConfig,
    BotConfig,
    Config,
    ConnectConfig,
)
from src.desktop.core.network.webhook import WebHook, WebHookData, create_offline_webhook_task, create_test_webhook_task


def make_config(qqid: int = 114514, name: str = "WebhookBot") -> Config:
    """构造 WebHook 测试用 Bot 配置。"""
    return Config(
        bot=BotConfig(
            name=name,
            QQID=qqid,
            musicSignUrl="",
            autoRestartSchedule=AutoRestartScheduleConfig(enable=False, time_unit="h", duration=1),
            offlineAutoRestart=False,
        ),
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[],
            websocketClients=[],
            plugins=[],
        ),
        advanced=AdvancedConfig(
            autoStart=False,
            offlineNotice=True,
            parseMultMsg=False,
            packetServer="",
            enableLocalFile2Url=False,
            fileLog=True,
            consoleLog=False,
            fileLogLevel="debug",
            consoleLogLevel="info",
            o3HookMode=1,
        ),
    )


def patch_cfg_get(monkeypatch: pytest.MonkeyPatch, overrides: dict[str, str]) -> None:
    """将 cfg.get 定向到测试数据。"""

    def fake_get(item):
        return overrides.get(item.key, "")

    monkeypatch.setattr(webhook_module.cfg, "get", fake_get)


def test_create_test_webhook_task_sends_parsed_payload(monkeypatch: pytest.MonkeyPatch) -> None:
    """测试 WebHook 请求体应以解析后的 JSON 对象发送。"""
    user_config_json = '{"text": "Test from user config"}'
    patch_cfg_get(
        monkeypatch,
        {
            webhook_module.cfg.web_hook_url.key: "https://example.com/webhook",
            webhook_module.cfg.web_hook_secret.key: "secret-token",
            webhook_module.cfg.web_hook_json.key: user_config_json,
        },
    )

    captured: dict[str, object] = {}

    class FakeResponse:
        status_code = 200

        @staticmethod
        def raise_for_status() -> None:
            return None

    def fake_post(url, *, json=None, headers=None, timeout=None):
        captured["url"] = url
        captured["json"] = json
        captured["headers"] = headers
        captured["timeout"] = timeout
        return FakeResponse()

    monkeypatch.setattr(webhook_module.httpx, "post", fake_post)

    task = create_test_webhook_task()
    task.run()

    assert captured["url"] == "https://example.com/webhook"
    assert captured["json"] == json.loads(user_config_json)
    assert not isinstance(captured["json"], str)
    assert task.data.json == json.dumps(json.loads(user_config_json), indent=4, ensure_ascii=False)


def test_create_offline_webhook_task_renders_template_and_sends_object(monkeypatch: pytest.MonkeyPatch) -> None:
    """离线通知应渲染模板后以 JSON 对象发送。"""
    patch_cfg_get(
        monkeypatch,
        {
            webhook_module.cfg.web_hook_url.key: "https://example.com/offline",
            webhook_module.cfg.web_hook_secret.key: "offline-secret",
            webhook_module.cfg.web_hook_json.key: '{"bot":"${bot_name}","qq":"${bot_qq_id}","time":"${disconnect_time}"}',
        },
    )

    captured: dict[str, object] = {}

    class FakeResponse:
        status_code = 200

        @staticmethod
        def raise_for_status() -> None:
            return None

    def fake_post(url, *, json=None, headers=None, timeout=None):
        captured["url"] = url
        captured["json"] = json
        captured["headers"] = headers
        captured["timeout"] = timeout
        return FakeResponse()

    monkeypatch.setattr(webhook_module.httpx, "post", fake_post)

    config = make_config(qqid=2477817352, name="OfflineBot")
    task = create_offline_webhook_task(config)
    task.run()

    assert captured["url"] == "https://example.com/offline"
    assert captured["json"]["bot"] == "OfflineBot"
    assert captured["json"]["qq"] == "2477817352"
    assert "disconnect_time" not in captured["json"]
    assert not isinstance(captured["json"], str)


def test_invalid_json_emits_error_and_skips_send(monkeypatch: pytest.MonkeyPatch) -> None:
    """无效 JSON 时不应发起请求。"""
    monkeypatch.setattr(
        webhook_module.httpx,
        "post",
        lambda *args, **kwargs: pytest.fail("无效 JSON 不应触发 httpx.post"),
    )

    emitted: list[str] = []
    task = WebHook(WebHookData(url="https://example.com/webhook", json="{invalid"))
    task.error_signal.connect(emitted.append)
    task.run()

    assert emitted == ["无效的 JSON 内容"]


def test_webhook_get_request_sends_params(monkeypatch: pytest.MonkeyPatch) -> None:
    """测试 GET 请求应将解析后的 JSON 作为 params 发送。"""
    user_config_json = '{"device_key": "abc123", "title": "Test"}'
    patch_cfg_get(
        monkeypatch,
        {
            webhook_module.cfg.web_hook_url.key: "https://example.com/webhook",
            webhook_module.cfg.web_hook_secret.key: "secret-token",
            webhook_module.cfg.web_hook_json.key: user_config_json,
            webhook_module.cfg.web_hook_method.key: "GET",
        },
    )

    captured: dict[str, object] = {}

    class FakeResponse:
        status_code = 200

        @staticmethod
        def raise_for_status() -> None:
            return None

    def fake_get(url, *, params=None, headers=None, timeout=None):
        captured["url"] = url
        captured["params"] = params
        captured["headers"] = headers
        captured["timeout"] = timeout
        return FakeResponse()

    monkeypatch.setattr(webhook_module.httpx, "get", fake_get)
    # 确保 post 不会被调用
    monkeypatch.setattr(
        webhook_module.httpx,
        "post",
        lambda *args, **kwargs: pytest.fail("GET 请求不应触发 httpx.post"),
    )

    task = create_test_webhook_task()
    task.run()

    assert captured["url"] == "https://example.com/webhook"
    assert captured["params"] == json.loads(user_config_json)
    assert captured["headers"] == {
        "Authorization": "Bearer secret-token",
        "Content-Type": "application/json",
    }
    assert captured["timeout"] == 10.0
