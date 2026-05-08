# -*- coding: utf-8 -*-
"""[`OperationBackend`](src/core/operation/backend.py) 接口与共享数据模型. 

该接口对应 [`docs/general/remote_ssh_plan.md`](../../../../docs/general/remote_ssh_plan.md) §2.2 的方法表, 
覆盖 NapCatQQ Desktop 在本地或远端运行 Bot 所需的全部 I/O / 进程 / 安装 / 日志 / WebUI 能力. 

设计要点:
- 接口以 "qq_id" 标识 NapCat 实例, 同时覆盖单机多 Bot 与远端多 Bot 场景
- 路径全部使用字符串而非 [`pathlib.Path`](https://docs.python.org/3/library/pathlib.html), 保证跨 Windows / Linux 表达
- 所有方法均为同步语义, Qt 信号桥接在调用方完成, 避免接口与 GUI 框架耦合
- 进度回调 [`ProgressCallback`](src/core/operation/backend.py) 用于安装/部署等长耗时操作向 UI 反馈进度
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable

if TYPE_CHECKING:
    from src.core.config.config_model import Config


# ==================== 数据模型 ====================
@dataclass(slots=True)
class FileEntry:
    """目录条目, 由 [`OperationBackend.list_dir`](src/core/operation/backend.py) 返回. """

    name: str
    is_dir: bool
    size: int = 0


@dataclass(slots=True)
class ProcessStatus:
    """NapCat 进程状态, 由 [`OperationBackend.get_process_status`](src/core/operation/backend.py) 返回. 

    Attributes:
        qq_id: 进程对应的 QQ 号
        running: 是否在线
        pid: 进程 ID, 不在线时为 None
        started_at: Unix 时间戳, 不在线时为 None
        memory_rss_bytes: 常驻集大小(字节), 不可用时为 None
        extra: 后端特定的附加信息(本地: QProcess state; 远程: pgrep 原始输出等)
    """

    qq_id: str
    running: bool
    pid: int | None = None
    started_at: float | None = None
    memory_rss_bytes: int | None = None
    extra: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class WebUIEndpoint:
    """WebUI 接入端点. 

    本地后端: ``base_url=http://127.0.0.1:{port}``. 
    远端后端: 默认通过 SSH 隧道暴露为本地端口(参考 §5.1 方案 A),
    返回值仍为 ``http://127.0.0.1:{tunnel_local_port}``, 由 backend 内部维护隧道生命周期. 
    """

    base_url: str
    token: str | None = None


@dataclass(slots=True)
class InstallationInfo:
    """聚合的安装信息探测结果. """

    napcat_version: str | None = None
    qq_version: str | None = None
    qq_install_path: str | None = None


# (message, percent_0_to_100) 进度回调签名
ProgressCallback = Callable[[str, int], None]


# ==================== 抽象接口 ====================
class OperationBackend(ABC):
    """NapCat 操作后端抽象. 

    所有上层逻辑(UI / Bot 管理 / 进程管理 / 安装流程)应仅依赖该接口,
    通过 [`LocalBackend`](src/core/operation/local_backend.py)
    或 [`RemoteBackend`](src/core/operation/remote_backend.py)
    在本地或远端透明执行. 
    """

    # ---------- 生命周期 ----------
    def connect(self) -> None:
        """准备后端运行环境. 

        远端后端用于建立 SSH 会话; 本地后端默认 no-op. 
        """
        return None

    def close(self) -> None:
        """释放后端资源. 

        远端后端用于关闭 SSH/SFTP/隧道; 本地后端默认 no-op. 
        """
        return None

    @property
    def is_connected(self) -> bool:
        """后端当前是否就绪. 本地默认恒为 True; 远端基于 SSH 会话状态. """
        return True

    def __enter__(self) -> "OperationBackend":
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    # ---------- 文件 ----------
    @abstractmethod
    def read_file(self, path: str) -> str:
        """读取文本文件全部内容. """

    @abstractmethod
    def write_file(self, path: str, content: str) -> None:
        """写入文本文件并覆盖, 父目录不存在时自动创建. """

    @abstractmethod
    def file_exists(self, path: str) -> bool:
        """判断路径是否存在(文件或目录). """

    @abstractmethod
    def list_dir(self, path: str) -> list[FileEntry]:
        """列出目录条目, 不递归. """

    @abstractmethod
    def mkdir(self, path: str, *, parents: bool = True, exist_ok: bool = True) -> None:
        """创建目录. """

    @abstractmethod
    def remove(self, path: str, *, recursive: bool = False) -> None:
        """删除文件或目录; 删除目录时需显式 ``recursive=True``. """

    @abstractmethod
    def upload(self, local_path: str | Path, remote_path: str) -> None:
        """从 Desktop 本地上传文件到 backend 工作区. """

    @abstractmethod
    def download(self, remote_path: str, local_path: str | Path) -> None:
        """从 backend 工作区下载文件到 Desktop 本地. """

    # ---------- 字节级流式 IO (P4 W3 F6 持久数据迁移) ----------
    # 这些方法以 default 实现给出便于第三方 backend 渐进适配; 真实 backend
    # (Local / Remote) 必须覆盖, 否则会触发 NotImplementedError. 不标 abstractmethod
    # 是为了避免破坏现有派生 / mock 类的实例化路径.
    def file_size(self, path: str) -> int:
        """返回文件字节数; 不存在时抛 ``FileNotFoundError``."""
        raise NotImplementedError

    def read_bytes(self, path: str, *, offset: int = 0, length: int | None = None) -> bytes:
        """从指定 ``offset`` 读取 ``length`` 字节; ``length=None`` 时读到末尾.

        - ``offset`` 必须 >= 0
        - 文件长度小于 ``offset + length`` 时返回实际可读数据, 不抛错
        """
        raise NotImplementedError

    def append_bytes(self, path: str, data: bytes) -> None:
        """以 append 模式向 ``path`` 追加 ``data``; 文件不存在时创建, 父目录自动创建.

        本期 P4 F6 持久数据迁移仅靠 append + rename 实现 ``.partial`` 续传,
        因此不暴露通用 seek-write API 以减小后端实现成本.
        """
        raise NotImplementedError

    def rename(self, src: str, dst: str) -> None:
        """原子重命名; ``dst`` 已存在时由 backend 决定是覆盖还是抛错."""
        raise NotImplementedError

    def walk_files(self, root: str) -> list[tuple[str, int]]:
        """递归列出 ``root`` 下所有文件 (相对路径 + 字节数), 不含目录条目.

        Default 实现走 :meth:`list_dir` 递归; backend 可覆盖以走更高效的本地化遍历.
        """
        if not self.file_exists(root):
            return []
        results: list[tuple[str, int]] = []
        # BFS 避免深递归
        stack: list[tuple[str, str]] = [("", root)]
        while stack:
            rel_prefix, abs_dir = stack.pop()
            try:
                entries = self.list_dir(abs_dir)
            except (FileNotFoundError, OSError):
                continue
            for entry in entries:
                rel_name = f"{rel_prefix}{entry.name}" if not rel_prefix else f"{rel_prefix}/{entry.name}"
                abs_name = self._join_posix(abs_dir, entry.name)
                if entry.is_dir:
                    stack.append((rel_name + "/", abs_name))
                else:
                    results.append((rel_name, entry.size))
        return results

    @staticmethod
    def _join_posix(parent: str, name: str) -> str:
        """跨 backend 拼接路径; backend 已经统一使用 POSIX 风格字符串."""
        if not parent:
            return name
        if parent.endswith("/") or parent.endswith("\\"):
            return f"{parent}{name}"
        return f"{parent}/{name}"

    # ---------- 进程 ----------
    @abstractmethod
    def start_napcat(self, qq_id: str, config: "Config") -> ProcessStatus:
        """启动 NapCat 实例. 

        - LocalBackend: 通过 [`QProcess`](https://doc.qt.io/qt-6/qprocess.html) 启动入口可执行文件
        - RemoteBackend: 通过 SSH 执行远端启动脚本(参考 [`templates.build_linux_deploy_script`](src/core/remote/templates.py))
        """

    @abstractmethod
    def stop_napcat(self, qq_id: str) -> None:
        """停止指定 NapCat 实例; 已停止时应静默成功. """

    @abstractmethod
    def get_process_status(self, qq_id: str) -> ProcessStatus:
        """获取指定 NapCat 实例的运行状态. """

    @abstractmethod
    def get_memory_usage(self, qq_id: str) -> int | None:
        """获取指定实例的 RSS 内存占用(字节); 进程不在线时返回 None. """

    # ---------- 安装 ----------
    @abstractmethod
    def install_napcat(
        self,
        archive_path: str | Path | None = None,
        *,
        progress: ProgressCallback | None = None,
    ) -> None:
        """安装或更新 NapCat. 

        - LocalBackend: 解压本地 zip 到 NapCat 安装目录
        - RemoteBackend: 上传/下载安装包并在远端解压
        """

    @abstractmethod
    def install_qq(self, *, progress: ProgressCallback | None = None) -> None:
        """安装 QQ 客户端. """

    @abstractmethod
    def detect_napcat_version(self) -> str | None:
        """探测当前已安装的 NapCat 版本; 未安装返回 None. """

    @abstractmethod
    def detect_qq_path(self) -> str | None:
        """探测 QQ 安装路径; 未安装返回 None. """

    @abstractmethod
    def detect_installation(self) -> InstallationInfo:
        """聚合探测一次安装信息. 便于上层一次性获取版本与路径. """

    # ---------- 日志 ----------
    @abstractmethod
    def read_log(self, qq_id: str) -> str:
        """读取指定实例完整日志. """

    @abstractmethod
    def tail_log(self, qq_id: str, *, lines: int = 200) -> str:
        """读取指定实例日志尾部 ``lines`` 行. """

    # ---------- WebUI ----------
    @abstractmethod
    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:
        """获取 NapCat WebUI 接入端点; 未启动 / 未发现时返回 None. """
