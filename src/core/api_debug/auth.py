# -*- coding: utf-8 -*-
"""接口调试认证注入器。"""

# 标准库导入
import hashlib

# 第三方库导入
import httpx

# 项目内模块导入
from src.core.api_debug.errors import ApiDebugAuthError
from src.core.api_debug.models import ApiDebugAuthConfig, ApiDebugAuthType, ApiDebugBuiltRequest


class ApiDebugAuthInjector:
    """认证注入器。"""

    def __init__(self) -> None:
        self._credential_cache: dict[tuple[str, str], str] = {}

    def inject(self, request: ApiDebugBuiltRequest, auth_config: ApiDebugAuthConfig | None) -> ApiDebugBuiltRequest:
        """将认证信息注入请求。"""
        if auth_config is None or auth_config.auth_type == ApiDebugAuthType.NONE:
            return request

        if auth_config.auth_type == ApiDebugAuthType.BEARER_TOKEN:
            return self._inject_bearer_token(request, auth_config)

        if auth_config.auth_type == ApiDebugAuthType.WEBUI_SESSION:
            return self._inject_webui_session(request, auth_config)

        raise ApiDebugAuthError(f"不支持的认证模式: {auth_config.auth_type}")

    def clear_cache(self) -> None:
        """清理内存中的 Session Credential 缓存。"""
        self._credential_cache.clear()

    def _inject_bearer_token(self, request: ApiDebugBuiltRequest, auth_config: ApiDebugAuthConfig) -> ApiDebugBuiltRequest:
        token = auth_config.token.strip()
        if not token:
            raise ApiDebugAuthError("Bearer Token 不能为空")

        headers = dict(request.headers)
        query_params = dict(request.query_params)

        if auth_config.use_query_token:
            query_params["access_token"] = token
        else:
            headers["Authorization"] = f"Bearer {token}"

        return ApiDebugBuiltRequest(
            method=request.method,
            url=request.url,
            headers=headers,
            query_params=query_params,
            body_type=request.body_type,
            body=request.body,
            timeout=request.timeout,
            follow_redirects=request.follow_redirects,
        )

    def _inject_webui_session(self, request: ApiDebugBuiltRequest, auth_config: ApiDebugAuthConfig) -> ApiDebugBuiltRequest:
        headers = dict(request.headers)
        query_params = dict(request.query_params)

        if auth_config.use_query_token:
            token = auth_config.token.strip()
            if not token:
                raise ApiDebugAuthError("WebUI Token 不能为空")
            query_params["webui_token"] = token
        else:
            credential = auth_config.session_credential.strip() or self._get_or_create_credential(
                request.url,
                auth_config.token.strip(),
                request.timeout,
            )
            headers["Authorization"] = f"Bearer {credential}"

        return ApiDebugBuiltRequest(
            method=request.method,
            url=request.url,
            headers=headers,
            query_params=query_params,
            body_type=request.body_type,
            body=request.body,
            timeout=request.timeout,
            follow_redirects=request.follow_redirects,
        )

    def _get_or_create_credential(self, request_url: str, token: str, timeout: float) -> str:
        if not token:
            raise ApiDebugAuthError("WebUI Session 模式缺少 token")

        parsed_url = httpx.URL(request_url)
        origin = f"{parsed_url.scheme}://{parsed_url.host}"
        if parsed_url.port is not None:
            origin = f"{origin}:{parsed_url.port}"

        cache_key = (origin, token)
        if cache_key in self._credential_cache:
            return self._credential_cache[cache_key]

        response = httpx.post(
            f"{origin.rstrip('/')}/api/auth/login",
            json={"hash": hashlib.sha256((token + ".napcat").encode("utf-8")).hexdigest()},
            headers={"Content-Type": "application/json"},
            timeout=timeout,
        )

        if response.status_code != 200:
            raise ApiDebugAuthError(f"WebUI 登录失败: HTTP {response.status_code}")

        try:
            credential = str(response.json().get("data", {}).get("Credential", "")).strip()
        except ValueError as error:
            raise ApiDebugAuthError(f"WebUI 登录响应解析失败: {error}") from error

        if not credential:
            raise ApiDebugAuthError("WebUI 登录成功但未返回 Credential")

        self._credential_cache[cache_key] = credential
        return credential


def create_bearer_auth_from_token(token: str | None, *, use_query_token: bool = False) -> ApiDebugAuthConfig:
    """根据 OneBot Token 创建 Bearer 认证配置。"""
    normalized_token = str(token or "").strip()
    if not normalized_token:
        return ApiDebugAuthConfig()
    return ApiDebugAuthConfig.bearer(normalized_token, use_query_token=use_query_token)


def create_bearer_auth_from_network_config(
    network_config: object | None,
    *,
    use_query_token: bool = False,
) -> ApiDebugAuthConfig:
    """根据现有网络配置对象中的 token 构造 Bearer 认证配置。"""
    token = getattr(network_config, "token", "") if network_config is not None else ""
    return create_bearer_auth_from_token(token, use_query_token=use_query_token)


def create_webui_auth_from_login_state(login_state: object | None) -> ApiDebugAuthConfig:
    """根据运行态登录状态对象构造 WebUI 认证配置。"""
    if login_state is None:
        return ApiDebugAuthConfig()

    token = str(getattr(login_state, "token", "") or "").strip()
    credential = str(getattr(login_state, "auth", "") or "").strip()
    if not token and not credential:
        return ApiDebugAuthConfig()

    return ApiDebugAuthConfig.webui_session(
        token=token,
        session_credential=credential,
        use_query_token=False,
    )
