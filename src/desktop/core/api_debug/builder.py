# -*- coding: utf-8 -*-
"""接口调试请求构造器。"""

# 标准库导入
import json
from typing import Any

# 第三方库导入
import httpx

# 项目内模块导入
from src.core.api_debug.errors import ApiDebugRequestBuildError
from src.core.api_debug.models import ApiDebugBodyType, ApiDebugBuiltRequest, ApiDebugRequestConfig


class ApiDebugRequestBuilder:
    """请求构造器。"""

    def build(self, config: ApiDebugRequestConfig) -> ApiDebugBuiltRequest:
        """构造归一化请求对象。"""
        method = str(config.method or "").strip().upper()
        if not method:
            raise ApiDebugRequestBuildError("请求 Method 不能为空")

        url = str(config.url or "").strip()
        if not url:
            raise ApiDebugRequestBuildError("请求 URL 不能为空")

        try:
            normalized_url = str(httpx.URL(url))
        except Exception as error:
            raise ApiDebugRequestBuildError(f"请求 URL 非法: {error}") from error

        headers = self._normalize_mapping(config.headers)
        query_params = self._normalize_mapping(config.query_params)
        body_type = config.body_type if isinstance(config.body_type, ApiDebugBodyType) else ApiDebugBodyType.NONE
        body = self._normalize_body(body_type, config.body)
        headers = self._apply_default_content_type(headers, body_type)

        return ApiDebugBuiltRequest(
            method=method,
            url=normalized_url,
            headers=headers,
            query_params=query_params,
            body_type=body_type,
            body=body,
            timeout=float(config.timeout),
            follow_redirects=bool(config.follow_redirects),
        )

    @staticmethod
    def build_fallback(config: ApiDebugRequestConfig) -> ApiDebugBuiltRequest:
        """为失败场景构造最小请求快照。"""
        method = str(config.method or "GET").strip().upper() or "GET"
        return ApiDebugBuiltRequest(
            method=method,
            url=str(config.url or "").strip(),
            headers=ApiDebugRequestBuilder._normalize_mapping(config.headers),
            query_params=ApiDebugRequestBuilder._normalize_mapping(config.query_params),
            body_type=config.body_type if isinstance(config.body_type, ApiDebugBodyType) else ApiDebugBodyType.NONE,
            body=config.body,
            timeout=float(config.timeout),
            follow_redirects=bool(config.follow_redirects),
        )

    @staticmethod
    def _normalize_mapping(payload: dict[str, Any] | None) -> dict[str, str]:
        """归一化 Header / Query 字典。"""
        if not payload:
            return {}

        normalized: dict[str, str] = {}
        for key, value in payload.items():
            if key is None:
                continue
            normalized[str(key)] = "" if value is None else str(value)
        return normalized

    def _normalize_body(self, body_type: ApiDebugBodyType, body: Any) -> Any:
        """根据请求体类型归一化 Body。"""
        if body_type == ApiDebugBodyType.NONE:
            return None

        if body_type == ApiDebugBodyType.JSON:
            if body is None or body == "":
                return None
            if isinstance(body, str):
                try:
                    return json.loads(body)
                except json.JSONDecodeError as error:
                    raise ApiDebugRequestBuildError(f"JSON 请求体格式错误: {error}") from error
            return body

        if body_type == ApiDebugBodyType.FORM:
            if body is None:
                return {}
            if isinstance(body, str):
                try:
                    parsed = json.loads(body)
                except json.JSONDecodeError as error:
                    raise ApiDebugRequestBuildError(f"FORM 请求体必须是对象或 JSON 字符串: {error}") from error

                if not isinstance(parsed, dict):
                    raise ApiDebugRequestBuildError("FORM 请求体必须解析为对象")
                return parsed

            if isinstance(body, dict):
                return body

            raise ApiDebugRequestBuildError("FORM 请求体必须是 dict 或 JSON 字符串")

        if body_type == ApiDebugBodyType.TEXT:
            if body is None:
                return ""
            if isinstance(body, str):
                return body
            return json.dumps(body, ensure_ascii=False)

        if body_type == ApiDebugBodyType.BYTES:
            if body is None:
                return b""
            if isinstance(body, bytes):
                return body
            if isinstance(body, str):
                return body.encode("utf-8")
            raise ApiDebugRequestBuildError("BYTES 请求体必须是 bytes 或 str")

        return body

    @staticmethod
    def _apply_default_content_type(headers: dict[str, str], body_type: ApiDebugBodyType) -> dict[str, str]:
        """在缺省时补齐合理的 Content-Type。"""
        normalized_headers = dict(headers)
        if any(key.lower() == "content-type" for key in normalized_headers):
            return normalized_headers

        if body_type == ApiDebugBodyType.JSON:
            normalized_headers["Content-Type"] = "application/json"
        elif body_type == ApiDebugBodyType.FORM:
            normalized_headers["Content-Type"] = "application/x-www-form-urlencoded"
        elif body_type == ApiDebugBodyType.TEXT:
            normalized_headers["Content-Type"] = "text/plain; charset=utf-8"

        return normalized_headers
