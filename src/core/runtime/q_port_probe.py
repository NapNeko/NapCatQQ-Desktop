# -*- coding: utf-8 -*-
"""QQ 登录账号探测 (走 QQ 自带的 ``tencent://`` 深链接 HTTP 服务).

QQ NT 启动后会在本地 ``9210-9219`` 端口监听一个迷你 HTTP 服务, 用于处理
``tencent://`` 深链接 (例如浏览器点击 QQ 群聊链接). 我们利用这个端口在 SnowLuma
注入**之前**探测目标 ``QQ.exe`` 的登录账号, 用于:

- 热启动 PID picker: 在 UI 上直接显示每个 QQ.exe 当前登录的 ``uin``, 用户不用
  靠"启动时间 + 内存"猜哪个是想要的账号.
- 防 cross-bot 误注入: 用户配置的 ``QQID`` 与候选进程实际登录的 ``uin`` 不一致时,
  上层可以提前拦截.

请求 / 响应协议 (逆向 ``Timwp.exe`` 得到, 见上游 `SnowLuma#56
<https://github.com/SnowLuma/SnowLuma/pull/56>`_):

- 请求: ``POST /tencent`` Body=``tencent://``, 端口 9210-9219.
- 响应: 含一段 JWT 字符串 (正则 ``eyJ[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+``);
  解 base64url payload 后拿到 ``{errCode, uin, uid, nickName, ...}``.
- ``errCode == 0`` 且 ``uin`` 非空 → 已登录.

**线程亲和性**: ``socket.connect`` / ``psutil.net_connections`` 都阻塞,
**禁止主线程调用**. 调用方应在 :class:`QThreadPool` 工作线程里跑.

参见: ``src/ui/page/bot_page/widget/snowluma_start_dialog.py``
``EnumerateQQProcessesWorker`` (PID picker 探测入口).
"""
from __future__ import annotations

import base64
import json
import re
import socket
from dataclasses import dataclass
from typing import Final

import psutil

from src.core.logging import LogSource, LogType, logger

# QQ NT 在 Windows / Linux 上监听的固定端口范围 (上游 PR 已验证).
_PORT_RANGE_START: Final[int] = 9210
_PORT_RANGE_END: Final[int] = 9219

# 单端口探测超时 (秒). 实测正常响应 < 100ms; 1s 足够.
_DEFAULT_TIMEOUT: Final[float] = 1.0

# JWT 三段式正则 (base64url 字符集). QQ 响应里 JWT 嵌在 JSON 中, 直接 search 取第一个.
_JWT_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"
)


@dataclass(frozen=True, slots=True)
class QqPortLoginInfo:
    """QQ 登录探测结果.

    Attributes:
        port: 命中的端口号 (9210-9219 之一).
        uin: 当前登录账号. 空字符串表示未登录.
        uid: QQ 内部 uid (非 uin), 可能为空.
        nickname: 昵称, 上游协议里没有该字段时为空.
        logged_in: ``uin`` 非空时为 True (与上游 PR 判定一致).
    """

    port: int
    uin: str
    uid: str = ""
    nickname: str = ""
    logged_in: bool = False


def _decode_jwt_payload(token: str) -> dict | None:
    """解 JWT 中段 (payload) 的 base64url JSON. 任意失败返回 None."""
    try:
        parts = token.split(".")
        if len(parts) != 3:
            return None
        payload_segment = parts[1]
        # base64url 在 JWT 里通常省略尾部 ``=`` padding, 这里手动补齐.
        padding = "=" * (-len(payload_segment) % 4)
        decoded = base64.urlsafe_b64decode(payload_segment + padding)
        parsed = json.loads(decoded)
        return parsed if isinstance(parsed, dict) else None
    except (ValueError, json.JSONDecodeError):
        return None


def _build_probe_payload(port: int) -> bytes:
    """构造 ``POST /tencent`` 探测请求.

    Note:
        body 不能用上游 PR 的 ``tencent://``: 实测部分 QQ NT 版本会把空协议解析为
        "打开 QQ 主窗口", 触发用户可见的弹窗 (主面板被拉到前台). 改用一个 QQ 没注册
        的伪 action ``tencent://snowluma-probe-noop`` — HTTP 层照常返回 JWT, 但
        deeplink dispatcher 找不到 handler, 静默丢弃, 不打扰用户.
    """
    link = "tencent://snowluma-probe-noop"
    request = (
        f"POST /tencent HTTP/1.1\r\n"
        f"Host: 127.0.0.1:{port}\r\n"
        f"Connection: close\r\n"
        f"Content-Length: {len(link)}\r\n"
        f"\r\n"
        f"{link}"
    )
    return request.encode("ascii")


def _probe_port(port: int, timeout: float) -> QqPortLoginInfo | None:
    """向单个端口发探测请求并解析 JWT.

    任何错误 (端口未开 / 连接拒绝 / 超时 / 响应不含 JWT / payload errCode != 0)
    都返回 None, 让调用方继续尝试下一个端口.
    """
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(timeout)
            sock.connect(("127.0.0.1", port))
            sock.sendall(_build_probe_payload(port))

            chunks: list[bytes] = []
            while True:
                try:
                    chunk = sock.recv(4096)
                except socket.timeout:
                    break
                if not chunk:
                    break
                chunks.append(chunk)
        response = b"".join(chunks).decode("utf-8", errors="replace")
    except (OSError, socket.timeout):
        return None

    match = _JWT_PATTERN.search(response)
    if not match:
        return None

    payload = _decode_jwt_payload(match.group(0))
    if payload is None:
        return None
    if payload.get("errCode") != 0:
        return None

    uin = str(payload.get("uin") or "").strip()
    if not uin:
        # 兜底: 部分 QQ 版本把 uin 嵌在 ``data.uin``.
        nested = payload.get("data") or {}
        if isinstance(nested, dict):
            uin = str(nested.get("uin") or "").strip()
    return QqPortLoginInfo(
        port=port,
        uin=uin,
        uid=str(payload.get("uid") or "").strip(),
        nickname=str(payload.get("nickName") or "").strip(),
        logged_in=bool(uin),
    )


def _list_listening_ports(pid: int) -> list[int]:
    """通过 :mod:`psutil` 拿 PID 的本地监听端口, 仅保留 9210-9219 范围.

    Windows 上 psutil 拿其它进程的 ``net_connections`` 通常需要管理员权限. 失败
    时返回空 list, 调用方退化为全端口扫描.
    """
    try:
        proc = psutil.Process(pid)
        connections = proc.net_connections(kind="inet")
    except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
        return []
    except Exception:  # noqa: BLE001 - psutil 在 Windows 偶发未知异常, 不应传播
        return []

    ports: set[int] = set()
    for conn in connections:
        try:
            if conn.status != psutil.CONN_LISTEN:
                continue
            laddr = conn.laddr
            port = int(getattr(laddr, "port", 0))
            if _PORT_RANGE_START <= port <= _PORT_RANGE_END:
                ports.add(port)
        except Exception:  # noqa: BLE001
            continue
    return sorted(ports)


def probe_qq_login(
    pid: int,
    *,
    timeout: float = _DEFAULT_TIMEOUT,
) -> QqPortLoginInfo | None:
    """探测指定 ``QQ.exe`` PID 的登录账号信息.

    Args:
        pid: 目标 ``QQ.exe`` 主进程 PID (通常来自
            :func:`enumerate_qq_processes` 过滤后的"主"候选).
        timeout: 单端口探测超时 (秒). 默认 1s.

    Returns:
        - ``None``: 完全失败 (10 个端口都没响应 / 进程已退出 / 全部 JWT 异常).
        - :class:`QqPortLoginInfo`: 有响应; 通过 ``logged_in`` 字段判断是否真登录.

    **线程亲和性**: 阻塞调用, 必须在工作线程; 主线程直调会卡 UI.
    """
    if pid <= 0:
        return None

    candidate_ports = _list_listening_ports(pid)
    if not candidate_ports:
        # 没拿到 PID 的监听端口 (权限不足 / 进程刚退出 / psutil 异常),
        # 退化到全端口扫描 - 与上游 PR 行为一致.
        candidate_ports = list(range(_PORT_RANGE_START, _PORT_RANGE_END + 1))

    for port in candidate_ports:
        info = _probe_port(port, timeout)
        if info is not None:
            logger.trace(
                (
                    f"probe_qq_login 命中 (pid={pid}, port={port}, "
                    f"uin={info.uin or '<empty>'}, logged_in={info.logged_in})"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return info
    logger.trace(
        f"probe_qq_login 全端口未命中 (pid={pid})",
        LogType.NETWORK,
        LogSource.CORE,
    )
    return None
