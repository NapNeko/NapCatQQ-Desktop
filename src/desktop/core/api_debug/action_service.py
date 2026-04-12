# -*- coding: utf-8 -*-
"""WebUI Action 调试服务。"""

# 标准库导入
from typing import Any

# 项目内模块导入
from src.desktop.core.api_debug.models import ApiDebugActionSchema, ApiDebugActionSession, ApiDebugAuthConfig
from src.desktop.core.api_debug.service import ApiDebugService
from src.desktop.core.api_debug.errors import ApiDebugException
from src.desktop.core.api_debug.models import ApiDebugBodyType, ApiDebugRequestConfig


class ActionDebugService:
    """封装 WebUI Debug API 的 schema、session 和 call。"""

    def __init__(self, api_service: ApiDebugService | None = None) -> None:
        self.api_service = api_service or ApiDebugService()

    def fetch_schemas(
        self,
        base_url: str,
        auth_config: ApiDebugAuthConfig | None = None,
        *,
        timeout: float = 10.0,
    ) -> list[ApiDebugActionSchema]:
        """获取 Action Schema。"""
        payload = self._request_json(
            ApiDebugRequestConfig(
                url=f"{base_url.rstrip('/')}/api/Debug/schemas",
                method="GET",
                timeout=timeout,
            ),
            auth_config,
        )

        if not isinstance(payload, dict):
            raise ApiDebugException("Debug Schema 响应格式非法")

        schemas: list[ApiDebugActionSchema] = []
        for action_name, item in payload.items():
            if not isinstance(item, dict):
                continue
            schemas.append(
                ApiDebugActionSchema(
                    action=str(action_name),
                    summary=str(item.get("description", "") or ""),
                    description=str(item.get("description", "") or ""),
                    payload_schema=item.get("payload"),
                    return_schema=item.get("response"),
                    payload_example=item.get("payloadExample"),
                    action_tags=[str(tag) for tag in item.get("tags", []) if tag is not None],
                )
            )

        schemas.sort(key=lambda item: item.action)
        return schemas

    def create_session(
        self,
        base_url: str,
        auth_config: ApiDebugAuthConfig | None = None,
        *,
        timeout: float = 10.0,
    ) -> ApiDebugActionSession:
        """创建或获取 DebugAdapter 会话。"""
        payload = self._request_json(
            ApiDebugRequestConfig(
                url=f"{base_url.rstrip('/')}/api/Debug/create",
                method="POST",
                body_type=ApiDebugBodyType.JSON,
                body={},
                timeout=timeout,
            ),
            auth_config,
        )
        if not isinstance(payload, dict):
            raise ApiDebugException("调试会话响应格式非法")

        adapter_name = str(payload.get("adapterName", "") or "").strip()
        token = str(payload.get("token", "") or "").strip()
        if not adapter_name or not token:
            raise ApiDebugException("调试会话缺少 adapterName 或 token")

        return ApiDebugActionSession(adapter_name=adapter_name, token=token, base_url=base_url.rstrip("/"))

    def call_action(
        self,
        session: ApiDebugActionSession,
        action: str,
        params: Any,
        auth_config: ApiDebugAuthConfig | None = None,
        *,
        timeout: float = 10.0,
        persist_history: bool = True,
    ):
        """通过 DebugAdapter 调用 Action。"""
        request = ApiDebugRequestConfig(
            url=f"{session.base_url.rstrip('/')}/api/Debug/call/{session.adapter_name}",
            method="POST",
            body_type=ApiDebugBodyType.JSON,
            body={"action": action, "params": params},
            timeout=timeout,
        )
        return self.api_service.execute(request, auth_config, persist_history=persist_history)

    def _request_json(
        self,
        request_config: ApiDebugRequestConfig,
        auth_config: ApiDebugAuthConfig | None,
    ) -> Any:
        result = self.api_service.execute(request_config, auth_config, persist_history=False)
        if result.error is not None:
            raise ApiDebugException(result.error.message)
        if result.response is None:
            raise ApiDebugException("调试请求未返回响应")
        if result.response.json_body is None:
            raise ApiDebugException("调试请求未返回 JSON")

        payload = result.response.json_body
        if isinstance(payload, dict) and "data" in payload:
            return payload.get("data")
        return payload
