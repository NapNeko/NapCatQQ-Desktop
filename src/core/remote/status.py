# -*- coding: utf-8 -*-
"""远端状态与日志读取骨架。"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
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

    def stop(self) -> None:
        """停止远端进程。"""
        self.backend.run(
            f'test -f "{self.paths.pid_file}" && kill $(cat "{self.paths.pid_file}") >/dev/null 2>&1 || true'
        )

    def restart(self, command: str) -> None:
        """重启远端进程。"""
        self.stop()
        self.start(command)

    def _infer_default_log_path(self) -> str:
        return PurePosixPath(self.paths.log_dir, "napcat.log").as_posix()

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
