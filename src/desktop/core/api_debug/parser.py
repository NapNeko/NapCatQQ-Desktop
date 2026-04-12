# -*- coding: utf-8 -*-
"""接口调试响应解析器。"""

# 标准库导入
import base64
import json

# 第三方库导入
import httpx

# 项目内模块导入
from src.desktop.core.api_debug.models import ApiDebugResponse, ApiDebugResponseBodyType


class ApiDebugResponseParser:
    """响应解析器。"""

    def __init__(self, max_body_chars: int = 20000) -> None:
        self.max_body_chars = max_body_chars

    def parse(self, response: httpx.Response) -> ApiDebugResponse:
        """解析 httpx.Response。"""
        content = response.content
        body_type, formatted_body, json_body = self._parse_body(response)
        elapsed_ms = self._get_elapsed_ms(response)

        return ApiDebugResponse(
            status_code=response.status_code,
            reason_phrase=response.reason_phrase,
            headers=dict(response.headers),
            body_type=body_type,
            formatted_body=self._truncate_text(formatted_body),
            json_body=json_body,
            elapsed_ms=elapsed_ms,
            size_bytes=len(content),
        )

    def _parse_body(self, response: httpx.Response) -> tuple[ApiDebugResponseBodyType, str, object | None]:
        content = response.content
        if not content:
            return ApiDebugResponseBodyType.EMPTY, "", None

        content_type = response.headers.get("content-type", "").lower()

        if "json" in content_type:
            return self._parse_json_body(response)

        if self._looks_like_json(content):
            body_type, formatted_body, json_body = self._parse_json_body(response, strict=False)
            if body_type == ApiDebugResponseBodyType.JSON:
                return body_type, formatted_body, json_body

        if self._is_text_content_type(content_type):
            return ApiDebugResponseBodyType.TEXT, response.text, None

        preview = base64.b64encode(content[:1024]).decode("ascii")
        formatted_body = (
            f"<binary body>\nsize={len(content)} bytes\n"
            f"preview(base64, first {min(len(content), 1024)} bytes)={preview}"
        )
        return ApiDebugResponseBodyType.BINARY, formatted_body, None

    @staticmethod
    def _looks_like_json(content: bytes) -> bool:
        stripped = content.lstrip()
        return stripped.startswith((b"{", b"["))

    @staticmethod
    def _is_text_content_type(content_type: str) -> bool:
        return content_type.startswith("text/") or any(
            marker in content_type for marker in ("xml", "html", "javascript", "x-www-form-urlencoded")
        )

    def _parse_json_body(
        self,
        response: httpx.Response,
        *,
        strict: bool = True,
    ) -> tuple[ApiDebugResponseBodyType, str, object | None]:
        try:
            json_body = response.json()
        except ValueError:
            if strict:
                return ApiDebugResponseBodyType.TEXT, response.text, None
            return ApiDebugResponseBodyType.TEXT, response.text, None

        formatted_body = json.dumps(json_body, ensure_ascii=False, indent=2)
        return ApiDebugResponseBodyType.JSON, formatted_body, json_body

    @staticmethod
    def _get_elapsed_ms(response: httpx.Response) -> float:
        try:
            return round(response.elapsed.total_seconds() * 1000, 2)
        except RuntimeError:
            return 0.0

    def _truncate_text(self, text: str) -> str:
        if len(text) <= self.max_body_chars:
            return text

        suffix = f"\n...<truncated {len(text) - self.max_body_chars} chars>"
        return f"{text[:self.max_body_chars]}{suffix}"
