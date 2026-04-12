# -*- coding: utf-8 -*-
"""接口调试工作台状态持久化。"""

# 标准库导入
import json
import os
import threading
import time
import uuid
from pathlib import Path
from typing import Any
from urllib.parse import parse_qsl, urlencode, urlsplit, urlunsplit

# 第三方库导入
from creart import it

# 项目内模块导入
from src.desktop.core.api_debug.models import (
    ApiDebugAuthConfig,
    ApiDebugAuthType,
    ApiDebugBodyType,
    ApiDebugMode,
    ApiDebugPreset,
    ApiDebugTargetType,
    ApiDebugWorkspaceState,
    ApiDebugActionDraft,
    ApiDebugHttpDraft,
    ApiDebugWebSocketDraft,
)
from src.desktop.core.logging import LogSource, LogType, logger
from src.desktop.core.runtime.paths import PathFunc

_SENSITIVE_FIELDS = {
    "authorization",
    "token",
    "access_token",
    "webui_token",
    "cookie",
    "credential",
    "session_credential",
    "set-cookie",
}


class ApiDebugWorkspaceStore:
    """读写接口调试工作台状态。"""

    def __init__(self, storage_path: Path | None = None) -> None:
        self.storage_path = storage_path or (it(PathFunc).config_dir_path / "api_debug_workspace.json")
        self._write_lock = threading.Lock()

    def load(self) -> ApiDebugWorkspaceState:
        """读取状态，不存在时返回默认值。"""
        payload = self._read_payload()
        return ApiDebugWorkspaceState(
            selected_bot_id=str(payload.get("selected_bot_id", "")),
            selected_mode=self._parse_mode(payload.get("selected_mode")),
            root_splitter_sizes=self._parse_sizes(payload.get("root_splitter_sizes")),
            detail_splitter_sizes=self._parse_sizes(payload.get("detail_splitter_sizes")),
            http_draft=self._parse_http_draft(payload.get("http_draft")),
            action_draft=self._parse_action_draft(payload.get("action_draft")),
            websocket_draft=self._parse_websocket_draft(payload.get("websocket_draft")),
            presets=self._parse_presets(payload.get("presets")),
        )

    def save(self, state: ApiDebugWorkspaceState) -> None:
        """写入状态。"""
        payload = {
            "version": 1,
            "selected_bot_id": state.selected_bot_id,
            "selected_mode": state.selected_mode.value,
            "root_splitter_sizes": self._parse_sizes(state.root_splitter_sizes),
            "detail_splitter_sizes": self._parse_sizes(state.detail_splitter_sizes),
            "http_draft": self._http_draft_to_dict(state.http_draft),
            "action_draft": self._action_draft_to_dict(state.action_draft),
            "websocket_draft": self._websocket_draft_to_dict(state.websocket_draft),
            "presets": [self._preset_to_dict(preset) for preset in state.presets],
        }
        self._write_payload(payload)

    def upsert_preset(
        self,
        state: ApiDebugWorkspaceState,
        *,
        name: str,
        mode: ApiDebugMode,
        payload: dict[str, Any],
        summary: str = "",
        preset_id: str | None = None,
    ) -> ApiDebugPreset:
        """创建或更新预设，并立即写回状态。"""
        normalized_id = preset_id or uuid.uuid4().hex
        preset = ApiDebugPreset(
            preset_id=normalized_id,
            name=name.strip() or "未命名预设",
            mode=mode,
            payload=self._sanitize_value(payload),
            summary=summary.strip(),
        )

        remaining = [item for item in state.presets if item.preset_id != normalized_id]
        state.presets = [preset, *remaining]
        self.save(state)
        return preset

    def remove_preset(self, state: ApiDebugWorkspaceState, preset_id: str) -> bool:
        """删除预设并写回。"""
        remaining = [item for item in state.presets if item.preset_id != preset_id]
        if len(remaining) == len(state.presets):
            return False
        state.presets = remaining
        self.save(state)
        return True

    def _read_payload(self) -> dict[str, Any]:
        try:
            return json.loads(self.storage_path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return {"version": 1}
        except (json.JSONDecodeError, OSError):
            return {"version": 1}

    def _write_payload(self, payload: dict[str, Any]) -> None:
        self.storage_path.parent.mkdir(parents=True, exist_ok=True)
        last_error: OSError | None = None

        with self._write_lock:
            for attempt in range(6):
                temp_path = self.storage_path.with_name(f"{self.storage_path.name}.{uuid.uuid4().hex}.tmp")
                try:
                    with open(temp_path, "w", encoding="utf-8") as file:
                        json.dump(payload, file, ensure_ascii=False, indent=2)
                        file.flush()
                        os.fsync(file.fileno())

                    os.replace(temp_path, self.storage_path)
                    return
                except OSError as error:
                    last_error = error
                    logger.warning(
                        f"写入接口调试工作台状态失败，准备重试: {type(error).__name__}: {error}",
                        LogType.FILE_FUNC,
                        LogSource.CORE,
                    )
                    time.sleep(0.02 * (attempt + 1))
                finally:
                    try:
                        if temp_path.exists():
                            temp_path.unlink()
                    except OSError:
                        pass

        if last_error is not None:
            raise last_error

    @staticmethod
    def _parse_mode(value: Any) -> ApiDebugMode:
        try:
            return ApiDebugMode(str(value))
        except ValueError:
            return ApiDebugMode.HTTP

    @staticmethod
    def _parse_target_type(value: Any, default: ApiDebugTargetType) -> ApiDebugTargetType:
        try:
            return ApiDebugTargetType(str(value))
        except ValueError:
            return default

    @staticmethod
    def _parse_body_type(value: Any) -> ApiDebugBodyType:
        try:
            return ApiDebugBodyType(str(value))
        except ValueError:
            return ApiDebugBodyType.NONE

    @staticmethod
    def _parse_auth(payload: Any) -> ApiDebugAuthConfig:
        if not isinstance(payload, dict):
            return ApiDebugAuthConfig()

        raw_type = str(payload.get("auth_type", ApiDebugAuthType.NONE.value))
        try:
            auth_type = ApiDebugAuthType(raw_type)
        except ValueError:
            auth_type = ApiDebugAuthType.NONE

        return ApiDebugAuthConfig(
            auth_type=auth_type,
            token=str(payload.get("token", "")),
            session_credential=str(payload.get("session_credential", "")),
            use_query_token=bool(payload.get("use_query_token", False)),
        )

    @staticmethod
    def _auth_to_dict(auth: ApiDebugAuthConfig) -> dict[str, Any]:
        return {
            "auth_type": auth.auth_type.value,
            "token": "<redacted>" if auth.token else "",
            "session_credential": "<redacted>" if auth.session_credential else "",
            "use_query_token": auth.use_query_token,
        }

    @staticmethod
    def _parse_sizes(payload: Any) -> dict[str, list[int]]:
        if not isinstance(payload, dict):
            return {}
        parsed: dict[str, list[int]] = {}
        for key, value in payload.items():
            if not isinstance(value, list):
                continue
            parsed[str(key)] = [int(item) for item in value if isinstance(item, (int, float))]
        return parsed

    def _parse_http_draft(self, payload: Any) -> ApiDebugHttpDraft:
        if not isinstance(payload, dict):
            return ApiDebugHttpDraft()
        return ApiDebugHttpDraft(
            url=str(payload.get("url", "")),
            method=str(payload.get("method", "GET")),
            headers=self._string_mapping(payload.get("headers")),
            query_params=self._string_mapping(payload.get("query_params")),
            body_type=self._parse_body_type(payload.get("body_type")),
            body=payload.get("body"),
            auth=self._parse_auth(payload.get("auth")),
            endpoint_id=str(payload.get("endpoint_id", "")),
            target_type=self._parse_target_type(payload.get("target_type"), ApiDebugTargetType.MANUAL_HTTP),
        )

    def _parse_action_draft(self, payload: Any) -> ApiDebugActionDraft:
        if not isinstance(payload, dict):
            return ApiDebugActionDraft()
        return ApiDebugActionDraft(
            action=str(payload.get("action", "")),
            search_query=str(payload.get("search_query", "")),
            params_text=str(payload.get("params_text", "{}")),
            endpoint_id=str(payload.get("endpoint_id", "")),
        )

    def _parse_websocket_draft(self, payload: Any) -> ApiDebugWebSocketDraft:
        if not isinstance(payload, dict):
            return ApiDebugWebSocketDraft()
        return ApiDebugWebSocketDraft(
            url=str(payload.get("url", "")),
            message_text=str(payload.get("message_text", "{}")),
            auth=self._parse_auth(payload.get("auth")),
            endpoint_id=str(payload.get("endpoint_id", "")),
            target_type=self._parse_target_type(payload.get("target_type"), ApiDebugTargetType.ONEBOT_WEBSOCKET),
            auto_scroll=bool(payload.get("auto_scroll", True)),
        )

    def _parse_presets(self, payload: Any) -> list[ApiDebugPreset]:
        if not isinstance(payload, list):
            return []
        presets: list[ApiDebugPreset] = []
        for item in payload:
            if not isinstance(item, dict):
                continue
            try:
                mode = ApiDebugMode(str(item.get("mode", ApiDebugMode.HTTP.value)))
            except ValueError:
                mode = ApiDebugMode.HTTP
            presets.append(
                ApiDebugPreset(
                    preset_id=str(item.get("preset_id", "")),
                    name=str(item.get("name", "")),
                    mode=mode,
                    payload=item.get("payload", {}) if isinstance(item.get("payload"), dict) else {},
                    summary=str(item.get("summary", "")),
                    created_at=str(item.get("created_at", "")),
                )
            )
        return presets

    def _http_draft_to_dict(self, draft: ApiDebugHttpDraft) -> dict[str, Any]:
        return {
            "url": self._sanitize_url(draft.url),
            "method": draft.method,
            "headers": self._sanitize_value(draft.headers),
            "query_params": self._sanitize_value(draft.query_params),
            "body_type": draft.body_type.value,
            "body": self._sanitize_value(draft.body),
            "auth": self._auth_to_dict(draft.auth),
            "endpoint_id": draft.endpoint_id,
            "target_type": draft.target_type.value,
        }

    def _action_draft_to_dict(self, draft: ApiDebugActionDraft) -> dict[str, Any]:
        return {
            "action": draft.action,
            "search_query": draft.search_query,
            "params_text": draft.params_text,
            "endpoint_id": draft.endpoint_id,
        }

    def _websocket_draft_to_dict(self, draft: ApiDebugWebSocketDraft) -> dict[str, Any]:
        return {
            "url": self._sanitize_url(draft.url),
            "message_text": draft.message_text,
            "auth": self._auth_to_dict(draft.auth),
            "endpoint_id": draft.endpoint_id,
            "target_type": draft.target_type.value,
            "auto_scroll": draft.auto_scroll,
        }

    def _preset_to_dict(self, preset: ApiDebugPreset) -> dict[str, Any]:
        return {
            "preset_id": preset.preset_id,
            "name": preset.name,
            "mode": preset.mode.value,
            "payload": self._sanitize_value(preset.payload),
            "summary": preset.summary,
            "created_at": preset.created_at,
        }

    @staticmethod
    def _string_mapping(payload: Any) -> dict[str, str]:
        if not isinstance(payload, dict):
            return {}
        return {str(key): "" if value is None else str(value) for key, value in payload.items()}

    def _sanitize_value(self, value: Any, *, key_name: str = "") -> Any:
        normalized_key = key_name.strip().lower()
        if normalized_key in _SENSITIVE_FIELDS:
            return "<redacted>"

        if isinstance(value, dict):
            return {str(key): self._sanitize_value(item, key_name=str(key)) for key, item in value.items()}
        if isinstance(value, list):
            return [self._sanitize_value(item) for item in value]
        if isinstance(value, bytes):
            return f"<binary:{len(value)} bytes>"
        return value

    def _sanitize_url(self, url: str) -> str:
        try:
            parts = urlsplit(url)
        except ValueError:
            return url

        if not parts.query:
            return url

        query_items = []
        for key, value in parse_qsl(parts.query, keep_blank_values=True):
            query_items.append((key, "<redacted>" if key.strip().lower() in _SENSITIVE_FIELDS else value))

        return urlunsplit((parts.scheme, parts.netloc, parts.path, urlencode(query_items), parts.fragment))
