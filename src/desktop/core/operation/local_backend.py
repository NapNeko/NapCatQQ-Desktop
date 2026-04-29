# -*- coding: utf-8 -*-
"""[`LocalBackend`](src/desktop/core/operation/local_backend.py): 本地 Windows 桌面环境的 [`OperationBackend`](src/desktop/core/operation/backend.py) 实现。

P0 阶段实现范围:
- 文件类 8 个方法: 完整 [`pathlib.Path`](https://docs.python.org/3/library/pathlib.html) 实现
- 检测类: ``detect_qq_path`` / ``detect_napcat_version`` / ``detect_installation``

P2 阶段补全(目前抛 NotImplementedError):
- 进程类: 与 [`ManagerNapCatQQProcess`](src/desktop/core/runtime/napcat.py) 桥接
- 安装类(写入): 与 [`NapCatInstall`](src/desktop/core/installation/installers.py) 桥接
- 日志类: 与 [`ManagerNapCatQQLog`](src/desktop/core/runtime/napcat.py) 桥接
- WebUI: 与 [`ManagerNapCatQQLoginState`](src/desktop/core/runtime/napcat.py) 桥接

之所以延后, 是因为本地这几类逻辑都重度耦合 Qt 信号 / [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) / [`QProcess`](https://doc.qt.io/qt-6/qprocess.html),
与 OperationBackend 的同步语义存在阻抗失配, 需要在 P2 阶段统一设计 Qt 信号桥接层。
"""

from __future__ import annotations

import re
import shutil
from pathlib import Path
from typing import TYPE_CHECKING

from .backend import (
    FileEntry,
    InstallationInfo,
    OperationBackend,
    ProcessStatus,
    ProgressCallback,
    WebUIEndpoint,
)

if TYPE_CHECKING:
    from src.desktop.core.config.config_model import Config


_NAPCAT_VERSION_PATTERN = re.compile(r'const\s+version\s*=\s*"([^"]+)"')

_P2_DEFER_MESSAGE = (
    "LocalBackend 当前方法已在 OperationBackend 接口中定义, "
    "实现排期: P2 阶段(远端 Bot 运行闭环), 届时与 ManagerNapCatQQProcess 桥接落地"
)


class LocalBackend(OperationBackend):
    """本地操作后端。

    所有路径参数均按 Windows 风格的 [`Path`](https://docs.python.org/3/library/pathlib.html) 解析。
    """

    # ==================== 文件 ====================
    def read_file(self, path: str) -> str:
        return Path(path).read_text(encoding="utf-8")

    def write_file(self, path: str, content: str) -> None:
        target = Path(path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    def file_exists(self, path: str) -> bool:
        return Path(path).exists()

    def list_dir(self, path: str) -> list[FileEntry]:
        target = Path(path)
        if not target.exists():
            raise FileNotFoundError(f"目录不存在: {target}")
        if not target.is_dir():
            raise NotADirectoryError(f"路径不是目录: {target}")

        entries: list[FileEntry] = []
        for item in target.iterdir():
            try:
                size = item.stat().st_size if item.is_file() else 0
            except OSError:
                size = 0
            entries.append(FileEntry(name=item.name, is_dir=item.is_dir(), size=size))
        return entries

    def mkdir(self, path: str, *, parents: bool = True, exist_ok: bool = True) -> None:
        Path(path).mkdir(parents=parents, exist_ok=exist_ok)

    def remove(self, path: str, *, recursive: bool = False) -> None:
        target = Path(path)
        if not target.exists():
            return
        if target.is_dir():
            if not recursive:
                raise IsADirectoryError(f"目标为目录, 删除需要 recursive=True: {target}")
            shutil.rmtree(target)
            return
        target.unlink()

    def upload(self, local_path: str | Path, remote_path: str) -> None:
        """本地后端的"上传"等价于本地拷贝。"""
        source = Path(local_path)
        target = Path(remote_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)

    def download(self, remote_path: str, local_path: str | Path) -> None:
        """本地后端的"下载"等价于本地拷贝。"""
        source = Path(remote_path)
        target = Path(local_path)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)

    # ==================== 进程 ====================
    def start_napcat(self, qq_id: str, config: "Config") -> ProcessStatus:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def stop_napcat(self, qq_id: str) -> None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def get_memory_usage(self, qq_id: str) -> int | None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    # ==================== 安装 ====================
    def install_napcat(
        self,
        archive_path: str | Path | None = None,
        *,
        progress: ProgressCallback | None = None,
    ) -> None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def install_qq(self, *, progress: ProgressCallback | None = None) -> None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def detect_napcat_version(self) -> str | None:
        """通过 [`PathFunc.napcat_path`](src/desktop/core/runtime/paths.py) 下的 ``napcat.mjs`` 探测版本。"""
        napcat_dir = self._resolve_napcat_path()
        if napcat_dir is None:
            return None

        mjs_path = napcat_dir / "napcat.mjs"
        if not mjs_path.exists():
            return None

        try:
            content = mjs_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return None

        match = _NAPCAT_VERSION_PATTERN.search(content)
        if match is None:
            return None
        return match.group(1).strip() or None

    def detect_qq_path(self) -> str | None:
        """通过 [`PathFunc.get_qq_path`](src/desktop/core/runtime/paths.py) (Windows 注册表)探测 QQ 安装路径。"""
        from src.desktop.core.runtime.paths import PathFunc

        qq_path = PathFunc.get_qq_path()
        if qq_path is None:
            return None
        return str(qq_path)

    def detect_installation(self) -> InstallationInfo:
        return InstallationInfo(
            napcat_version=self.detect_napcat_version(),
            qq_version=self._detect_qq_version(),
            qq_install_path=self.detect_qq_path(),
        )

    # ==================== 日志 ====================
    def read_log(self, qq_id: str) -> str:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    def tail_log(self, qq_id: str, *, lines: int = 200) -> str:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    # ==================== WebUI ====================
    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:
        raise NotImplementedError(_P2_DEFER_MESSAGE)

    # ==================== 内部辅助 ====================
    @staticmethod
    def _resolve_napcat_path() -> Path | None:
        """通过 [`creart`](https://github.com/MeetWq/creart) 单例获取 NapCat 安装目录。

        若 [`PathFunc`](src/desktop/core/runtime/paths.py) 单例不可用(测试环境等), 静默返回 None。
        """
        try:
            from creart import it

            from src.desktop.core.runtime.paths import PathFunc
        except ImportError:
            return None

        try:
            return Path(it(PathFunc).napcat_path)
        except Exception:  # noqa: BLE001 - creart 在缺失初始化时抛通用异常
            return None

    def _detect_qq_version(self) -> str | None:
        """读取 ``QQ/resources/app/package.json`` 中的 version 字段。"""
        qq_path_str = self.detect_qq_path()
        if not qq_path_str:
            return None

        package_json = Path(qq_path_str) / "resources" / "app" / "package.json"
        if not package_json.exists():
            return None

        try:
            import json

            payload = json.loads(package_json.read_text(encoding="utf-8", errors="replace"))
        except (OSError, json.JSONDecodeError):
            return None

        version = payload.get("version") if isinstance(payload, dict) else None
        if not isinstance(version, str):
            return None
        return version.strip() or None
