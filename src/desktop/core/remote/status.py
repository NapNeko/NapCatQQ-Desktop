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
        """读取远端状态。

        适配标准 NapCat 安装器的进程检测方式：
        - 通过 pgrep 查找 qq --no-sandbox -q 进程
        - 从命令行提取 QQ 号
        - 优先使用 napcat.mjs 中的版本信息
        """
        # 首先尝试使用标准 NapCat 方式检测进程
        pgrep_result = self.backend.run(r"pgrep -f '.*/qq --no-sandbox -q [0-9]{4,}' || true")

        pid = None
        running = False
        qq_account = None

        if pgrep_result.ok and pgrep_result.stdout.strip():
            # 找到进程，提取 PID 和 QQ 号
            pids = pgrep_result.stdout.strip().split('\n')
            if pids:
                # 使用第一个 PID
                pid = int(pids[0].strip())
                # 验证进程是否真正存活
                process_check = self.backend.run(f"kill -0 {pid} >/dev/null 2>&1")
                running = process_check.ok

                # 从命令行提取 QQ 号
                if running:
                    cmdline_result = self.backend.run(f"ps -o cmd= -p {pid} 2>/dev/null || true")
                    if cmdline_result.ok:
                        import re
                        match = re.search(r'--no-sandbox\s+-q\s+([0-9]{4,})', cmdline_result.stdout)
                        if match:
                            qq_account = match.group(1)

        # 回退到 PID 文件方式（用于兼容我们的 launcher 脚本）
        if not running:
            pid_result = self.backend.run(f'test -f "{self.paths.pid_file}" && cat "{self.paths.pid_file}" || true')
            pid_text = pid_result.stdout.strip()
            if pid_text.isdigit():
                pid = int(pid_text)
                process_check = self.backend.run(f"kill -0 {pid} >/dev/null 2>&1")
                running = process_check.ok

        # 读取状态文件
        status_result = self.backend.run(f'test -f "{self.paths.status_file}" && cat "{self.paths.status_file}" || true')
        payload = self._parse_status_payload(status_result.stdout)

        # 尝试从 napcat.mjs 读取版本
        version = self._as_string(payload.get("version"))
        if not version:
            version_result = self.backend.run(
                f"grep 'const version = ' '{self.paths.napcat_dir}/napcat.mjs' 2>/dev/null | "
                r"sed -n 's/.*const version = \"\([^\"]*\)\".*/\1/p' || true"
            )
            if version_result.ok:
                version = version_result.stdout.strip() or None

        return RemoteNapCatStatus(
            running=running,
            pid=pid,
            qq=qq_account or self._as_string(payload.get("qq")),
            version=version,
            log_file=self._as_string(payload.get("log_file")),
            raw_payload=payload,
        )

    def tail_log(self, log_path: str | None = None, *, lines: int = 200) -> RemoteLogTail:
        """读取远端日志尾部。

        适配标准 NapCat 日志路径:
        - 默认查找 $HOME/Napcat/log/napcat_*.log
        - 优先读取当前运行实例的日志
        """
        safe_lines = max(1, lines)

        if log_path:
            target_path = log_path
        else:
            # 尝试找到当前运行实例的日志
            target_path = self._infer_running_log_path() or self._infer_default_log_path()

        result = self.backend.run(f'test -f "{target_path}" && tail -n {safe_lines} "{target_path}" || true')
        return RemoteLogTail(path=target_path, content=result.stdout, lines=safe_lines)

    def _infer_running_log_path(self) -> str | None:
        """尝试从运行中的进程推断日志路径。"""
        # 获取当前运行的 QQ 号
        result = self.backend.run(r"pgrep -f '.*/qq --no-sandbox -q [0-9]{4,}' || true")
        if not result.ok or not result.stdout.strip():
            return None

        pids = result.stdout.strip().split('\n')
        if not pids:
            return None

        # 获取第一个进程的命令行
        cmdline_result = self.backend.run(f"ps -o cmd= -p {pids[0].strip()} 2>/dev/null || true")
        if not cmdline_result.ok:
            return None

        # 提取 QQ 号
        import re
        match = re.search(r'--no-sandbox\s+-q\s+([0-9]{4,})', cmdline_result.stdout)
        if match:
            qq = match.group(1)
            return f"{self.paths.log_dir}/napcat_{qq}.log"

        return None

    def start(self, command: str) -> None:
        """启动远端进程。

        这里保留命令注入位，后续会由部署层/配置层生成标准启动命令。
        """
        self.backend.run(command, check=True)
        status = self.get_status()
        if not status.running or status.pid is None:
            raise RuntimeError("远端启动命令已执行，但未检测到运行中的 NapCat 进程")

    def stop(self, command: str | None = None) -> None:
        """停止远端进程。"""
        if command:
            self.backend.run(command, check=True)
        else:
            self.backend.run(
                f'test -f "{self.paths.pid_file}" && kill $(cat "{self.paths.pid_file}") >/dev/null 2>&1 || true',
                check=False,
            )
        status = self.get_status()
        if status.running:
            raise RuntimeError("远端停止命令已执行，但 NapCat 进程仍在运行")

    def restart(self, command: str, stop_command: str | None = None) -> None:
        """重启远端进程。"""
        self.stop(stop_command)
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
