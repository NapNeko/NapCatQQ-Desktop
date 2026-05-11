# -*- coding: utf-8 -*-
"""SnowLuma 适配 P2: SnowLuma 配置渲染器.

Desktop 是 Bot 配置的事实源 (SOT), 在 SnowLuma 进程启动**之前**把 Desktop 的 BotConfig
渲染成 SnowLuma 自身期望的若干个 JSON 文件:

- ``<snowluma_path>/config/runtime.json`` — 全局 (webuiPort)
- ``<snowluma_path>/config/webui.json``  — WebUI 登录密码 (scrypt hash; 默认让 SnowLuma 自治)
- ``<snowluma_path>/config/onebot_<uin>.json`` — 每 Bot 的 OneBot HTTP/WS 端口与 accessToken

启动后 Desktop **不再**写这些文件, 让 SnowLuma 自己的 ``saveJson`` 落地用户在 WebUI
里改的运行时变更, 避免双写冲突.

参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §2.2.

上游真实样本 (1:1 参照):
``C:\\Users\\QIAO\\Desktop\\SnowLuma-v1.7.5-win-x64\\config\\*.json``

上游密码哈希算法 (从 ``server-CgTFTARm.js:2469-2541`` 反编译确认):
``scrypt(password, salt, keylen=64, N=16384, r=8, p=1)``; salt = ``randomBytes(16)``;
hash 与 salt 均以 ``hex`` 编码持久化.
"""
from __future__ import annotations

import hashlib
import json
import secrets
from datetime import datetime, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any, Final

from src.core.logging import LogSource, logger

if TYPE_CHECKING:
    from src.core.config.config_model import (
        ConnectConfig,
        HttpClientsConfig,
        HttpServersConfig,
        WebsocketClientsConfig,
        WebsocketServersConfig,
    )


# ==================== scrypt 参数 (与 SnowLuma 上游对齐) ====================
_SCRYPT_KEYLEN = 64
_SCRYPT_N = 16384
_SCRYPT_R = 8
_SCRYPT_P = 1
_SCRYPT_SALT_BYTES = 16
# Python 默认 maxmem=32 MiB; N*r*128 = 16384*8*128 = 16 MiB, 默认范围内.
# 显式设置保证未来上游调参时仍可工作.
_SCRYPT_MAXMEM = 64 * 1024 * 1024


# ==================== runtime.json ====================
def render_runtime_json(snowluma_path: Path, *, webui_port: int = 5099) -> None:
    """写入 ``<snowluma_path>/config/runtime.json``.

    上游样本结构: ``{"webuiPort": 5099}`` (仅一个键).

    Args:
        snowluma_path: SnowLuma 安装根目录 (即 :attr:`PathFunc.snowluma_path`).
        webui_port: WebUI 监听端口, 默认 5099 与上游一致.
    """
    config_dir = snowluma_path / "config"
    config_dir.mkdir(parents=True, exist_ok=True)
    target = config_dir / "runtime.json"
    payload: dict[str, Any] = {"webuiPort": int(webui_port)}
    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


# ==================== webui.json ====================
def render_webui_json(
    snowluma_path: Path,
    *,
    password: str | None = None,
    must_change: bool = False,
) -> None:
    """写入 / 维持 ``<snowluma_path>/config/webui.json``.

    设计原则 (与需求 §10 推断假设第 3 条对齐):

    - ``password is None`` 且 ``webui.json`` **存在**: 什么都不做, 保留用户在 WebUI 里改过的密码.
    - ``password is None`` 且 ``webui.json`` **不存在**: 什么都不做, 让 SnowLuma 自首次启动时
      使用随机 ``initialPassword`` 自治生成 (``server-CgTFTARm.js:2586``).
    - ``password`` 非空: 用 scrypt 重新生成 hash + 16 字节 salt 写入, 等价于上游 ``setPassword``
      流程; 用户后续可凭 ``password`` 登录 WebUI. ``must_change=True`` 表示登录后强制改密.

    Args:
        snowluma_path: SnowLuma 安装根目录.
        password: 明文密码 (UTF-8). ``None`` 表示不动文件.
        must_change: 是否设置 ``mustChangePassword=True``.

    Note:
        参数顺序与字段命名严格匹配上游 ``server-CgTFTARm.js:2469-2541`` 的 scrypt 实现.
        若上游算法变更需同步调整本模块.
    """
    config_dir = snowluma_path / "config"
    target = config_dir / "webui.json"

    if password is None:
        # 不主动创建; 让 SnowLuma 自治.
        return

    if not isinstance(password, str) or not password:
        raise ValueError("password 为空字符串无意义; 传 None 让 SnowLuma 自治, 或传非空字符串显式重置")

    salt_bytes = secrets.token_bytes(_SCRYPT_SALT_BYTES)
    hash_bytes = hashlib.scrypt(
        password.encode("utf-8"),
        salt=salt_bytes,
        n=_SCRYPT_N,
        r=_SCRYPT_R,
        p=_SCRYPT_P,
        dklen=_SCRYPT_KEYLEN,
        maxmem=_SCRYPT_MAXMEM,
    )

    now_iso = _utc_now_iso()
    payload: dict[str, Any] = {
        "passwordHash": hash_bytes.hex(),
        "passwordSalt": salt_bytes.hex(),
        "mustChangePassword": bool(must_change),
        "generatedAt": now_iso,
        "updatedAt": now_iso,
    }

    config_dir.mkdir(parents=True, exist_ok=True)
    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


# ==================== onebot_<uin>.json ====================
# P2 (Tier B): SnowLuma 上游 reconnectIntervalMs 强制 max(1000, value),
# 见 example/SnowLuma-main/packages/core/src/onebot/config.ts:299. Desktop 在 renderer
# 层同步 clamp + warn, 让用户 (在 NapCat 模式下保留原值不变, 切到 SnowLuma 渲染时被 clamp)
# 感知到上游限制.
_SNOWLUMA_MIN_RECONNECT_MS: Final = 1000


def _clamp_reconnect_ms(value: int, *, name: str) -> int:
    """SnowLuma 上游强制 max(1000, value), Desktop 同步 clamp 并 logger.warning."""
    if value < _SNOWLUMA_MIN_RECONNECT_MS:
        logger.warning(
            (
                f"用户配置 reconnectInterval={value}ms (wsClient name={name!r}) 被 clamp 到 "
                f"{_SNOWLUMA_MIN_RECONNECT_MS}ms; SnowLuma 上游 "
                "(packages/core/src/onebot/config.ts:299) 强制下限."
            ),
            log_source=LogSource.CORE,
        )
        return _SNOWLUMA_MIN_RECONNECT_MS
    return value


def _render_http_server(server: "HttpServersConfig") -> dict[str, Any]:
    """NapCat HttpServersConfig → SnowLuma networks.httpServers[].

    字段映射:
    - enable → enabled
    - messagePostFormat → messageFormat
    - token → accessToken (空字符串也写入)
    - host / port / name / reportSelfMessage / path → 同名透传
    - debug / enableCors / enableWebsocket → NapCat-only, 静默丢弃
    """
    return {
        "name": server.name,
        "enabled": bool(server.enable),
        "messageFormat": server.messagePostFormat,
        "accessToken": server.token,
        "reportSelfMessage": False,  # SnowLuma httpServers 没有 reportSelfMessage 字段, NapCat 也不支持; 占位以保留 P1 上游样本结构
        "host": server.host,
        "port": int(server.port),
        "path": server.path,
    }


def _render_http_client(client: "HttpClientsConfig") -> dict[str, Any]:
    """NapCat HttpClientsConfig → SnowLuma networks.httpClients[].

    字段映射:
    - enable → enabled
    - messagePostFormat → messageFormat
    - token → accessToken
    - url → url
    - reportSelfMessage → 同名透传
    - timeoutMs → 仅当非 None 时写入 (None 表示让 SnowLuma 走默认 5000ms)
    - debug → NapCat-only, 静默丢弃
    """
    payload: dict[str, Any] = {
        "name": client.name,
        "enabled": bool(client.enable),
        "messageFormat": client.messagePostFormat,
        "accessToken": client.token,
        "url": str(client.url),
        "reportSelfMessage": bool(client.reportSelfMessage),
    }
    if client.timeoutMs is not None:
        payload["timeoutMs"] = int(client.timeoutMs)
    return payload


def _render_ws_server(server: "WebsocketServersConfig") -> dict[str, Any]:
    """NapCat WebsocketServersConfig → SnowLuma networks.wsServers[].

    字段映射:
    - enable → enabled
    - messagePostFormat → messageFormat
    - token → accessToken
    - host / port / name / reportSelfMessage / path / role → 同名透传
    - debug / enableForcePushEvent / heartInterval → NapCat-only, 静默丢弃
    """
    return {
        "name": server.name,
        "enabled": bool(server.enable),
        "messageFormat": server.messagePostFormat,
        "accessToken": server.token,
        "reportSelfMessage": bool(server.reportSelfMessage),
        "host": server.host,
        "port": int(server.port),
        "path": server.path,
        "role": server.role,
    }


def _render_ws_client(client: "WebsocketClientsConfig") -> dict[str, Any]:
    """NapCat WebsocketClientsConfig → SnowLuma networks.wsClients[].

    字段映射:
    - enable → enabled
    - messagePostFormat → messageFormat
    - token → accessToken
    - url → url
    - reportSelfMessage → 同名透传
    - reconnectInterval (ms) → reconnectIntervalMs (clamp ≥ 1000ms)
    - role → 同名透传
    - debug / heartInterval → NapCat-only, 静默丢弃
    """
    return {
        "name": client.name,
        "enabled": bool(client.enable),
        "messageFormat": client.messagePostFormat,
        "accessToken": client.token,
        "url": str(client.url),
        "reportSelfMessage": bool(client.reportSelfMessage),
        "reconnectIntervalMs": _clamp_reconnect_ms(int(client.reconnectInterval), name=client.name),
        "role": client.role,
    }


def _build_fallback_networks() -> dict[str, Any]:
    """当 connect.httpServers 与 connect.websocketServers 都为空时, 兜底一份与 SnowLuma
    ``makeDefaultOneBotConfig()`` 等价的默认值, 避免 SnowLuma 因 ``onebot_<uin>.json.networks``
    全空启动失败.

    accessToken 用 ``secrets.token_urlsafe(32)`` 随机生成, 与 P1 行为一致.
    """
    fallback_token = secrets.token_urlsafe(32)
    return {
        "httpServers": [
            {
                "name": "http-default",
                "enabled": True,
                "messageFormat": "array",
                "accessToken": fallback_token,
                "reportSelfMessage": False,
                "host": "0.0.0.0",
                "port": 3000,
                "path": "/",
            }
        ],
        "wsServers": [
            {
                "name": "ws-default",
                "enabled": True,
                "messageFormat": "array",
                "accessToken": fallback_token,
                "reportSelfMessage": False,
                "host": "0.0.0.0",
                "port": 3001,
                "path": "/",
                "role": "Universal",
            }
        ],
        "httpClients": [],
        "wsClients": [],
    }


def render_onebot_json(
    snowluma_path: Path,
    qqid: int,
    *,
    connect: "ConnectConfig",
    music_sign_url: str = "",
) -> None:
    """渲染 SnowLuma onebot_<qqid>.json (P2 后端感知, Tier B).

    ``connect`` 中的 4 类网络配置 (httpServers / httpClients / websocketServers /
    websocketClients) 全量映射到 SnowLuma networks.* 数组. ``httpSseServers`` /
    ``plugins`` SnowLuma 不识别, 静默丢弃.

    若 ``connect.httpServers`` 与 ``connect.websocketServers`` 都为空, 自动兜底一份
    与 SnowLuma ``makeDefaultOneBotConfig()`` 等价的默认值 (避免 SnowLuma 启动失败).

    Args:
        snowluma_path: SnowLuma 安装根目录.
        qqid: Bot 的 QQ 号; 决定文件名.
        connect: Bot 的 ``ConnectConfig`` 对象 (来自 ``BotConfig.connect``).
        music_sign_url: 音乐签名地址, 默认空串与上游一致.

    Raises:
        ValueError: ``qqid`` 不是正整数.
    """
    if not isinstance(qqid, int) or qqid <= 0:
        raise ValueError(f"qqid 必须为正整数, 收到: {qqid!r}")

    config_dir = snowluma_path / "config"
    config_dir.mkdir(parents=True, exist_ok=True)
    target = config_dir / f"onebot_{qqid}.json"

    # 构造 networks 字段
    if not connect.httpServers and not connect.websocketServers:
        # 全空兜底 (与 SnowLuma makeDefaultOneBotConfig 等价)
        networks: dict[str, Any] = _build_fallback_networks()
        # 保留用户填的 httpClients / websocketClients (即使 server 全空, 客户端可能仍有定义)
        networks["httpClients"] = [_render_http_client(c) for c in connect.httpClients]
        networks["wsClients"] = [_render_ws_client(c) for c in connect.websocketClients]
    else:
        networks = {
            "httpServers": [_render_http_server(s) for s in connect.httpServers],
            "httpClients": [_render_http_client(c) for c in connect.httpClients],
            "wsServers": [_render_ws_server(s) for s in connect.websocketServers],
            "wsClients": [_render_ws_client(c) for c in connect.websocketClients],
        }

    payload: dict[str, Any] = {
        "networks": networks,
        "musicSignUrl": music_sign_url,
    }

    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


def read_existing_onebot_json(snowluma_path: Path, qqid: int) -> dict[str, Any] | None:
    """读取已存在的 ``onebot_<qqid>.json``; 不存在 / 损坏时返回 ``None``.

    给升级路径用: 调用方可读出已有结构后做局部 ``dict.update`` 再 :func:`render_onebot_json`,
    避免覆盖用户在 SnowLuma WebUI 内对该 Bot 配置的运行时改动.

    Note:
        实际工程中 SnowLuma 启动后会自己 ``saveJson`` 这个文件 (``config-DyfbYA36.js:149``);
        Desktop 仅在启动**前**写一次, 所以本函数主要用于安装升级与 UI 信息回显, 不参与运行时同步.
    """
    target = snowluma_path / "config" / f"onebot_{qqid}.json"
    if not target.exists():
        return None
    try:
        payload = json.loads(target.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


# ==================== 内部工具 ====================
def _utc_now_iso() -> str:
    """生成 ISO 8601 UTC 时间戳, 格式与上游 ``webui.json.generatedAt`` 一致.

    上游样本: ``"2026-05-10T12:58:08.315Z"`` — 毫秒精度, ``Z`` 后缀.
    Python ``datetime.isoformat`` 默认 ``+00:00``, 这里手动归一化为 ``Z``.
    """
    now = datetime.now(timezone.utc)
    # 截到毫秒精度 (上游 JS Date.toISOString() 也是毫秒)
    micro = now.microsecond
    millis = micro // 1000
    formatted = now.strftime("%Y-%m-%dT%H:%M:%S") + f".{millis:03d}Z"
    return formatted
