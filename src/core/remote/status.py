# -*- coding: utf-8 -*-
"""远端状态与日志读取骨架。"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import PurePosixPath
from typing import Any

from .execution_backend import ExecutionBackend
from .models import LinuxCorePaths


@dataclass(slots=True)
class RemoteNapCatStatus:
    """远端 NapCat 运行状态。"""

    running: bool
    pid: int | None = None
    qq: str | None = None
    version: str | None = None
    log_file: str | None = None
    raw_payload: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class RemoteLogTail:
    """远端日志尾部内容。"""

    path: str
    content: str
    lines: int


class RemoteRuntimeService:
    """远端运行时服务。

    当前阶段提供最小能力：
    - 读取 PID 文件判断运行态
    - 读取状态文件补充展示信息
    - tail 远端日志
    - 提供启停命令骨架
    """

    def __init__(self, backend: ExecutionBackend, paths: LinuxCorePaths | None = None) -> None:
        self.backend = backend
        self.paths = paths or LinuxCorePaths()

    def get_status(self) -> RemoteNapCatStatus:
        """读取远端状态。"""
        pid_result = self.backend.run(f'test -f "{self.paths.pid_file}" && cat "{self.paths.pid_file}" || true')
        status_result = self.backend.run(f'test -f "{self.paths.status_file}" && cat "{self.paths.status_file}" || true')

        pid_text = pid_result.stdout.strip()
        pid = int(pid_text) if pid_text.isdigit() else None
        running = False
        if pid is not None:
            process_check = self.backend.run(f"kill -0 {pid} >/dev/null 2>&1")
            running = process_check.ok

        payload = self._parse_status_payload(status_result.stdout)
        return RemoteNapCatStatus(
            running=running,
            pid=pid,
            qq=self._as_string(payload.get("qq")),
            version=self._as_string(payload.get("version")),
            log_file=self._as_string(payload.get("log_file")),
            raw_payload=payload,
        )

    def tail_log(self, log_path: str | None = None, *, lines: int = 200) -> RemoteLogTail:
        """读取远端日志尾部。"""
        target_path = log_path or self._infer_default_log_path()
        safe_lines = max(1, lines)
        result = self.backend.run(f'test -f "{target_path}" && tail -n {safe_lines} "{target_path}" || true')
        return RemoteLogTail(path=target_path, content=result.stdout, lines=safe_lines)

    def start(self, command: str) -> None:
        """启动远端进程。

        这里保留命令注入位，后续会由部署层/配置层生成标准启动命令。
        """
        self.backend.run(command, check=True)
        self.write_status_payload(
            {
                "running": True,
                "updated_at": self._current_timestamp(),
                "last_action": "start",
                "log_file": self._infer_default_log_path(),
            }
        )

    def stop(self) -> None:
        """停止远端进程。"""
        self.backend.run(
            f'test -f "{self.paths.pid_file}" && kill $(cat "{self.paths.pid_file}") >/dev/null 2>&1 || true'
        )
        self.write_status_payload(
            {
                "running": False,
                "pid": None,
                "updated_at": self._current_timestamp(),
                "last_action": "stop",
                "log_file": self._infer_default_log_path(),
            }
        )

    def restart(self, command: str) -> None:
        """重启远端进程。"""
        self.stop()
        self.start(command)

    def write_status_payload(self, payload: dict[str, Any]) -> None:
        """写入远端状态文件。

        这是 P1 阶段的最小状态协议落点，后续可以继续扩展字段，
        但必须保持 JSON 对象结构稳定。
        """
        serialized = json.dumps(payload, ensure_ascii=False, indent=2)
        escaped = serialized.replace("'", "'\"'\"'")
        self.backend.run(f"cat <<'EOF' > \"{self.paths.status_file}\"\n{escaped}\nEOF", check=True)

    @staticmethod
    def build_status_payload(
        *,
        running: bool,
        pid: int | None = None,
        qq: str | None = None,
        version: str | None = None,
        log_file: str | None = None,
        last_action: str | None = None,
        last_error: str | None = None,
    ) -> dict[str, Any]:
        """构建标准 `status.json` 结构。"""
        return {
            "running": running,
            "pid": pid,
            "qq": qq,
            "version": version,
            "log_file": log_file,
            "last_action": last_action,
            "last_error": last_error,
            "updated_at": RemoteRuntimeService._current_timestamp(),
        }

    def _infer_default_log_path(self) -> str:
        return PurePosixPath(self.paths.log_dir, "napcat.log").as_posix()

    @staticmethod
    def _current_timestamp() -> str:
        return datetime.now().astimezone().isoformat(timespec="seconds")

    @staticmethod
    def _parse_status_payload(raw_text: str) -> dict[str, Any]:
        if not raw_text.strip():
            return {}
        try:
            payload = json.loads(raw_text)
        except json.JSONDecodeError:
            return {"raw": raw_text.strip()}
        return payload if isinstance(payload, dict) else {"raw": payload}

    @staticmethod
    def _as_string(value: object) -> str | None:
        if value is None:
            return None
        text = str(value).strip()
        return text or None
