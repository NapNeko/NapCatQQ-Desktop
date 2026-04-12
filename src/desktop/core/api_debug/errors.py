# -*- coding: utf-8 -*-
"""接口调试错误封装。"""

# 标准库导入
from typing import Any

# 第三方库导入
import httpx

# 项目内模块导入
from src.desktop.core.api_debug.models import ApiDebugError, ApiDebugErrorKind


class ApiDebugException(Exception):
    """接口调试基础异常。"""


class ApiDebugRequestBuildError(ApiDebugException):
    """请求构造异常。"""


class ApiDebugAuthError(ApiDebugException):
    """认证注入异常。"""


class ApiDebugHistoryError(ApiDebugException):
    """历史记录异常。"""


class ApiDebugErrorMiddleware:
    """统一封装接口调试错误。"""

    def wrap_exception(self, error: Exception) -> ApiDebugError:
        """将异常转换为统一错误模型。"""
        if isinstance(error, ApiDebugRequestBuildError):
            return ApiDebugError(kind=ApiDebugErrorKind.REQUEST_BUILD, message=str(error))

        if isinstance(error, ApiDebugAuthError):
            return ApiDebugError(kind=ApiDebugErrorKind.AUTH, message=str(error))

        if isinstance(error, ApiDebugHistoryError):
            return ApiDebugError(kind=ApiDebugErrorKind.HISTORY, message=str(error))

        if isinstance(error, httpx.TimeoutException):
            return ApiDebugError(kind=ApiDebugErrorKind.TIMEOUT, message=str(error))

        if isinstance(error, httpx.HTTPStatusError):
            return ApiDebugError(
                kind=ApiDebugErrorKind.HTTP_STATUS,
                message=str(error),
                status_code=error.response.status_code if error.response is not None else None,
            )

        if isinstance(error, httpx.RequestError):
            return ApiDebugError(
                kind=ApiDebugErrorKind.NETWORK,
                message=str(error),
                details={"url": str(error.request.url)} if error.request is not None else {},
            )

        return ApiDebugError(
            kind=ApiDebugErrorKind.UNKNOWN,
            message=str(error) or error.__class__.__name__,
            details={"type": error.__class__.__name__},
        )

    def build_http_status_error(
        self,
        *,
        status_code: int,
        reason_phrase: str,
        details: dict[str, Any] | None = None,
    ) -> ApiDebugError:
        """构造 HTTP 状态码错误。"""
        message = f"HTTP {status_code}"
        if reason_phrase:
            message = f"{message} {reason_phrase}"

        return ApiDebugError(
            kind=ApiDebugErrorKind.HTTP_STATUS,
            message=message,
            status_code=status_code,
            details=details or {},
        )
