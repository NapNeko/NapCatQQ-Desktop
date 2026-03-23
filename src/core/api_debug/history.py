# -*- coding: utf-8 -*-
"""接口调试历史记录管理。"""

# 标准库导入
import json
import os
import uuid
from dataclasses import asdict
from pathlib import Path
from typing import Any

# 第三方库导入
from creart import it

# 项目内模块导入
from src.core.api_debug.errors import ApiDebugHistoryError
from src.core.api_debug.models import (
    ApiDebugBuiltRequest,
    ApiDebugError,
    ApiDebugErrorKind,
    ApiDebugExecutionResult,
    ApiDebugHistoryEntry,
    ApiDebugHistoryRequestSnapshot,
    ApiDebugHistoryResponseSnapshot,
)
from src.core.logging import logger
from src.core.runtime.paths import PathFunc

_SENSITIVE_FIELD_NAMES = {
    "authorization",
    "token",
    "access_token",
    "webui_token",
    "credential",
    "cookie",
    "set-cookie",
}


class ApiDebugHistoryManager:
    """接口调试历史记录管理器。"""

    def __init__(self, storage_path: Path | None = None, max_entries: int = 100) -> None:
        self.storage_path = storage_path or (it(PathFunc).config_dir_path / "api_debug_history.json")
        self.max_entries = max_entries

    def append_result(self, result: ApiDebugExecutionResult) -> ApiDebugHistoryEntry:
        """追加一条执行结果到历史记录。"""
        entry = self._build_entry(result)
        entries = self.list_entries()
        entries.insert(0, entry)
        self._write_entries(entries[: self.max_entries])
        return entry

    def list_entries(self, limit: int | None = None) -> list[ApiDebugHistoryEntry]:
        """读取历史记录列表。"""
        payload = self._read_payload()
        entries = [self._entry_from_dict(item) for item in payload.get("entries", [])]
        return entries[:limit] if limit is not None else entries

    def get_entry(self, history_id: str) -> ApiDebugHistoryEntry | None:
        """按 ID 获取历史记录。"""
        for entry in self.list_entries():
            if entry.history_id == history_id:
                return entry
        return None

    def delete_entry(self, history_id: str) -> bool:
        """删除指定历史记录。"""
        entries = self.list_entries()
        remaining = [entry for entry in entries if entry.history_id != history_id]
        if len(remaining) == len(entries):
            return False
        self._write_entries(remaining)
        return True

    def clear(self) -> None:
        """清空历史记录。"""
        self._write_entries([])

    def _build_entry(self, result: ApiDebugExecutionResult) -> ApiDebugHistoryEntry:
        request_snapshot = self._build_request_snapshot(result.request)
        response_snapshot = None
        if result.response is not None:
            response_snapshot = ApiDebugHistoryResponseSnapshot(
                status_code=result.response.status_code,
                reason_phrase=result.response.reason_phrase,
                headers=self._sanitize_mapping(result.response.headers),
                body_type=result.response.body_type.value,
                formatted_body=result.response.formatted_body,
                elapsed_ms=result.response.elapsed_ms,
                size_bytes=result.response.size_bytes,
            )

        return ApiDebugHistoryEntry(
            history_id=uuid.uuid4().hex,
            created_at=result.completed_at.isoformat(),
            request=request_snapshot,
            response=response_snapshot,
            error=result.error,
        )

    def _build_request_snapshot(self, request: ApiDebugBuiltRequest) -> ApiDebugHistoryRequestSnapshot:
        return ApiDebugHistoryRequestSnapshot(
            method=request.method,
            url=request.url,
            headers=self._sanitize_mapping(request.headers),
            query_params=self._sanitize_mapping(request.query_params),
            body_type=request.body_type.value,
            body=self._sanitize_body(request.body),
        )

    def _read_payload(self) -> dict[str, Any]:
        try:
            return json.loads(self.storage_path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return {"version": 1, "entries": []}
        except json.JSONDecodeError as error:
            logger.error(f"读取接口调试历史记录失败: {type(error).__name__}: {error}")
            return {"version": 1, "entries": []}
        except OSError as error:
            raise ApiDebugHistoryError(f"读取接口调试历史记录失败: {error}") from error

    def _write_entries(self, entries: list[ApiDebugHistoryEntry]) -> None:
        payload = {
            "version": 1,
            "entries": [self._entry_to_dict(entry) for entry in entries],
        }
        self.storage_path.parent.mkdir(parents=True, exist_ok=True)
        temp_path = self.storage_path.with_name(f"{self.storage_path.name}.{uuid.uuid4().hex}.tmp")

        try:
            with open(temp_path, "w", encoding="utf-8") as file:
                json.dump(payload, file, ensure_ascii=False, indent=2)

            os.replace(temp_path, self.storage_path)
        except OSError as error:
            raise ApiDebugHistoryError(f"写入接口调试历史记录失败: {error}") from error
        finally:
            try:
                if temp_path.exists():
                    temp_path.unlink()
            except OSError:
                pass

    def _entry_to_dict(self, entry: ApiDebugHistoryEntry) -> dict[str, Any]:
        payload = asdict(entry)
        if isinstance(payload.get("error"), dict) and "kind" in payload["error"]:
            payload["error"]["kind"] = entry.error.kind.value if entry.error is not None else None
        return payload

    def _entry_from_dict(self, payload: dict[str, Any]) -> ApiDebugHistoryEntry:
        request_payload = payload.get("request", {})
        response_payload = payload.get("response")
        error_payload = payload.get("error")

        error = None
        if isinstance(error_payload, dict):
            raw_kind = str(error_payload.get("kind", ApiDebugErrorKind.UNKNOWN.value))
            if raw_kind.startswith("ApiDebugErrorKind."):
                raw_kind = raw_kind.rsplit(".", 1)[-1].lower()
            try:
                error_kind = ApiDebugErrorKind(raw_kind)
            except ValueError:
                error_kind = ApiDebugErrorKind.UNKNOWN
            error = ApiDebugError(
                kind=error_kind,
                message=str(error_payload.get("message", "")),
                status_code=error_payload.get("status_code"),
                details=error_payload.get("details", {}),
            )

        response = None
        if isinstance(response_payload, dict):
            response = ApiDebugHistoryResponseSnapshot(
                status_code=int(response_payload.get("status_code", 0)),
                reason_phrase=str(response_payload.get("reason_phrase", "")),
                headers=self._sanitize_mapping(response_payload.get("headers", {})),
                body_type=str(response_payload.get("body_type", "")),
                formatted_body=str(response_payload.get("formatted_body", "")),
                elapsed_ms=float(response_payload.get("elapsed_ms", 0.0)),
                size_bytes=int(response_payload.get("size_bytes", 0)),
            )

        return ApiDebugHistoryEntry(
            history_id=str(payload.get("history_id", "")),
            created_at=str(payload.get("created_at", "")),
            request=ApiDebugHistoryRequestSnapshot(
                method=str(request_payload.get("method", "")),
                url=str(request_payload.get("url", "")),
                headers=self._sanitize_mapping(request_payload.get("headers", {})),
                query_params=self._sanitize_mapping(request_payload.get("query_params", {})),
                body_type=str(request_payload.get("body_type", "")),
                body=request_payload.get("body"),
            ),
            response=response,
            error=error,
        )

    def _sanitize_mapping(self, payload: dict[str, Any] | None) -> dict[str, str]:
        if not isinstance(payload, dict):
            return {}

        sanitized: dict[str, str] = {}
        for key, value in payload.items():
            key_text = str(key)
            if key_text.strip().lower() in _SENSITIVE_FIELD_NAMES:
                sanitized[key_text] = "<redacted>"
            else:
                sanitized[key_text] = "" if value is None else str(value)
        return sanitized

    def _sanitize_body(self, body: Any) -> Any:
        if isinstance(body, bytes):
            return f"<binary body: {len(body)} bytes>"
        if isinstance(body, (str, int, float, bool)) or body is None:
            return body
        if isinstance(body, list):
            return [self._sanitize_body(item) for item in body]
        if isinstance(body, dict):
            return {str(key): self._sanitize_body(value) for key, value in body.items()}
        return str(body)
