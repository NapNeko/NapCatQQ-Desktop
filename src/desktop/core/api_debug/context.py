# -*- coding: utf-8 -*-
"""接口调试页面的 Bot 上下文解析。"""

# 标准库导入
from typing import Callable

# 第三方库导入
from creart import it

# 项目内模块导入
from src.core.api_debug.auth import create_bearer_auth_from_network_config, create_webui_auth_from_login_state
from src.core.api_debug.models import ApiDebugBotContext, ApiDebugEndpointSummary, ApiDebugTargetType
from src.core.config.operate_config import read_config
from src.core.runtime.napcat import ManagerNapCatQQLoginState


ConfigReader = Callable[[], list[object]]


class ApiDebugContextService:
    """从现有 Bot 配置和运行态信息推导调试上下文。"""

    def __init__(
        self,
        *,
        config_reader: ConfigReader | None = None,
        login_state_provider: object | None = None,
    ) -> None:
        self.config_reader = config_reader or read_config
        self.login_state_provider = login_state_provider

    def list_bot_contexts(self) -> list[ApiDebugBotContext]:
        """枚举所有可调试的 Bot 上下文。"""
        contexts: list[ApiDebugBotContext] = []
        login_manager = self._get_login_manager()

        for config in self.config_reader():
            bot_id = str(getattr(getattr(config, "bot", None), "QQID", ""))
            bot_name = str(getattr(getattr(config, "bot", None), "name", bot_id))
            login_state = login_manager.get_login_state(bot_id) if login_manager is not None else None

            context = ApiDebugBotContext(
                bot_id=bot_id,
                bot_name=bot_name,
                http_targets=self._build_http_targets(config, login_state),
                websocket_targets=self._build_websocket_targets(config, login_state),
                webui_base_url=self._build_webui_base_url(login_state),
                webui_token=str(getattr(login_state, "token", "") or "").strip(),
                webui_credential=str(getattr(login_state, "auth", "") or "").strip(),
            )
            contexts.append(context)

        return contexts

    def get_context(self, bot_id: str) -> ApiDebugBotContext | None:
        """按 Bot ID 获取上下文。"""
        normalized = str(bot_id).strip()
        for context in self.list_bot_contexts():
            if context.bot_id == normalized:
                return context
        return None

    def _get_login_manager(self) -> object | None:
        if self.login_state_provider is not None:
            return self.login_state_provider

        try:
            return it(ManagerNapCatQQLoginState)
        except Exception:
            return None

    def _build_http_targets(self, config: object, login_state: object | None) -> list[ApiDebugEndpointSummary]:
        targets: list[ApiDebugEndpointSummary] = []
        bot_id = str(getattr(getattr(config, "bot", None), "QQID", ""))
        connect = getattr(config, "connect", None)

        for network in getattr(connect, "httpServers", []) or []:
            if not getattr(network, "enable", False):
                continue
            host = self._normalize_host(getattr(network, "host", "127.0.0.1"))
            port = int(getattr(network, "port", 0) or 0)
            url = f"http://{host}:{port}"
            targets.append(
                ApiDebugEndpointSummary(
                    endpoint_id=f"{bot_id}:http:{getattr(network, 'name', port)}",
                    name=f"{getattr(network, 'name', 'HTTP')} · OneBot HTTP",
                    url=url,
                    target_type=ApiDebugTargetType.ONEBOT_HTTP,
                    auth_config=create_bearer_auth_from_network_config(network),
                    description="OneBot HTTP 服务端",
                )
            )

        webui_base_url = self._build_webui_base_url(login_state)
        if webui_base_url:
            targets.append(
                ApiDebugEndpointSummary(
                    endpoint_id=f"{bot_id}:webui",
                    name="运行中 WebUI",
                    url=webui_base_url,
                    target_type=ApiDebugTargetType.WEBUI_HTTP,
                    auth_config=create_webui_auth_from_login_state(login_state),
                    description="NapCat WebUI 后端入口",
                )
            )

        return targets

    def _build_websocket_targets(self, config: object, login_state: object | None) -> list[ApiDebugEndpointSummary]:
        targets: list[ApiDebugEndpointSummary] = []
        bot_id = str(getattr(getattr(config, "bot", None), "QQID", ""))
        connect = getattr(config, "connect", None)

        for network in getattr(connect, "websocketServers", []) or []:
            if not getattr(network, "enable", False):
                continue
            host = self._normalize_host(getattr(network, "host", "127.0.0.1"))
            port = int(getattr(network, "port", 0) or 0)
            url = f"ws://{host}:{port}"
            targets.append(
                ApiDebugEndpointSummary(
                    endpoint_id=f"{bot_id}:ws:{getattr(network, 'name', port)}",
                    name=f"{getattr(network, 'name', 'WebSocket')} · OneBot WS",
                    url=url,
                    target_type=ApiDebugTargetType.ONEBOT_WEBSOCKET,
                    auth_config=create_bearer_auth_from_network_config(network, use_query_token=True),
                    description="独立 OneBot WebSocket 服务端",
                )
            )

        for network in getattr(connect, "httpServers", []) or []:
            if not getattr(network, "enable", False) or not getattr(network, "enableWebsocket", False):
                continue
            host = self._normalize_host(getattr(network, "host", "127.0.0.1"))
            port = int(getattr(network, "port", 0) or 0)
            url = f"ws://{host}:{port}"
            targets.append(
                ApiDebugEndpointSummary(
                    endpoint_id=f"{bot_id}:http-ws:{getattr(network, 'name', port)}",
                    name=f"{getattr(network, 'name', 'HTTP')} · HTTP/WS",
                    url=url,
                    target_type=ApiDebugTargetType.ONEBOT_WEBSOCKET,
                    auth_config=create_bearer_auth_from_network_config(network, use_query_token=True),
                    description="由 HTTP 服务端暴露的 WebSocket 调试入口",
                )
            )

        webui_base_url = self._build_webui_base_url(login_state)
        if webui_base_url:
            targets.append(
                ApiDebugEndpointSummary(
                    endpoint_id=f"{bot_id}:debug-ws",
                    name="运行中 Debug WS",
                    url=webui_base_url,
                    target_type=ApiDebugTargetType.DEBUG_WEBSOCKET,
                    description="WebUI DebugAdapter 实时调试入口",
                )
            )

        return targets

    @staticmethod
    def _build_webui_base_url(login_state: object | None) -> str:
        if login_state is None:
            return ""
        port = getattr(login_state, "port", None)
        if port in {None, ""}:
            return ""
        return f"http://127.0.0.1:{int(port)}"

    @staticmethod
    def _normalize_host(host: str) -> str:
        normalized = str(host or "").strip()
        if normalized in {"", "0.0.0.0", "::", "[::]"}:
            return "127.0.0.1"
        return normalized
