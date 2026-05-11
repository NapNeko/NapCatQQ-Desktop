# -*- coding: utf-8 -*-
"""SnowLuma WebUI HTTP 客户端 (Tier D, P2 注入流程自动化).

封装 SnowLuma ``packages/core/src/webui/server.ts`` 的 8 个端点, 让 Desktop 一键启动
SnowLuma Bot 时不需要用户去浏览器登录 WebUI 选 PID 注入.

API 全集见 ``docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md`` §10.3.

设计要点:

- 所有调用走 :mod:`httpx` 短连接 (与 :class:`SnowLumaStatusPoller` 对齐, 不持单例
  ``httpx.Client``).
- 认证语义: ``login()`` 后内部持 ``Bearer`` token; 任何 API 调用收到 ``401`` 时**自动**
  重 ``login`` + 重试一次, 仍失败则抛 :class:`SnowLumaWebUIError`.
- 超时矩阵: ``wait_ready`` 最多 30s (1s × 30 轮), 一般 API 默认 5s,
  ``load_process`` 因为涉及 native ``dlopen`` 放宽到 15s.

参考实现:
- 上游 WebUI server: ``example/SnowLuma-main/packages/core/src/webui/server.ts:34-440``
- 上游 HookManager: ``example/SnowLuma-main/packages/core/src/hook/hook-manager.ts:21-29``
- 上游 auth 规则: ``example/SnowLuma-main/packages/core/src/webui/auth.ts:38-44``
"""
from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Any, Callable, Final, Literal

import httpx

from src.core.logging import LogSource, LogType, logger


# 状态值 (从 packages/core/src/hook/hook-manager.ts:12-19)
HookProcessStatus = Literal[
    "available",   # 找到 QQ.exe, 未注入
    "loading",     # 注入中
    "connecting",  # 注入成功, 连 named pipe 中
    "loaded",      # 已注入 + 连上 pipe, 未登录 (用户该扫码了)
    "online",      # QQ 已登录, bot 完全可用
    "error",       # 注入或连接失败 (`error` 字段含具体原因)
    "disconnected",  # 之前注入过, 现在 named pipe 掉了 (QQ.exe 退出 / hook 模块被卸载)
]


@dataclass(frozen=True)
class HookProcessInfo:
    """匹配上游 ``packages/core/src/hook/hook-manager.ts:21-29`` ``HookProcessInfo``.

    Attributes:
        pid: QQ.exe PID
        name: 进程名 (一般为 ``"QQ.exe"``)
        path: 进程可执行文件全路径
        uin: 已登录的 QQ 号 (登录前为空字符串)
        status: ``HookProcessStatus`` 7 档之一
        error: 当 ``status == "error"`` 时含具体错误信息, 否则为空字符串
    """

    pid: int
    name: str
    path: str
    uin: str
    status: str
    error: str = ""


@dataclass(frozen=True)
class OneBotInstanceInfo:
    """匹配上游 ``/api/qq-list`` 返回的单个 OneBot 实例.

    Server 实现位置: ``@example/SnowLuma-main/.../webui/server.ts:307-311``.
    返回值是 ``oneBotManager.getInstances()`` 映射后的 ``{uin, nickname}``.

    Attributes:
        uin: 登录 QQ 号 (字符串形式, 与 :class:`HookProcessInfo.uin` 一致)
        nickname: QQ 昵称 (登录后 hook 上报)
    """

    uin: str
    nickname: str


class SnowLumaWebUIError(Exception):
    """所有 WebUI HTTP 调用失败的统一异常类型.

    Attributes:
        status_code: HTTP 响应码; ``0`` 表示无 HTTP 响应 (网络错误 / JSON 解析失败).
        message: 详细错误描述, 适合直接 emit 到 ``notification_signal``.
    """

    def __init__(self, status_code: int, message: str) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.message = message


_DEFAULT_TIMEOUT_S: Final[float] = 5.0
_LOAD_TIMEOUT_S: Final[float] = 15.0
_WAIT_READY_INTERVAL_S: Final[float] = 1.0
_WAIT_READY_MAX_S: Final[float] = 30.0

# P2 (Tier E 修复): SnowLuma 的 @hono/node-server 调 ``serve({port})`` 没传 ``hostname``,
# Node 默认走 ``server.listen(port, undefined)`` 优先绑 ``::`` (IPv6 unspecified).
# 在 Windows + ``IPV6_V6ONLY=1`` 的机器上 ``::`` 只接 IPv6, 我们的 ``httpx.get(127.0.0.1)``
# 是 IPv4 → 连不上 → 30s 空转.
# 解法: 给一份候选 host 列表, 按顺序 probe, 第一个能 200 的就锁定当 ``self._host``,
# 后续所有 API 调用复用. 兼容 Node 绑 ``::`` / ``0.0.0.0`` / IPv4-only 这几种情况.
_HOST_CANDIDATES: Final[tuple[str, ...]] = (
    "localhost",  # 优先让 OS DNS 决定 (modern Win10/11 默认 IPv6 优先)
    "127.0.0.1",  # IPv4 显式
    "[::1]",      # IPv6 显式 (httpx URL 里 IPv6 字面量必须方括号包裹)
)

# P2 (Tier E 修复): 所有 httpx 调用一律 ``trust_env=False``, 避免:
# - Clash / Surge 等 TUN 模式代理工具设置 ``HTTP_PROXY`` / ``ALL_PROXY`` 等环境变量后
#   httpx 把 localhost:5099 请求送给代理 → 代理无法回环 → 无响应空转;
# - 系统 .netrc 文件被意外引用导致莫名认证逻辑.
# SnowLuma WebUI 总是 localhost, 走代理百害无一利.
_HTTPX_KWARGS: Final[dict[str, Any]] = {"trust_env": False}


class SnowLumaWebUIClient:
    """SnowLuma WebUI HTTP 客户端.

    认证语义: login 后内部持 Bearer token; 任何 API 调用收到 401 时自动重 login + 重试一次,
    仍失败则抛 :class:`SnowLumaWebUIError(401, ...)`.

    所有调用走 :mod:`httpx` 短连接 (与 :class:`SnowLumaStatusPoller` 对齐,
    不持单例 ``httpx.Client``).
    """

    def __init__(self, host: str, port: int, password: str) -> None:
        # P2 (Tier E 修复): ``host`` 入参作为**初始**候选 (driver 传 "127.0.0.1" 是上一版
        # 的硬编码), :meth:`wait_ready` 会按 :data:`_HOST_CANDIDATES` 逐个 probe 直到命中
        # 真正在听的 host, 然后把 ``self._host`` 锁定到那个值. 后续 API 调用直接复用.
        self._host = host
        self._port = port
        self._password = password
        self._token: str | None = None
        # P2 (Tier E 修复): 暴露 wait_ready 失败时每个候选 host 最后一次错误, 让上层
        # (_phase_b_wait_and_login) 可以把诊断信息塞进抛出的 RuntimeError 里, UI
        # notification 也能看见.
        self.last_wait_errors: dict[str, str] = {}

    @property
    def base_url(self) -> str:
        return f"http://{self._host}:{self._port}"

    @property
    def token(self) -> str | None:
        """当前 Bearer token (login 之前为 ``None``)."""
        return self._token

    # ==================== 公共 API (8 个) ====================
    def wait_ready(
        self,
        timeout: float = _WAIT_READY_MAX_S,
        is_dead_check: Callable[[], bool] | None = None,
    ) -> bool:
        """轮询 ``GET /api/status`` 直到 ``200`` 或 ``timeout`` (秒).

        每次失败间隔 :data:`_WAIT_READY_INTERVAL_S` 秒后重试. 最多 ``timeout / 1.0`` 轮.

        P2 (Tier E 修复): 每轮**按 :data:`_HOST_CANDIDATES` 顺序逐个 probe** 直到命中
        真正在听的 host (兼容 Node 绑 ``::`` / ``0.0.0.0`` / IPv4-only), 命中后把
        ``self._host`` 锁定, 后续 API 调用复用此 host.

        Args:
            timeout: 最大等待秒数.
            is_dead_check: 可选的快速失败回调; 每轮重试前调用一次, 返回 ``True`` 时立即
                结束 (返回 ``False``). 用于工作线程探测 node.exe 已挂的场景, 避免空等满
                ``timeout`` 秒.

        Returns:
            ``True`` 如果 server 起来 (任一候选 host 200); ``False`` 如果 ``timeout`` 内
            未起或 ``is_dead_check`` 判定进程已死.
        """
        deadline = time.monotonic() + timeout
        # 候选列表: 初始 self._host 排第一 (尊重调用方传的值), 再追加未重复的标准候选
        candidates: list[str] = [self._host]
        for host in _HOST_CANDIDATES:
            if host not in candidates:
                candidates.append(host)

        last_errors: dict[str, str] = {}
        self.last_wait_errors = last_errors  # 持引用; 失败时上层可读
        attempt_count = 0
        while time.monotonic() < deadline:
            # 优先检查进程是否还活着 (跨线程 fast-fail). 进程已死时再 polling 浪费时间.
            if is_dead_check is not None and is_dead_check():
                return False
            attempt_count += 1
            for host in candidates:
                url = f"http://{host}:{self._port}/api/status"
                try:
                    resp = httpx.get(url, timeout=_DEFAULT_TIMEOUT_S, **_HTTPX_KWARGS)
                    # P2 (Tier E 修复): SnowLuma 的 ``/api/*`` 全部走 auth middleware (除
                    # ``/api/login``), 无 token → 401. ``/api/status`` 也不例外. 我们这里
                    # 是**就绪探测**, 不关心 auth 结果, 只关心 HTTP server 是否在响应.
                    # 任何 HTTP 响应码 (200/401/403/404/5xx) 都说明 server 起来了, 后续
                    # ``login()`` 会真正校验 password. 仅 socket 级错误 (RequestError /
                    # TimeoutException) 才算未起.
                    # 命中: 锁定 host 后续 API 复用
                    if self._host != host:
                        logger.info(
                            (
                                f"SnowLuma WebUI ready (HTTP {resp.status_code}), "
                                f"host 锁定为 {host} (初始候选: {candidates[0]}; "
                                f"尝试顺序: {candidates}; 经过 {attempt_count} 轮探测)"
                            ),
                            LogType.NETWORK,
                            LogSource.CORE,
                        )
                        self._host = host
                    else:
                        logger.trace(
                            f"SnowLuma WebUI ready (HTTP {resp.status_code}) on {host}",
                            LogType.NETWORK,
                            LogSource.CORE,
                        )
                    return True
                except (httpx.RequestError, httpx.TimeoutException) as exc:
                    last_errors[host] = f"{type(exc).__name__}: {exc}"
            time.sleep(_WAIT_READY_INTERVAL_S)

        # 超时: 把每个候选最后一次的错误打出来便于诊断
        logger.warning(
            (
                f"SnowLuma WebUI {timeout}s 内未就绪 (探测 {attempt_count} 轮), "
                f"各候选 host 最后一次错误: {last_errors}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        return False

    def login(self) -> str:
        """``POST /api/login {password}`` → 拿 Bearer token, 内部持有, 返回 token.

        Raises:
            SnowLumaWebUIError: 登录失败 (404 / 401 / 403 / 500 / 网络错误等).
        """
        try:
            resp = httpx.post(
                f"{self.base_url}/api/login",
                json={"password": self._password},
                timeout=_DEFAULT_TIMEOUT_S,
                **_HTTPX_KWARGS,
            )
        except (httpx.RequestError, httpx.TimeoutException) as exc:
            raise SnowLumaWebUIError(0, f"login 请求失败 ({type(exc).__name__}: {exc})") from exc

        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"login 失败 (status={resp.status_code}): {resp.text[:200]}",
            )

        try:
            payload = resp.json()
        except ValueError as exc:
            raise SnowLumaWebUIError(0, f"login 响应非 JSON: {resp.text[:200]}") from exc

        token = payload.get("token") if isinstance(payload, dict) else None
        if not isinstance(token, str) or not token:
            raise SnowLumaWebUIError(0, f"login 响应结构异常: {payload!r}")

        self._token = token
        return token

    def logout(self) -> None:
        """``POST /api/logout`` 清理 server 端 session; 失败静默忽略 (stop bot 路径用)."""
        if self._token is None:
            return
        try:
            httpx.post(
                f"{self.base_url}/api/logout",
                headers=self._auth_header(),
                timeout=_DEFAULT_TIMEOUT_S,
                **_HTTPX_KWARGS,
            )
        except (httpx.RequestError, httpx.TimeoutException) as exc:
            logger.trace(
                f"SnowLuma WebUI logout 静默忽略: {type(exc).__name__}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        finally:
            self._token = None

    def list_processes(self) -> list[HookProcessInfo]:
        """``GET /api/processes`` 列 QQ.exe 进程; 401 自动重试.

        SL server 返回 ``{"list": [HookProcessInfo, ...]}`` (wrapped). 历史 (W5) 版本
        曾错把 response 当成 plain list 解析, 导致总返回 ``[]``; W7 修复同时兼容
        ``[...]`` (理论 fallback) 和 ``{"list": [...]}`` (实际 SL 行为).

        Returns:
            匹配上游 ``HookProcessInfo[]`` 的解析后列表; 解析失败时返回 ``[]``.
        """
        resp = self._authed_request("GET", "/api/processes")
        if resp.status_code != 200:
            return []
        try:
            payload = resp.json()
        except ValueError:
            return []
        # SL server 实际返回 {"list": [...]}; 兼容 plain list 形式仅作 fallback.
        if isinstance(payload, dict):
            items = payload.get("list", [])
        elif isinstance(payload, list):
            items = payload
        else:
            return []
        if not isinstance(items, list):
            return []
        return [self._parse_hook_info(item) for item in items if isinstance(item, dict)]

    def list_qq_instances(self) -> list[OneBotInstanceInfo]:
        """``GET /api/qq-list`` 列已通过 hook 登录的 QQ OneBot 实例; 401 自动重试.

        与 :meth:`list_processes` 的关键区别:

        - ``/api/processes`` 来自 native ``getAllMainProcess()`` 枚举 (W5 用户实测在某些
          Windows 环境会**返回空**, 即使注入已成功 + hook 已激活, 疑似权限/签名问题).
        - ``/api/qq-list`` 来自 ``oneBotManager.getInstances()`` — 直接读 SnowLuma 内部
          OneBot 实例字典, 不依赖系统枚举. 只要 hook 检测到登录就有数据.

        因此本端点是**热启动 UIN 检测的可靠源**: ``/api/processes`` 返回空时, fallback
        到本端点拿登录 UIN 与 BotConfig.QQID 比对.

        Server 返回 ``{"list": [{"uin": str, "nickname": str}]}``.

        Returns:
            :class:`OneBotInstanceInfo` 列表; 解析失败 / 端点不存在 (老版 SL) 返回 ``[]``.
        """
        try:
            resp = self._authed_request("GET", "/api/qq-list")
        except SnowLumaWebUIError:
            return []
        if resp.status_code != 200:
            return []
        try:
            payload = resp.json()
        except ValueError:
            return []
        if not isinstance(payload, dict):
            return []
        items = payload.get("list", [])
        if not isinstance(items, list):
            return []
        result: list[OneBotInstanceInfo] = []
        for item in items:
            if not isinstance(item, dict):
                continue
            uin = str(item.get("uin", "") or "")
            nickname = str(item.get("nickname", "") or "")
            if uin:
                result.append(OneBotInstanceInfo(uin=uin, nickname=nickname))
        return result

    def load_process(self, pid: int) -> HookProcessInfo:
        """``POST /api/processes/<pid>/load`` 触发注入; 等响应 status (15s timeout).

        SL server 返回 ``{"success": bool, "process": HookProcessInfo}`` (wrapped).
        W7 修复: 原实现把整个响应 dict 传给 ``_parse_hook_info``, 导致 ``pid=0 / uin="" /
        status="available"`` (默认值) — 默认值不是 "error" 刚好跳过错误处理, 但后续逻辑拿到的
        是头信息不完整的 HookProcessInfo. 必须先 unwrap ``payload["process"]``.

        Args:
            pid: QQ.exe PID (一般来自 :meth:`list_processes` 或 ``QProcess.processId()``).

        Returns:
            注入后的 :class:`HookProcessInfo`; 上层应据 ``status`` 决定下一步.

        Raises:
            SnowLumaWebUIError: ``status_code != 200`` 或网络错误.
        """
        resp = self._authed_request("POST", f"/api/processes/{pid}/load", timeout=_LOAD_TIMEOUT_S)
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"load 失败 (pid={pid}, status={resp.status_code}): {resp.text[:200]}",
            )
        try:
            payload = resp.json()
        except ValueError as exc:
            raise SnowLumaWebUIError(0, f"load 响应非 JSON: {resp.text[:200]}") from exc
        # SL 返回 {"success": bool, "process": {...}}; 取 process 子对象.
        # 老版本可能直接返 HookProcessInfo, 源头上外部调用者不依赖 fallback (W5/W7).
        if isinstance(payload, dict):
            inner = payload.get("process") if "process" in payload else payload
        else:
            inner = payload
        if not isinstance(inner, dict):
            raise SnowLumaWebUIError(0, f"load 响应结构异常: {payload!r}")
        return self._parse_hook_info(inner)

    def unload_process(self, pid: int) -> HookProcessInfo:
        """``POST /api/processes/<pid>/unload`` 卸载注入.

        Args:
            pid: QQ.exe PID.

        Returns:
            卸载后的 :class:`HookProcessInfo`.

        Raises:
            SnowLumaWebUIError: 失败时.
        """
        resp = self._authed_request("POST", f"/api/processes/{pid}/unload")
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"unload 失败 (pid={pid}, status={resp.status_code})",
            )
        try:
            payload = resp.json()
        except ValueError as exc:
            raise SnowLumaWebUIError(0, f"unload 响应非 JSON: {resp.text[:200]}") from exc
        # 同 load: SL 返 {"success": bool, "process": {...}}; 取 process 子对象.
        if isinstance(payload, dict):
            inner = payload.get("process") if "process" in payload else payload
        else:
            inner = payload
        if not isinstance(inner, dict):
            raise SnowLumaWebUIError(0, f"unload 响应结构异常: {payload!r}")
        return self._parse_hook_info(inner)

    def update_onebot_config(self, uin: str | int, config: dict[str, Any]) -> bool:
        """``POST /api/config/:uin`` 热更新 OneBot 配置 (问题 2 修复).

        Server 实现见 ``@example/SnowLuma-main/packages/core/src/webui/server.ts:411-427``;
        Body 直接是 ``OneBotConfig`` JSON (与 :func:`render_onebot_json` 写盘格式完全一致).
        Server 内部调 ``saveOneBotConfig(uin, body)`` 持久化, 然后
        ``oneBotManager.reloadConfig(uin)`` 触发当前会话的 hot reload (若 uin 在线).

        Args:
            uin: Bot 的 QQ 号 (字符串或整数都接受, 内部转字符串拼接 URL).
            config: 要推送的 OneBot 配置, 形如
                ``{"networks": {"httpServers": [...], ...}, "musicSignUrl": "..."}``.

        Returns:
            ``True`` 表示该 uin 当前在线且已 hot reload 成功;
            ``False`` 表示已落盘但未热重载 (uin 不在线, 下次连接生效).

        Raises:
            SnowLumaWebUIError: HTTP 调用失败 / 401 重试后仍失败 / server 返回非 200 /
                响应结构异常.
        """
        path = f"/api/config/{uin}"
        resp = self._authed_request("POST", path, json=config)
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"update_onebot_config 失败 (uin={uin}, status={resp.status_code}): "
                f"{resp.text[:200]}",
            )
        try:
            payload = resp.json()
        except ValueError as exc:
            raise SnowLumaWebUIError(
                0, f"update_onebot_config 响应非 JSON: {resp.text[:200]}"
            ) from exc
        if not isinstance(payload, dict):
            raise SnowLumaWebUIError(
                0, f"update_onebot_config 响应结构异常: {payload!r}"
            )
        # server 返回 { success, reloaded, message }; 仅 success=true 才算成功
        if not payload.get("success", False):
            raise SnowLumaWebUIError(
                0,
                f"update_onebot_config server 拒绝 (uin={uin}): "
                f"{payload.get('message', '<no message>')}",
            )
        reloaded = bool(payload.get("reloaded", False))
        logger.info(
            (
                f"SnowLuma update_onebot_config OK (uin={uin}, reloaded={reloaded}): "
                f"{payload.get('message', '<no message>')}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        return reloaded

    def get_auth_state(self) -> dict[str, Any]:
        """``GET /api/auth/state`` 取 ``mustChangePassword`` / session 状态.

        Returns:
            服务端返回的 dict (或 ``{}`` 如果非 200 / 解析失败).
        """
        resp = self._authed_request("GET", "/api/auth/state")
        if resp.status_code != 200:
            return {}
        try:
            payload = resp.json()
        except ValueError:
            return {}
        return payload if isinstance(payload, dict) else {}

    def change_password(self, new: str) -> None:
        """``POST /api/auth/change-password`` 改密 (本期不调用; 保留 API 兼容).

        D2 决策: Desktop 主导密码 → 不通过此 API, 而是直接重写 ``webui.json``.
        本方法保留供未来 P3 扩展.

        Raises:
            SnowLumaWebUIError: 失败时.
        """
        resp = self._authed_request(
            "POST",
            "/api/auth/change-password",
            json={"currentPassword": self._password, "newPassword": new},
        )
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"change_password 失败 (status={resp.status_code}): {resp.text[:200]}",
            )
        self._password = new

    # ==================== 内部 ====================
    def _auth_header(self) -> dict[str, str]:
        if self._token is None:
            return {}
        return {"Authorization": f"Bearer {self._token}"}

    def _authed_request(
        self,
        method: str,
        path: str,
        *,
        json: dict[str, Any] | None = None,
        timeout: float = _DEFAULT_TIMEOUT_S,
    ) -> httpx.Response:
        """带 401 自动 retry 的 HTTP 调用.

        - 首次调用前若 ``_token is None`` 自动 ``login()``.
        - 收到 401 时清空 token + 重 ``login()`` + 重试一次.
        - 第二次仍 401 时抛 :class:`SnowLumaWebUIError(401, ...)`.

        Raises:
            SnowLumaWebUIError: 网络错误 / login 失败 / 401 重试后仍失败.
        """
        if self._token is None:
            self.login()

        for attempt in range(2):
            try:
                resp = httpx.request(
                    method,
                    f"{self.base_url}{path}",
                    headers=self._auth_header(),
                    json=json,
                    timeout=timeout,
                    **_HTTPX_KWARGS,
                )
            except (httpx.RequestError, httpx.TimeoutException) as exc:
                raise SnowLumaWebUIError(
                    0, f"{method} {path} 请求失败 ({type(exc).__name__}: {exc})"
                ) from exc

            if resp.status_code != 401 or attempt == 1:
                return resp

            # 401: token 失效, 重 login + 重试
            logger.trace(
                f"SnowLuma WebUI 401 重试 ({method} {path})",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self._token = None
            self.login()

        raise SnowLumaWebUIError(401, f"{method} {path} 401 重试后仍失败")

    @staticmethod
    def _parse_hook_info(item: dict[str, Any]) -> HookProcessInfo:
        return HookProcessInfo(
            pid=int(item.get("pid", 0)),
            name=str(item.get("name", "")),
            path=str(item.get("path", "")),
            uin=str(item.get("uin", "")),
            status=str(item.get("status", "available")),
            error=str(item.get("error", "")),
        )
