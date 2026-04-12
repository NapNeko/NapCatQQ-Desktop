# -*- coding: utf-8 -*-
"""接口调试服务编排层。"""

# 标准库导入
from datetime import datetime, timezone
from typing import Callable

# 第三方库导入
import httpx

# 项目内模块导入
from src.desktop.core.api_debug.auth import ApiDebugAuthInjector
from src.desktop.core.api_debug.builder import ApiDebugRequestBuilder
from src.desktop.core.api_debug.errors import ApiDebugErrorMiddleware
from src.desktop.core.api_debug.history import ApiDebugHistoryManager
from src.desktop.core.api_debug.models import ApiDebugAuthConfig, ApiDebugExecutionResult, ApiDebugRequestConfig
from src.desktop.core.api_debug.parser import ApiDebugResponseParser
from src.desktop.core.logging import LogSource, LogType, logger
from src.desktop.core.logging.crash_bundle import summarize_url

ClientFactory = Callable[..., httpx.Client]


class ApiDebugService:
    """统一协调请求构造、认证注入、发送、解析和历史记录。"""

    def __init__(
        self,
        *,
        request_builder: ApiDebugRequestBuilder | None = None,
        auth_injector: ApiDebugAuthInjector | None = None,
        response_parser: ApiDebugResponseParser | None = None,
        error_middleware: ApiDebugErrorMiddleware | None = None,
        history_manager: ApiDebugHistoryManager | None = None,
        client_factory: ClientFactory | None = None,
    ) -> None:
        self.request_builder = request_builder or ApiDebugRequestBuilder()
        self.auth_injector = auth_injector or ApiDebugAuthInjector()
        self.response_parser = response_parser or ApiDebugResponseParser()
        self.error_middleware = error_middleware or ApiDebugErrorMiddleware()
        self.history_manager = history_manager or ApiDebugHistoryManager()
        self.client_factory = client_factory or httpx.Client

    def execute(
        self,
        request_config: ApiDebugRequestConfig,
        auth_config: ApiDebugAuthConfig | None = None,
        *,
        persist_history: bool = True,
    ) -> ApiDebugExecutionResult:
        """执行一次接口调试请求。"""
        started_at = datetime.now(timezone.utc)
        request = self.request_builder.build_fallback(request_config)

        try:
            request = self.request_builder.build(request_config)
            request = self.auth_injector.inject(request, auth_config)
            logger.info(
                f"开始接口调试请求: method={request.method}, url={summarize_url(request.url)}",
                LogType.NETWORK,
                LogSource.CORE,
            )

            with self.client_factory(timeout=request.timeout, follow_redirects=request.follow_redirects) as client:
                response = client.request(**request.to_httpx_kwargs())

            parsed_response = self.response_parser.parse(response)
            result = ApiDebugExecutionResult(
                request=request,
                response=parsed_response,
                started_at=started_at,
                completed_at=datetime.now(timezone.utc),
            )

            if response.status_code >= 400:
                result.error = self.error_middleware.build_http_status_error(
                    status_code=response.status_code,
                    reason_phrase=response.reason_phrase,
                    details={"url": request.url},
                )

            if persist_history:
                self._persist_history(result)

            logger.info(
                (
                    "接口调试请求结束: "
                    f"method={request.method}, url={summarize_url(request.url)}, "
                    f"status={parsed_response.status_code}, success={result.is_success}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return result

        except Exception as error:
            wrapped_error = self.error_middleware.wrap_exception(error)
            result = ApiDebugExecutionResult(
                request=request,
                error=wrapped_error,
                started_at=started_at,
                completed_at=datetime.now(timezone.utc),
            )

            if persist_history:
                self._persist_history(result)

            logger.error(
                (
                    "接口调试请求失败: "
                    f"method={request.method}, url={summarize_url(request.url)}, "
                    f"kind={wrapped_error.kind.value}, error={wrapped_error.message}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return result

    def _persist_history(self, result: ApiDebugExecutionResult) -> None:
        try:
            entry = self.history_manager.append_result(result)
        except Exception as error:
            history_error = self.error_middleware.wrap_exception(error)
            if result.error is None:
                result.error = history_error
            logger.error(f"写入接口调试历史记录失败: {history_error.message}", LogType.NETWORK, LogSource.CORE)
            return

        result.history_id = entry.history_id
