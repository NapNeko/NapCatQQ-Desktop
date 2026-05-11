# -*- coding: utf-8 -*-
"""SnowLuma 适配 P7.2: snowluma_config_renderer 单测.

参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §4.2
"""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

from src.core.config.config_model import (
    ConnectConfig,
    HttpServersConfig,
    WebsocketServersConfig,
)
from src.core.runtime.snowluma_config_renderer import (
    read_existing_onebot_json,
    render_onebot_json,
    render_runtime_json,
    render_webui_json,
)


def _make_default_connect(token: str = "") -> ConnectConfig:
    """构造一个仅有默认 http-default + ws-default 两个 server 的 ConnectConfig.

    与 P1 老 signature ``render_onebot_json(snowluma_path, qqid, access_token=token)``
    的等价语义: 单 HTTP server (port=3000) + 单 WS server (port=3001), 都用同一 token.
    """
    return ConnectConfig(
        httpServers=[
            HttpServersConfig(
                name="http-default",
                host="0.0.0.0",
                port=3000,
                token=token,
            )
        ],
        websocketServers=[
            WebsocketServersConfig(
                name="ws-default",
                host="0.0.0.0",
                port=3001,
                token=token,
            )
        ],
    )


# 上游真实样本路径 (用于 golden 比对; 仅在该路径存在时跑); CI 环境可能跳过
_UPSTREAM_SAMPLE_PATH = Path(r"C:\Users\QIAO\Desktop\SnowLuma-v1.7.5-win-x64\config")


# ==================== render_runtime_json ====================
class TestRenderRuntimeJson:
    def test_default_webui_port(self, tmp_path: Path) -> None:
        render_runtime_json(tmp_path)
        target = tmp_path / "config" / "runtime.json"
        assert target.exists()
        payload = json.loads(target.read_text(encoding="utf-8"))
        assert payload == {"webuiPort": 5099}

    def test_custom_webui_port(self, tmp_path: Path) -> None:
        render_runtime_json(tmp_path, webui_port=12345)
        payload = json.loads((tmp_path / "config" / "runtime.json").read_text(encoding="utf-8"))
        assert payload == {"webuiPort": 12345}


# ==================== render_onebot_json (P2 W3 重构后) ====================
class TestRenderOnebotJson:
    def test_filename_uses_qqid(self, tmp_path: Path) -> None:
        # W3: connect 全空走 fallback (与 SnowLuma makeDefaultOneBotConfig 等价)
        render_onebot_json(tmp_path, 10000, connect=ConnectConfig())
        assert (tmp_path / "config" / "onebot_10000.json").exists()

    def test_default_structure_matches_upstream_shape(self, tmp_path: Path) -> None:
        # W3: 用显式 ConnectConfig 携带单 HTTP + 单 WS server, 与老 access_token="abc" 等价
        render_onebot_json(tmp_path, 10001, connect=_make_default_connect(token="abc"))
        payload = json.loads(
            (tmp_path / "config" / "onebot_10001.json").read_text(encoding="utf-8")
        )

        assert "networks" in payload
        nets = payload["networks"]
        assert isinstance(nets["httpServers"], list) and len(nets["httpServers"]) == 1
        assert isinstance(nets["wsServers"], list) and len(nets["wsServers"]) == 1
        assert nets["httpClients"] == []
        assert nets["wsClients"] == []

        http = nets["httpServers"][0]
        assert http["port"] == 3000
        assert http["accessToken"] == "abc"
        assert http["messageFormat"] == "array"
        assert http["host"] == "0.0.0.0"
        assert http["path"] == "/"
        assert http["reportSelfMessage"] is False

        ws = nets["wsServers"][0]
        assert ws["port"] == 3001
        assert ws["accessToken"] == "abc"
        assert ws["role"] == "Universal"

        assert payload["musicSignUrl"] == ""

    def test_custom_ports_and_token(self, tmp_path: Path) -> None:
        # W3: 老的标量参数 (http_port / ws_port / message_format / report_self_message)
        # 都从 ConnectConfig 字段映射. 用户只填 1 个 HTTP server (port=8080) +
        # 1 个 WS server (port=9090) + token="custom-token" + messageFormat="string" +
        # reportSelfMessage=True.
        connect = ConnectConfig(
            httpServers=[
                HttpServersConfig(
                    name="http-custom",
                    host="0.0.0.0",
                    port=8080,
                    token="custom-token",
                    messagePostFormat="string",
                )
            ],
            websocketServers=[
                WebsocketServersConfig(
                    name="ws-custom",
                    host="0.0.0.0",
                    port=9090,
                    token="custom-token",
                    reportSelfMessage=True,
                )
            ],
        )
        render_onebot_json(
            tmp_path,
            10002,
            connect=connect,
            music_sign_url="https://example.com/music",
        )
        payload = json.loads(
            (tmp_path / "config" / "onebot_10002.json").read_text(encoding="utf-8")
        )
        assert payload["networks"]["httpServers"][0]["port"] == 8080
        assert payload["networks"]["wsServers"][0]["port"] == 9090
        assert payload["networks"]["httpServers"][0]["accessToken"] == "custom-token"
        assert payload["networks"]["wsServers"][0]["accessToken"] == "custom-token"
        assert payload["networks"]["httpServers"][0]["messageFormat"] == "string"
        # reportSelfMessage 在 W3 实现中: HTTP server 占位 False (上游 SnowLuma httpServers
        # 没有 reportSelfMessage 字段, 渲染器仅为保留 P1 上游样本结构而填 False).
        # WS server 透传, 这里设了 True.
        assert payload["networks"]["wsServers"][0]["reportSelfMessage"] is True
        assert payload["musicSignUrl"] == "https://example.com/music"

    def test_invalid_qqid_raises(self, tmp_path: Path) -> None:
        # W3: connect 仍为必填关键字, qqid 校验逻辑不变
        with pytest.raises(ValueError):
            render_onebot_json(tmp_path, 0, connect=ConnectConfig())
        with pytest.raises(ValueError):
            render_onebot_json(tmp_path, -10, connect=ConnectConfig())

    def test_napcat_only_fields_silently_dropped(self, tmp_path: Path) -> None:
        """P2 (Tier B): NapCat-only 字段 (debug / enableCors / enableWebsocket /
        enableForcePushEvent / heartInterval) 在渲染到 SnowLuma JSON 时静默丢弃.
        """
        connect = ConnectConfig(
            httpServers=[
                HttpServersConfig(
                    name="http-test",
                    host="0.0.0.0",
                    port=4000,
                    token="ACCESS-TEST",
                    debug=True,
                    enableCors=True,
                    enableWebsocket=True,
                    path="/api",
                )
            ],
        )
        render_onebot_json(tmp_path, 88888, connect=connect)
        payload = json.loads(
            (tmp_path / "config" / "onebot_88888.json").read_text(encoding="utf-8")
        )
        http = payload["networks"]["httpServers"][0]
        # NapCat-only 必须不写入
        assert "debug" not in http
        assert "enableCors" not in http
        assert "enableWebsocket" not in http
        # SnowLuma 独有字段必须写入
        assert http["path"] == "/api"
        assert http["accessToken"] == "ACCESS-TEST"

    def test_reconnect_interval_clamps_below_1000ms(self, tmp_path: Path) -> None:
        """P2 (Tier B): wsClient.reconnectInterval < 1000 ms 应被 clamp 到 1000 ms,
        与 SnowLuma 上游 ``packages/core/src/onebot/config.ts:299`` 行为一致.
        """
        from src.core.config.config_model import WebsocketClientsConfig

        connect = ConnectConfig(
            websocketClients=[
                WebsocketClientsConfig(
                    name="wsc-low-reconnect",
                    url="ws://localhost:8080",
                    reconnectInterval=500,
                )
            ],
        )
        render_onebot_json(tmp_path, 77777, connect=connect)
        payload = json.loads(
            (tmp_path / "config" / "onebot_77777.json").read_text(encoding="utf-8")
        )
        ws_client = payload["networks"]["wsClients"][0]
        assert ws_client["reconnectIntervalMs"] == 1000

    def test_fallback_when_servers_empty(self, tmp_path: Path) -> None:
        """P2 (Tier B): connect.httpServers / websocketServers 都为空时, 自动兜底
        与 ``makeDefaultOneBotConfig()`` 等价的默认值 (port=3000/3001 + 随机 token).
        """
        render_onebot_json(tmp_path, 99999, connect=ConnectConfig())
        payload = json.loads(
            (tmp_path / "config" / "onebot_99999.json").read_text(encoding="utf-8")
        )
        assert payload["networks"]["httpServers"][0]["port"] == 3000
        assert payload["networks"]["wsServers"][0]["port"] == 3001
        # accessToken 是随机生成, 但不应为空
        assert len(payload["networks"]["httpServers"][0]["accessToken"]) > 0
        assert len(payload["networks"]["wsServers"][0]["accessToken"]) > 0

    @pytest.mark.skipif(
        not _UPSTREAM_SAMPLE_PATH.exists(),
        reason="上游样本目录不存在 (CI 环境跳过)",
    )
    def test_golden_match_with_upstream_sample(self, tmp_path: Path) -> None:
        """与上游真实样本 onebot_2550419068.json 结构对齐 (顶层键 + 网络分组结构)."""
        upstream = _UPSTREAM_SAMPLE_PATH / "onebot_2550419068.json"
        if not upstream.exists():
            pytest.skip("上游样本文件不存在")

        # W3: 用 fallback 路径 (空 ConnectConfig) 走出与上游样本同形结构
        render_onebot_json(tmp_path, 2550419068, connect=ConnectConfig())
        rendered = json.loads(
            (tmp_path / "config" / "onebot_2550419068.json").read_text(encoding="utf-8")
        )
        sample = json.loads(upstream.read_text(encoding="utf-8"))

        # 顶层键完全一致 (sample 与 rendered 都应有 networks / musicSignUrl)
        assert set(rendered.keys()) == set(sample.keys())
        # networks 子键一致
        assert set(rendered["networks"].keys()) == set(sample["networks"].keys())
        # P2 (W3): rendered 必须**包含** sample 的所有字段; 但允许 rendered 多写
        # ``enabled`` (与 SnowLuma 上游 TypeScript ``NetworkConfigBase.enabled`` 对齐;
        # 上游真实样本里这个字段在某些用户态被裸序列化时可能缺失, 比如手动改过
        # webui.json 后 SnowLuma 没有重写). 因此走 issubset 而非完全相等.
        assert set(sample["networks"]["httpServers"][0].keys()).issubset(
            set(rendered["networks"]["httpServers"][0].keys())
        )
        assert set(sample["networks"]["wsServers"][0].keys()).issubset(
            set(rendered["networks"]["wsServers"][0].keys())
        )


# ==================== render_webui_json ====================
class TestRenderWebuiJson:
    def test_password_none_does_not_create_file(self, tmp_path: Path) -> None:
        render_webui_json(tmp_path)
        assert not (tmp_path / "config" / "webui.json").exists()

    def test_password_none_does_not_overwrite_existing(self, tmp_path: Path) -> None:
        config_dir = tmp_path / "config"
        config_dir.mkdir(parents=True)
        target = config_dir / "webui.json"
        existing = '{"existing": true}'
        target.write_text(existing, encoding="utf-8")

        render_webui_json(tmp_path)  # password=None
        assert target.read_text(encoding="utf-8") == existing

    def test_password_set_writes_scrypt_hash(self, tmp_path: Path) -> None:
        render_webui_json(tmp_path, password="secret", must_change=True)
        target = tmp_path / "config" / "webui.json"
        assert target.exists()
        payload = json.loads(target.read_text(encoding="utf-8"))

        assert "passwordHash" in payload and isinstance(payload["passwordHash"], str)
        assert len(payload["passwordHash"]) == 128  # scrypt keylen=64 -> 128 hex chars
        assert "passwordSalt" in payload and len(payload["passwordSalt"]) == 32  # 16 bytes -> 32 hex
        assert payload["mustChangePassword"] is True
        assert "generatedAt" in payload
        assert "updatedAt" in payload

    def test_empty_password_raises(self, tmp_path: Path) -> None:
        with pytest.raises(ValueError):
            render_webui_json(tmp_path, password="")


# ==================== read_existing_onebot_json ====================
class TestReadExistingOnebotJson:
    def test_returns_none_when_missing(self, tmp_path: Path) -> None:
        assert read_existing_onebot_json(tmp_path, 99999) is None

    def test_returns_dict_when_exists(self, tmp_path: Path) -> None:
        render_onebot_json(tmp_path, 12345, connect=_make_default_connect(token="t"))
        payload = read_existing_onebot_json(tmp_path, 12345)
        assert isinstance(payload, dict)
        assert payload["networks"]["httpServers"][0]["accessToken"] == "t"

    def test_returns_none_on_corrupted_json(self, tmp_path: Path) -> None:
        config_dir = tmp_path / "config"
        config_dir.mkdir(parents=True)
        (config_dir / "onebot_42.json").write_text("not valid json {", encoding="utf-8")
        assert read_existing_onebot_json(tmp_path, 42) is None
