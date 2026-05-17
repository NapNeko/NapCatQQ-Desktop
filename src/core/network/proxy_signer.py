# -*- coding: utf-8 -*-
"""中转代理客户端: HMAC 签名 + 服务器时间漂移自愈.

用法:
    >>> signer = ProxySigner.instance()
    >>> headers = signer.sign_headers("/v1/release/napcat")
    >>> # 拿 headers 去请求, 失败时调用 update_offset_from_response
    >>> signer.update_offset_from_response(response.headers)

设计要点:
    - SHARED_SECRET 通过 ``_build_constants.py`` 注入. 该文件 .gitignore,
      仓库 clone 后无法直接拉到完整功能, 必须用官方发布的二进制包.
    - 时间戳偏差 ±5 分钟内 Worker 接受. 客户端首次失败时读响应头
      ``X-Server-Time``, 算出 offset 并持久化. 下次签名自动校正.
    - offset 持久化路径: ``<config_dir>/.proxy_clock_offset``,
      纯文本一行存秒数 (整数). 不进 cfg 是为了避免触发 cfg 校验逻辑.
"""

from __future__ import annotations

import hashlib
import hmac
import time
from pathlib import Path
from threading import Lock

from src.core.logging import logger
from src.core.network._build_constants import PROXY_SHARED_SECRET
from src.core.runtime.paths import PathFunc

try:
    from creart import it
except ImportError:
    it = None

_OFFSET_FILENAME = ".proxy_clock_offset"
_USER_AGENT_TEMPLATE = "NapCatQQ-Desktop/{version}"


class ProxySigner:
    """对中转代理请求签名, 并维护本地与服务器的时钟偏差.

    单例使用. 构造时从磁盘加载历史 offset; 每次失败响应可调用
    ``update_offset_from_response`` 更新.
    """

    _instance: "ProxySigner | None" = None
    _instance_lock = Lock()

    def __init__(self, app_version: str) -> None:
        self._user_agent = _USER_AGENT_TEMPLATE.format(version=app_version.lstrip("v"))
        self._offset_seconds = 0
        self._lock = Lock()
        self._load_offset()
        # 诊断日志: 只打长度 + 占位标志, 足够诊断 "未注入 / 占位 / 已配置" 三态;
        # 不打任何 secret 字符 (含哈希前缀), 避免日志 / crash bundle 间接泄露.
        is_placeholder = "PLACEHOLDER" in PROXY_SHARED_SECRET
        logger.info(
            f"ProxySigner 初始化: ua={self._user_agent}, "
            f"secret_len={len(PROXY_SHARED_SECRET)}, "
            f"placeholder={is_placeholder}, offset={self._offset_seconds}s"
        )

    @classmethod
    def instance(cls, app_version: str | None = None) -> "ProxySigner":
        """返回全局单例. 首次调用必须传 app_version."""
        with cls._instance_lock:
            if cls._instance is None:
                if app_version is None:
                    from src.core.config import __version__ as fallback_version
                    app_version = fallback_version
                cls._instance = cls(app_version)
            return cls._instance

    def sign_headers(self, path: str) -> dict[str, str]:
        """生成签名头. ``path`` 为 ``/v1/release/...`` 形式."""
        with self._lock:
            ts = str(int(time.time()) + self._offset_seconds)
        sig = hmac.new(
            PROXY_SHARED_SECRET.encode(),
            f"{ts}.{path}".encode(),
            hashlib.sha256,
        ).hexdigest()
        return {
            "User-Agent": self._user_agent,
            "X-Timestamp": ts,
            "X-Signature": sig,
        }

    def update_offset_from_response(self, response_headers) -> bool:
        """读响应头里的 X-Server-Time 校正 offset, 返回是否更新成功."""
        server_time_raw = self._extract_header(response_headers, "X-Server-Time")
        if not server_time_raw:
            return False
        try:
            server_time = int(server_time_raw)
        except (TypeError, ValueError):
            return False

        new_offset = server_time - int(time.time())
        with self._lock:
            if abs(new_offset - self._offset_seconds) < 2:
                # 抖动小于 2 秒不写盘, 避免频繁 IO.
                return False
            self._offset_seconds = new_offset
            self._persist_offset()
        logger.info(f"代理时钟偏差更新为 {new_offset} 秒")
        return True

    @staticmethod
    def _extract_header(headers, name: str) -> str | None:
        if hasattr(headers, "get"):
            value = headers.get(name) or headers.get(name.lower())
            return value
        return None

    def _offset_path(self) -> Path | None:
        try:
            base = it(PathFunc).config_dir_path if it else None
        except Exception:
            return None
        if base is None:
            return None
        return Path(base) / _OFFSET_FILENAME

    def _load_offset(self) -> None:
        path = self._offset_path()
        if path is None or not path.exists():
            return
        try:
            content = path.read_text(encoding="utf-8").strip()
            self._offset_seconds = int(content)
        except (OSError, ValueError):
            self._offset_seconds = 0

    def _persist_offset(self) -> None:
        path = self._offset_path()
        if path is None:
            return
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(str(self._offset_seconds), encoding="utf-8")
        except OSError as exc:
            logger.warning(f"持久化代理时钟偏差失败: {exc}")


def fetch_release_via_proxy(
    proxy_url: str,
    proxy_path: str,
    fallback_url: str,
    timeout: int = 10,
    github_token: str | None = None,
) -> dict | None:
    """同步拉一个 release JSON, 优先走中转, 失败回退 GitHub 官方 API.

    专供 ``urllib`` 调用方 (如 server_manager.py) 使用. service.py 已用 httpx
    自带签名 + 校时, 不需要这个 helper.

    Args:
        github_token: 用户配置的 GitHub Personal Token. 非空时, 兜底直连 GitHub
            会带上 ``Authorization: Bearer ...``, 把限速从 60/h 拉到 5000/h.

    返回 None 表示主备都失败; 调用方自行决定怎么报错.
    """
    import json
    import urllib.error
    import urllib.request

    signer = ProxySigner.instance()

    def _try(url: str, signed: bool, with_token: bool) -> tuple[dict | None, dict | None]:
        """返回 (json_body, response_headers); body 为 None 即失败."""
        headers = {
            "Accept": "application/vnd.github+json",
            "User-Agent": "NapCatQQ-Desktop",
        }
        if signed:
            headers.update(signer.sign_headers(proxy_path))
        elif with_token and github_token:
            headers["Authorization"] = f"Bearer {github_token}"
        req = urllib.request.Request(url, headers=headers)  # noqa: S310
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:  # noqa: S310
                return json.loads(resp.read()), dict(resp.headers)
        except urllib.error.HTTPError as exc:
            return None, dict(exc.headers) if exc.headers else None
        except (urllib.error.URLError, ValueError, OSError):
            return None, None

    body, headers = _try(proxy_url, signed=True, with_token=False)
    if body is not None:
        return body

    if headers and signer.update_offset_from_response(headers):
        body, _ = _try(proxy_url, signed=True, with_token=False)
        if body is not None:
            return body

    body, _ = _try(fallback_url, signed=False, with_token=True)
    return body
