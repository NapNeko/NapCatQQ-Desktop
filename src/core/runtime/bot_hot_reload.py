# -*- coding: utf-8 -*-
"""配置热推送 (2026-05-11 问题 2 修复).

Desktop 用户在 UI 修改 Bot 配置 → 保存 → 写 ``bot.json`` → 渲染
``onebot11_<qqid>.json`` / ``napcat_<qqid>.json`` (NapCat) / ``onebot_<qqid>.json``
(SnowLuma). 但**正在跑的** Bot 进程是从启动时**读盘一次**, 之后 Desktop 改盘内容
后端是感知不到的, 必须重启 Bot 才生效.

本模块在 ``update_config`` 成功后调一次 :func:`push_hot_reload`, 通过 NapCat / SnowLuma
的 WebUI 接口把新配置**热推送**给在跑的 Bot, 由后端自己 ``reloadConfig`` 应用. 用户不
用重启 Bot.

设计要点
========

- **后台 worker**: HTTP 调用走 :class:`QThreadPool`, 主线程不阻塞.
- **fire-and-forget**: 失败不阻塞用户保存配置流程; 失败时通过信号让 UI 弹 ``info_bar``
  提示 "配置已保存, 但热推送失败, 请重启 Bot 生效".
- **后端路由**: 按 ``config.bot.backend_type`` 分流; 远端 Bot (``is_remote=True``)
  跳过 (远端走 SFTP 同步 + SSH 重启路径, 本模块不处理).
- **未启动 Bot**: ``BotProcessManager.get_process`` 返回 None, 直接 no-op 不弹通知
  (静默成功; 配置已落盘, 下次启动生效).

参考
====

- NapCat: ``POST /api/OB11Config/SetConfig`` (需 QQ 已登录),
  上游实现 ``example/NapCatQQ-main/packages/napcat-webui-backend/src/api/OB11Config.ts:38-60``
- SnowLuma: ``POST /api/config/:uin`` (无登录依赖),
  上游实现 ``example/SnowLuma-main/packages/core/src/webui/server.ts:411-427``
"""
from __future__ import annotations

# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal

# 项目内模块导入
from src.core.logging import LogSource, LogType, logger
from src.core.logging.crash_bundle import mask_qqid
from src.core.runtime.backend_type import BackendType
from src.core.runtime.paths import PathFunc

if TYPE_CHECKING:
    from src.core.config.config_model import Config


class HotReloadResult:
    """单次热推送的结果 (UI 友好).

    Attributes:
        ok: 是否成功推送 (HTTP 调用 + 后端确认).
        reloaded: 后端是否当场热重载 (SnowLuma 返回; NapCat 没这个字段, 始终视为 True).
        not_running: Bot 当前未在跑, 跳过推送 (不是错误, 配置已落盘).
        not_logged_in: NapCat 后端检测到 QQ 未登录, 推送被拒绝 (典型场景: Bot 启动但
            用户还没扫码).
        error_message: ``ok=False`` 时的错误描述, 适合直接弹 ``info_bar``.
    """

    __slots__ = ("ok", "reloaded", "not_running", "not_logged_in", "error_message")

    def __init__(
        self,
        *,
        ok: bool = False,
        reloaded: bool = False,
        not_running: bool = False,
        not_logged_in: bool = False,
        error_message: str = "",
    ) -> None:
        self.ok = ok
        self.reloaded = reloaded
        self.not_running = not_running
        self.not_logged_in = not_logged_in
        self.error_message = error_message


class HotReloadSignals(QObject):
    """worker → 主线程的信号桥.

    Signals:
        finished: ``(qq_id: str, result: HotReloadResult)`` —
            worker 完成时 emit (任意结果), 主线程接到后据 ``result`` 弹相应通知.
    """

    finished = Signal(str, object)


class _HotReloadRunnable(QRunnable):
    """后台 worker: 实际跑 HTTP 推送, 完成后 emit ``finished``.

    不持 ``BotProcessManager`` 引用避免循环依赖; 启动时拿到 backend + 网络配置后,
    worker 内自己构造 / 复用 WebUI client.
    """

    __slots__ = ("_qq_id", "_backend", "_payload", "_signals")

    def __init__(
        self,
        *,
        qq_id: str,
        backend: BackendType,
        payload: dict,
        signals: HotReloadSignals,
    ) -> None:
        super().__init__()
        self._qq_id = qq_id
        self._backend = backend
        self._payload = payload
        self._signals = signals
        # QThreadPool 默认 autoDelete=True, runnable 跑完自动 GC; signals 由调用方持有.

    def run(self) -> None:  # noqa: D401 - QRunnable 协议
        try:
            if self._backend == BackendType.SNOWLUMA:
                result = self._push_snowluma()
            else:
                result = self._push_napcat()
        except Exception as exc:  # noqa: BLE001 - worker 不能让任何异常逃逸到 QThreadPool
            logger.warning(
                (
                    f"配置热推送 worker 未知异常 (qq_id={mask_qqid(self._qq_id)}, "
                    f"backend={self._backend.value}): {type(exc).__name__}: {exc}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            result = HotReloadResult(
                ok=False,
                error_message=f"{type(exc).__name__}: {exc}",
            )
        self._signals.finished.emit(self._qq_id, result)

    def _push_snowluma(self) -> HotReloadResult:
        """通过 SnowLuma daemon 共享的 WebUI client 推送配置."""
        # 延迟 import 避免循环依赖 (bot_process_manager 导入本模块时会引入间接依赖)
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon
        from src.core.runtime.snowluma_webui_client import SnowLumaWebUIError

        daemon = it(SnowLumaDaemon)
        try:
            client = daemon.webui_client()
        except RuntimeError:
            # daemon 未 READY (例如 Bot 显示在跑但 daemon 还在 STARTING). 跳过推送,
            # 让用户重启 Bot 生效. 这种边界场景实际很短暂, 不弹错误.
            logger.info(
                (
                    f"SnowLuma daemon 未就绪, 跳过热推送 "
                    f"(qq_id={mask_qqid(self._qq_id)}); 用户需重启 Bot 生效"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return HotReloadResult(
                ok=False,
                error_message="SnowLuma daemon 未就绪, 请重启 Bot 生效",
            )

        try:
            reloaded = client.update_onebot_config(int(self._qq_id), self._payload)
        except SnowLumaWebUIError as exc:
            return HotReloadResult(
                ok=False,
                error_message=f"SnowLuma 热推送失败: {exc.message}",
            )
        return HotReloadResult(ok=True, reloaded=reloaded)

    def _push_napcat(self) -> HotReloadResult:
        """构造 NapCat WebUI client 推送配置.

        NapCat 没有像 SnowLuma daemon 那样的进程级 client 单例 (NapCat 每个 Bot 都是
        独立的 NTQQ 注入式进程, 共用 webui.json). 这里每次推送 lazy 构造一个 client,
        login + push + 丢弃.
        """
        # 延迟 import
        from src.core.runtime.napcat_webui_client import (
            NapCatWebUIClient,
            NapCatWebUIError,
        )

        napcat_path = it(PathFunc).napcat_path
        client = NapCatWebUIClient.from_napcat_path(napcat_path)
        if client is None:
            return HotReloadResult(
                ok=False,
                error_message="未找到 NapCat WebUI 配置 (webui.json 不存在或缺 token); 请先启动 Bot",
            )

        # 先检查 QQ 是否登录; 未登录的话直接降级提示, 避免后续 set_ob11_config 抛 "Not Login"
        try:
            client.login()
        except NapCatWebUIError as exc:
            return HotReloadResult(
                ok=False,
                error_message=f"NapCat WebUI 认证失败: {exc.message}",
            )

        if not client.check_login_status():
            logger.info(
                (
                    f"NapCat QQ 未登录, 跳过热推送 "
                    f"(qq_id={mask_qqid(self._qq_id)}); 用户扫码登录后下次重启生效"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return HotReloadResult(ok=False, not_logged_in=True)

        try:
            client.set_ob11_config(self._payload)
        except NapCatWebUIError as exc:
            # 后端可能在 login 与 set 之间状态变化 (例如 QQ 掉线), 兜底当未登录
            if "Not Login" in exc.message:
                return HotReloadResult(ok=False, not_logged_in=True)
            return HotReloadResult(
                ok=False,
                error_message=f"NapCat 热推送失败: {exc.message}",
            )
        return HotReloadResult(ok=True, reloaded=True)


def _build_napcat_payload(config: "Config") -> dict:
    """构造 NapCat 端的 ``OneBotConfig`` payload.

    NapCat ``OneBotConfig`` schema (上游 ``onebot/config.ts:88``):
    ``{network, musicSignUrl, enableLocalFile2Url, parseMultMsg}``.

    Returns:
        与 NapCat ``OB11Config/SetConfig`` 期望格式一致的 dict.
    """
    from src.core.config.config_model import json_payload

    return {
        "network": json_payload(config.connect),
        "musicSignUrl": config.bot.musicSignUrl,
        "enableLocalFile2Url": config.advanced.enableLocalFile2Url,
        "parseMultMsg": config.advanced.parseMultMsg,
    }


def _build_snowluma_payload(config: "Config") -> dict:
    """构造 SnowLuma 端的 ``OneBotConfig`` payload (与 :func:`render_onebot_json` 一致)."""
    # 直接复用 renderer 的内部映射, 保证写盘 / 热推送格式同源
    from src.core.runtime.snowluma_config_renderer import (
        _build_fallback_networks,
        _render_http_client,
        _render_http_server,
        _render_ws_client,
        _render_ws_server,
    )

    connect = config.connect
    if not connect.httpServers and not connect.websocketServers:
        networks = _build_fallback_networks()
        networks["httpClients"] = [_render_http_client(c) for c in connect.httpClients]
        networks["wsClients"] = [_render_ws_client(c) for c in connect.websocketClients]
    else:
        networks = {
            "httpServers": [_render_http_server(s) for s in connect.httpServers],
            "httpClients": [_render_http_client(c) for c in connect.httpClients],
            "wsServers": [_render_ws_server(s) for s in connect.websocketServers],
            "wsClients": [_render_ws_client(c) for c in connect.websocketClients],
        }
    return {
        "networks": networks,
        "musicSignUrl": config.bot.musicSignUrl,
    }


def push_hot_reload(config: "Config", signals: HotReloadSignals) -> bool:
    """对在跑的本地 Bot 触发后端**热推送配置**.

    必须**主线程**调用 (内部访问 ``BotProcessManager`` / ``PathFunc``).

    流程:

    1. 远端 Bot (``is_remote=True``) → 跳过 (远端走 SFTP + SSH 重启路径, 本模块不接入);
    2. Bot 未在跑 (``BotProcessManager.get_process`` 返回 None) → 跳过, 返回 ``False``;
    3. 构造 backend-specific payload + 提交后台 worker → 立即返回 ``True``;
    4. worker 完成后 emit ``signals.finished(qq_id, HotReloadResult)``, 主线程接到后据
       结果弹相应通知 (success_bar / info_bar / 静默).

    Args:
        config: 当前要推送的 Bot 配置 (来自 ``ConfigPage.get_config`` 合并后的版本).
        signals: 调用方提供的 :class:`HotReloadSignals` 实例; ``finished`` 槽**必须**
            提前接好, 否则信号 emit 后无人接收.

    Returns:
        ``True`` 表示**已提交**到后台 worker (并非已成功);
        ``False`` 表示 fast-skip (远端 / 未在跑 / 不需要推送), 调用方无需等待信号.
    """
    # 延迟 import 避免循环 (bot_process_manager 内部不依赖本模块, 但导入路径长)
    from src.core.runtime.bot_process_manager import BotProcessManager

    qq_id = str(config.bot.QQID)

    # 远端 Bot 走独立路径, 不参与本地热推送
    if config.bot.is_remote:
        logger.trace(
            f"远端 Bot 跳过本地热推送 (qq_id={mask_qqid(qq_id)})",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return False

    manager = it(BotProcessManager)
    process_record = manager.get_process(qq_id)
    if process_record is None:
        logger.trace(
            f"Bot 未在跑, 跳过热推送 (qq_id={mask_qqid(qq_id)})",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return False

    backend = config.bot.backend_type
    if backend == BackendType.SNOWLUMA:
        payload = _build_snowluma_payload(config)
    else:
        payload = _build_napcat_payload(config)

    logger.info(
        (
            f"提交配置热推送 worker (qq_id={mask_qqid(qq_id)}, "
            f"backend={backend.value})"
        ),
        LogType.NETWORK,
        LogSource.CORE,
    )
    worker = _HotReloadRunnable(
        qq_id=qq_id,
        backend=backend,
        payload=payload,
        signals=signals,
    )
    QThreadPool.globalInstance().start(worker)
    return True
