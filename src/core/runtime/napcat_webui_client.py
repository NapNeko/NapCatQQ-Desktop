# -*- coding: utf-8 -*-
"""NapCat WebUI HTTP 客户端 (2026-05-11 问题 2 修复, 配置热推送).

封装 NapCat WebUI 后端 (基于 Express) 的鉴权流程 + OB11 / NapCat 配置更新接口,
让 Desktop 在 ``update_config`` 成功后能把新配置 push 到正在跑的 Bot, 触发
``WebUiDataRuntime.setOB11Config`` 的 hot reload, 无需用户重启 Bot.

设计要点
========

- 所有调用走 :mod:`httpx` 短连接, ``trust_env=False`` 避免代理拦截 localhost.
- 鉴权流程**两步**:

  1. ``POST /api/auth/login`` body: ``{hash}``, 其中 ``hash = sha256(token + ".napcat").hex()``,
     ``token`` 从 ``<napcat_path>/config/webui.json`` 读取;
  2. server 返回 ``{Credential: <base64>}``, 后续 API 用
     ``Authorization: Bearer <Credential>``.

- Credential **1 小时**过期 (NapCat ``MAX_CREDENTIAL_VALID_SECONDS`` = 3600), 401 时
  自动重 login. Desktop 一次 push 用不到 1h, 实际不会触发.

- NapCat 错误响应 **status 200 + body.code = -1 + body.message** (而非 HTTP 401),
  这是 NapCat 设计选择 (统一通过 body.code 判断); 本 client 透明处理.

- ``POST /api/OB11Config/SetConfig`` body: ``{config: <stringified JSON>}``; **必须 QQ
  已登录** (NapCat server side 检查 ``WebUiDataRuntime.getQQLoginStatus()``), 未登录
  时返回 ``Not Login`` 错误, 上层应将其视为"已落盘, 待用户扫码登录后生效"的语义.

参考实现
========

- 上游 WebUI server: ``example/NapCatQQ-main/packages/napcat-webui-backend/index.ts``
- Auth handler: ``example/NapCatQQ-main/packages/napcat-webui-backend/src/api/Auth.ts:9-44``
- SignToken (sha256 + ".napcat"):
  ``example/NapCatQQ-main/packages/napcat-webui-backend/src/helper/SignToken.ts:101-103``
- OB11SetConfig: ``example/NapCatQQ-main/packages/napcat-webui-backend/src/api/OB11Config.ts:38-60``
- 响应格式: ``example/NapCatQQ-main/packages/napcat-webui-backend/src/utils/response.ts``
"""
from __future__ import annotations

# 标准库导入
import hashlib
import json as json_lib
from pathlib import Path
from typing import Any, Final, Literal

# 第三方库导入
import httpx

# 项目内模块导入
from src.core.logging import LogSource, LogType, logger


_DEFAULT_TIMEOUT_S: Final[float] = 5.0
_DEFAULT_PORT: Final[int] = 6099
_PASSWORD_SALT: Final[str] = ".napcat"

# httpx kwargs: 不走系统代理 (与 SnowLumaWebUIClient 对齐)
_HTTPX_KWARGS: Final[dict[str, Any]] = {"trust_env": False}

# NapCat WebUI host 候选 (与 SnowLuma 同理, 兼容 IPv4 / IPv6 / localhost)
_HOST_CANDIDATES: Final[tuple[str, ...]] = (
    "127.0.0.1",
    "localhost",
)


class NapCatWebUIError(Exception):
    """所有 NapCat WebUI HTTP 调用失败的统一异常类型.

    Attributes:
        code: NapCat 业务层 code (``0=success``, ``-1=error``, 或 HTTP 层错误时 0).
        message: server message 字段或本地构造的描述, 适合直接 emit 到 ``notification_signal``.
    """

    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def read_napcat_webui_config(napcat_path: Path) -> dict[str, Any] | None:
    """读取 ``<napcat_path>/config/webui.json``, 返回字典或 ``None`` (文件不存在 / 损坏).

    Args:
        napcat_path: NapCat 安装根目录 (``PathFunc.napcat_path``).

    Returns:
        dict 含 ``host``/``port``/``token`` 等字段, 或 ``None`` 表示 NapCat 还没启过
        (默认 webui.json 在 NapCat 首次启动时由 ``WebUiConfigWrapper.ensureConfigFileExists``
        创建). 调用方应据此提示用户先启动一次 Bot.
    """
    target = napcat_path / "config" / "webui.json"
    if not target.exists():
        return None
    try:
        payload = json_lib.loads(target.read_text(encoding="utf-8"))
    except (OSError, json_lib.JSONDecodeError) as exc:
        logger.warning(
            f"读取 NapCat webui.json 失败 (path={target}): {type(exc).__name__}: {exc}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return None
    return payload if isinstance(payload, dict) else None


def _generate_password_hash(token: str) -> str:
    """复现 NapCat ``AuthHelper.generatePasswordHash`` (sha256 + ".napcat" 盐).

    上游实现 (一字不改):
    ``return crypto.createHash('sha256').update(password + '.napcat').digest().toString('hex');``

    Args:
        token: webui.json 中的 raw token.

    Returns:
        小写 hex string (sha256 digest 长度 64).
    """
    return hashlib.sha256((token + _PASSWORD_SALT).encode("utf-8")).hexdigest()


class NapCatWebUIClient:
    """NapCat WebUI HTTP 客户端.

    认证语义: :meth:`login` 后内部持 Credential (base64 Bearer); 任何 API 调用收到
    NapCat 业务码 ``code=-1`` + ``message='Unauthorized'`` 时**自动**重 login + 重试一次,
    仍失败则抛 :class:`NapCatWebUIError`.

    所有调用走 :mod:`httpx` 短连接.

    Note:
        构造时不要求 host/port 必须正确, :meth:`login` 会按 :data:`_HOST_CANDIDATES`
        逐个尝试, 命中后锁定 ``self._host``.
    """

    def __init__(self, host: str, port: int, token: str) -> None:
        """构造 client.

        Args:
            host: WebUI host (一般 ``"127.0.0.1"``); login 时会按候选 host 探测,
                命中后锁定 ``self._host``.
            port: WebUI 端口 (默认 6099, 从 webui.json 读取).
            token: webui.json 中的 raw token.
        """
        self._host = host
        self._port = port
        self._token = token
        self._credential: str | None = None

    @classmethod
    def from_napcat_path(cls, napcat_path: Path) -> "NapCatWebUIClient | None":
        """工厂方法: 从 ``<napcat_path>/config/webui.json`` 构造 client.

        Args:
            napcat_path: NapCat 安装根目录.

        Returns:
            构造好的 client, 或 ``None`` (webui.json 不存在 / 损坏 / 缺关键字段).
            调用方应据此判断 "NapCat 是否从未启动过", 选择是否提示用户.
        """
        payload = read_napcat_webui_config(napcat_path)
        if payload is None:
            return None

        host_raw = payload.get("host", "")
        port_raw = payload.get("port", _DEFAULT_PORT)
        token = payload.get("token", "")

        # webui.json 默认 host="::", 这是 Node listen 的 IPv6 unspecified, 客户端不能直接
        # 当 host 用 ("[::]:port" 不可达); 统一映射为 127.0.0.1 + 后续 login 探测候选.
        if not isinstance(host_raw, str) or host_raw in ("", "::", "0.0.0.0"):
            host = "127.0.0.1"
        else:
            host = host_raw

        if not isinstance(port_raw, int) or port_raw <= 0:
            port = _DEFAULT_PORT
        else:
            port = port_raw

        if not isinstance(token, str) or not token:
            logger.warning(
                f"NapCat webui.json 缺 token 字段, 跳过 WebUI client 构造 (host={host}, port={port})",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            return None

        return cls(host=host, port=port, token=token)

    # ==================== 属性 ====================
    @property
    def base_url(self) -> str:
        return f"http://{self._host}:{self._port}"

    @property
    def credential(self) -> str | None:
        """当前 Bearer credential (login 之前为 ``None``)."""
        return self._credential

    # ==================== 公共 API ====================
    def login(self, timeout: float = _DEFAULT_TIMEOUT_S) -> str:
        """``POST /api/auth/login {"hash": <sha256>}`` → 拿 Credential.

        2026-05-11: NapCat WebUI listen 默认在 ``::`` (IPv6 unspecified). Windows + IPv6
        优先策略下 ``httpx.post("http://127.0.0.1:6099/...")`` 可能成功也可能 ECONNREFUSED
        (取决于 OS / 双栈配置). 我们按 :data:`_HOST_CANDIDATES` 逐个尝试, 第一个能 connect
        的就锁定到 ``self._host`` 后续 API 复用.

        Args:
            timeout: 单次 HTTP 请求超时 (秒).

        Returns:
            Credential (base64 字符串), 已存到 ``self._credential``.

        Raises:
            NapCatWebUIError: 所有候选 host 都连不上 / login 失败 / 响应结构异常.
        """
        password_hash = _generate_password_hash(self._token)

        candidates: list[str] = [self._host]
        for host in _HOST_CANDIDATES:
            if host not in candidates:
                candidates.append(host)

        last_errors: dict[str, str] = {}
        for host in candidates:
            url = f"http://{host}:{self._port}/api/auth/login"
            try:
                resp = httpx.post(
                    url,
                    json={"hash": password_hash},
                    timeout=timeout,
                    **_HTTPX_KWARGS,
                )
            except (httpx.RequestError, httpx.TimeoutException) as exc:
                last_errors[host] = f"{type(exc).__name__}: {exc}"
                continue

            # 命中可用 host, 锁定 self._host 后续 API 复用
            if self._host != host:
                logger.info(
                    f"NapCat WebUI host 锁定: {self._host} -> {host}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self._host = host

            # 解析响应 (NapCat status 永远 200, 通过 body.code 判断成败)
            credential = self._parse_login_response(resp)
            self._credential = credential
            logger.info(
                f"NapCat WebUI login OK (host={host}, port={self._port})",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return credential

        # 所有候选都连不上
        raise NapCatWebUIError(
            0,
            f"NapCat WebUI login 网络层失败 (port={self._port}, "
            f"候选 host 最后错误={last_errors})",
        )

    def set_ob11_config(self, config: dict[str, Any]) -> None:
        """``POST /api/OB11Config/SetConfig {"config": <stringified JSON>}``.

        Server side 实现见 ``@example/NapCatQQ-main/packages/napcat-webui-backend/src/api/OB11Config.ts:38-60``;
        会:

        1. 检查 ``WebUiDataRuntime.getQQLoginStatus()`` — 未登录返回 ``Not Login``;
        2. ``json5.parse(req.body.config)`` → ``loadConfig`` → ``setOB11Config``;
        3. ``onOB11ConfigChanged(ob11)`` 触发 hot reload.

        Args:
            config: OneBot 配置 dict (与 :func:`render_onebot_json` 写盘格式同源, 但 NapCat
                schema 是 ``OneBotConfig = {network, musicSignUrl, enableLocalFile2Url,
                parseMultMsg}``, 调用方应提前构造好).

        Raises:
            NapCatWebUIError: server 拒绝 / 网络错误 / QQ 未登录.
        """
        # NapCat 要求 body.config 是**字符串**, 内部走 json5.parse. 不能直接传 dict.
        config_str = json_lib.dumps(config, ensure_ascii=False)
        payload = self._authed_call(
            "POST",
            "/api/OB11Config/SetConfig",
            json_body={"config": config_str},
        )
        # 成功路径: body = {code: 0, message: 'success', data: null}; 失败时 _authed_call 已抛.
        logger.info(
            (
                f"NapCat set_ob11_config OK: "
                f"{payload.get('message', '<no message>')}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

    def check_login_status(self) -> bool:
        """``POST /api/QQLogin/CheckLoginStatus`` 检查 QQ 是否已登录.

        用于在 :meth:`set_ob11_config` 之前预判, 让上层选择降级提示
        ("Bot 未登录, 重启后生效") 还是直接推送.

        Returns:
            ``True`` 表示已登录, ``False`` 表示未登录或调用失败.

        Note:
            本方法吞掉所有异常 (返回 ``False``); 鉴权失败 / 网络错误都视为"未登录".
            上层应据此降级到 "配置已保存, 重启 Bot 后生效" 提示.
        """
        try:
            payload = self._authed_call("POST", "/api/QQLogin/CheckLoginStatus")
        except NapCatWebUIError:
            return False

        # response.data 可能是 bool 或 {isLogin: bool, ...}; 兼容两种.
        data = payload.get("data")
        if isinstance(data, bool):
            return data
        if isinstance(data, dict):
            value = data.get("isLogin")
            if isinstance(value, bool):
                return value
        return False

    # ==================== 内部 ====================
    def _parse_login_response(self, resp: httpx.Response) -> str:
        """解析 ``POST /api/auth/login`` 响应, 返回 Credential 或抛 :class:`NapCatWebUIError`."""
        try:
            payload = resp.json()
        except ValueError as exc:
            raise NapCatWebUIError(
                0, f"login 响应非 JSON (status={resp.status_code}): {resp.text[:200]}"
            ) from exc

        if not isinstance(payload, dict):
            raise NapCatWebUIError(0, f"login 响应结构异常: {payload!r}")

        code = payload.get("code")
        message = payload.get("message", "")
        if code != 0:
            raise NapCatWebUIError(
                code if isinstance(code, int) else 0,
                f"NapCat login 失败 (code={code}): {message}",
            )

        data = payload.get("data")
        if not isinstance(data, dict):
            raise NapCatWebUIError(0, f"login 响应缺 data 字段: {payload!r}")

        credential = data.get("Credential")
        if not isinstance(credential, str) or not credential:
            raise NapCatWebUIError(0, f"login 响应缺 Credential: {data!r}")

        return credential

    def _auth_header(self) -> dict[str, str]:
        if self._credential is None:
            return {}
        return {"Authorization": f"Bearer {self._credential}"}

    def _authed_call(
        self,
        method: Literal["GET", "POST"],
        path: str,
        *,
        json_body: dict[str, Any] | None = None,
        timeout: float = _DEFAULT_TIMEOUT_S,
    ) -> dict[str, Any]:
        """发起鉴权 API 调用; 收到 NapCat ``code=-1 + Unauthorized`` 时自动重 login 并重试一次.

        Args:
            method: HTTP 方法 (NapCat 大部分配置写入接口走 POST).
            path: API 路径, 例如 ``/api/OB11Config/SetConfig``.
            json_body: 请求体 dict (会被 ``httpx`` 序列化为 JSON).
            timeout: 单次 HTTP 请求超时 (秒).

        Returns:
            解析后的响应 payload (NapCat 包了一层 ``{code, message, data}``, 这里返回整体 dict
            供调用方按需读 ``data``).

        Raises:
            NapCatWebUIError: 业务码 ``code != 0`` / 网络错误 / 响应非 JSON / 重 login 后仍失败.
        """
        # 首次调用必须先 login (lazy login: 避免构造 client 时就发请求)
        if self._credential is None:
            self.login(timeout=timeout)

        url = f"{self.base_url}{path}"
        try:
            resp = httpx.request(
                method,
                url,
                headers=self._auth_header(),
                json=json_body,
                timeout=timeout,
                **_HTTPX_KWARGS,
            )
        except (httpx.RequestError, httpx.TimeoutException) as exc:
            raise NapCatWebUIError(
                0, f"{method} {path} 网络层失败: {type(exc).__name__}: {exc}"
            ) from exc

        try:
            payload = resp.json()
        except ValueError as exc:
            raise NapCatWebUIError(
                0, f"{method} {path} 响应非 JSON (status={resp.status_code}): {resp.text[:200]}"
            ) from exc

        if not isinstance(payload, dict):
            raise NapCatWebUIError(0, f"{method} {path} 响应结构异常: {payload!r}")

        code = payload.get("code")
        message = payload.get("message", "")

        # NapCat 的 401 等价物: code=-1 + message='Unauthorized' (HTTP status 仍是 200).
        # Credential 1h 过期时会触发, 我们重 login + 重试一次.
        if code != 0 and isinstance(message, str) and "Unauthorized" in message:
            logger.info(
                f"NapCat WebUI 收到 Unauthorized, 重新 login + 重试 ({method} {path})",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self._credential = None
            self.login(timeout=timeout)
            # 重试一次 (递归只 1 层: 重 login 后已设新 credential, 不会再走这个分支)
            try:
                resp = httpx.request(
                    method,
                    url,
                    headers=self._auth_header(),
                    json=json_body,
                    timeout=timeout,
                    **_HTTPX_KWARGS,
                )
                payload = resp.json()
            except (httpx.RequestError, httpx.TimeoutException, ValueError) as exc:
                raise NapCatWebUIError(
                    0,
                    f"{method} {path} 重试失败 (login 后): {type(exc).__name__}: {exc}",
                ) from exc
            if not isinstance(payload, dict):
                raise NapCatWebUIError(
                    0, f"{method} {path} 重试响应结构异常: {payload!r}"
                )
            code = payload.get("code")
            message = payload.get("message", "")

        if code != 0:
            raise NapCatWebUIError(
                code if isinstance(code, int) else 0,
                f"NapCat {method} {path} 失败 (code={code}): {message}",
            )

        return payload
