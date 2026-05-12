# -*- coding: utf-8 -*-
"""[`ServerManager`](src/core/remote/server_manager.py): 多服务器管理服务. 

职责:
- 包装 [`ServerRegistry`](src/core/remote/servers.py) 的增删改查
- 内存中缓存活跃服务器的 SSH 密码(不持久化, 参考 §6.2)
- 惰性创建并复用 [`RemoteBackend`](src/core/operation/remote_backend.py) 实例
- Qt 信号通知 UI(server_added / server_updated / server_removed)
- 同步 ``test_connection`` 接口供后台线程调用; UI 应通过 [`QRunnable`](https://doc.qt.io/qt-6/qrunnable.html) 调度
- 启动时一次性迁移旧版单服务器配置 (``cfg.remote_*``) 到新版 ``servers.json``

存储路径默认: ``{PathFunc.config_dir_path}/servers.json``. 
"""

from __future__ import annotations

import threading
from abc import ABC
from collections.abc import Callable
from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QObject, Signal

from src.core.logging import LogSource, LogType, logger

from .credential_store import CredentialStore
from .errors import (
    RemoteDeploymentCancelledError,
    RemoteDeploymentError,
    RemoteDeploymentInProgressError,
    RemoteError,
)
from .execution_backend import RemoteExecutionBackend
from .models import SSHCredentials
from .servers import BackendFlavor, DeploymentState, ServerProfile, ServerRegistry
from .ssh_client import SSHClient

if TYPE_CHECKING:
    from src.core.operation.backend import OperationBackend
    from src.core.operation.remote_backend import RemoteBackend
    from src.core.operation.remote_snowluma_backend import RemoteSnowLumaBackend


# 进度回调签名: (message, percent_0_to_100)
DeploymentProgressCallback = Callable[[str, int], None]


@dataclass(slots=True)
class DeploymentResult:
    """[`ServerManager.deploy_server`](src/core/remote/server_manager.py) 的返回值. """

    server_id: str
    ok: bool
    message: str
    napcat_version: str | None = None
    qq_version: str | None = None


def _default_storage_path() -> Path:
    """返回默认 ``servers.json`` 持久化路径. 

    取自 [`PathFunc.config_dir_path`](src/core/runtime/paths.py),
    若 creart 单例不可用(例如纯单元测试场景), 回退到当前工作目录. 
    """
    try:
        from creart import it

        from src.core.runtime.paths import PathFunc

        return Path(it(PathFunc).config_dir_path) / "servers.json"
    except Exception:  # noqa: BLE001 - 测试或最早期初始化时允许回退
        return Path.cwd() / "servers.json"


class ServerManager(QObject):
    """多服务器管理服务. 

    Signals:
        server_added (str): 新增服务器, 参数为 ``server_id``
        server_updated (str): 服务器档案被更新
        server_removed (str): 服务器被删除
        server_state_changed (str, str): 部署状态变化, ``(server_id, new_state_value)``
    """

    server_added = Signal(str)
    server_updated = Signal(str)
    server_removed = Signal(str)
    server_state_changed = Signal(str, str)
    # P1: 部署阶段进度 / 完结信号
    deployment_progress = Signal(str, str, int)  # (server_id, message, percent)
    deployment_finished = Signal(str, bool, str)  # (server_id, ok, message)
    # P1.5: 部署期间每行远端 stdout(含合并的 stderr) 实时回显
    deployment_log = Signal(str, str)  # (server_id, line)
    # 部署期间 ``\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条等) 实时回显;
    # 与 ``deployment_log`` 互斥, UI 端用于"原地覆盖上一行"以模拟终端行为,
    # 避免 dnf 进度条在控制台堆出上千条重复行 (典型现象: 同一个包的 ``Installing`` 行
    # 因 [bar] 不断重画被反复推入). 
    deployment_log_progress = Signal(str, str)  # (server_id, line)

    def __init__(
        self,
        storage_path: str | Path | None = None,
        parent: QObject | None = None,
        *,
        credential_store: CredentialStore | None = None,
    ) -> None:
        super().__init__(parent)
        path = Path(storage_path) if storage_path is not None else _default_storage_path()
        had_storage_file = path.exists()
        self._registry = ServerRegistry(path)
        # 服务器 ID -> 内存中的 SSH 密码(不落盘)
        self._password_cache: dict[str, str] = {}
        # 服务器 ID -> OperationBackend 实例(惰性创建并缓存);
        # 实际类型按 profile.backend_flavor 决定: RemoteBackend (NC) 或 RemoteSnowLumaBackend (SL).
        self._backend_cache: dict[str, "OperationBackend"] = {}
        # P1: 服务器 ID -> 是否正在部署(防并发)
        self._deploying: set[str] = set()
        # 部署取消标志: 服务器 ID -> threading.Event; ``set()`` 即"用户已请求取消".
        # 后台线程在多个埋点 (preflight 后 / install_qq 前后 / install_napcat 内的源迭代等)
        # 检查此 Event, 命中则抛 :class:`RemoteDeploymentCancelledError` 提前退出.
        # 与 ``_deploying`` 同步生命周期: ``deploy_server`` 入口创建, ``finally`` 清理.
        self._cancel_events: dict[str, threading.Event] = {}
        # P4 F5.2: keyring 包装; 缺省构造一个; 测试可注入自己的实例
        self._credential_store: CredentialStore = credential_store if credential_store is not None else CredentialStore()

        # 仅在首次启动(磁盘上无 servers.json)时尝试迁移旧版单服务器配置
        if not had_storage_file:
            self._migrate_legacy_single_server_config()

        # P4 F5.2: 启动时把 keyring 已记的密码预加载到内存缓存,
        # 让 ``test_connection`` / ``get_backend`` 不必每次都问 keyring (减少 IO).
        # keyring 不可用时 (非 Windows / 用户禁用 Credential Manager) 静默跳过.
        self._preload_passwords_from_keyring()

    # ==================== 仓库直通 ====================
    @property
    def registry(self) -> ServerRegistry:
        return self._registry

    @property
    def storage_path(self) -> Path:
        return self._registry.storage_path

    def list_servers(self) -> list[ServerProfile]:
        return self._registry.list()

    def get_server(self, server_id: str) -> ServerProfile | None:
        return self._registry.get(server_id)

    # ==================== CRUD ====================
    def add_server(
        self,
        profile: ServerProfile,
        *,
        password: str | None = None,
        remember_password: bool = False,
    ) -> None:
        """添加新服务器档案; 若是密码认证, 调用方应同时传入 password 暂存到内存. 

        Args:
            profile: 新档案 (id 由调用方生成).
            password: 密码认证模式下的密码; 缺省 None.
            remember_password: P4 F5.2: 设为 True 时把密码写入 Windows Credential
                Manager (keyring), 重启 Desktop 后仍可用. 仅当 ``auth_method=="password"``
                + keyring 可用时实际生效, 其他情况静默忽略.
        """
        self._registry.add(profile)
        if password and profile.credentials.auth_method == "password":
            self._password_cache[profile.id] = password
            if remember_password:
                self._persist_password_to_keyring(profile.id, password)
        logger.info(
            f"已新增服务器档案: id={profile.id}, name={profile.name}, host={profile.credentials.host}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        # P4 W2 F3: 新增服务器仅发信号; 资源监控 attach 由
        # ResourceMonitorService 订阅 server_added 后自行决定, 避免从 ServerManager
        # 依赖 creart 单例造成 W1 期间还不需要采样的场景也被迫启动 worker.
        self.server_added.emit(profile.id)

    def update_server(
        self,
        profile: ServerProfile,
        *,
        password: str | None = None,
        remember_password: bool | None = None,
    ) -> None:
        """覆盖现有档案, 同时失效 backend 缓存; 密码可选更新.

        Args:
            password: 新密码; 传 None 表示不修改, 传 ``""`` 视为清除.
            remember_password: P4 F5.2: 三态:
                - ``True``: 写入 keyring (要求 password 非空 + 密码模式)
                - ``False``: 显式从 keyring 删除
                - ``None``: 不动 keyring, 与 P3 行为一致
        """
        self._registry.update(profile)
        self._backend_cache.pop(profile.id, None)
        if password is not None:
            if password and profile.credentials.auth_method == "password":
                self._password_cache[profile.id] = password
            else:
                self._password_cache.pop(profile.id, None)

        if remember_password is True and password and profile.credentials.auth_method == "password":
            self._persist_password_to_keyring(profile.id, password)
        elif remember_password is False:
            self._delete_password_from_keyring(profile.id)
        logger.info(
            f"已更新服务器档案: id={profile.id}, name={profile.name}, host={profile.credentials.host}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.server_updated.emit(profile.id)

    def remove_server(self, server_id: str) -> bool:
        """删除档案与所有关联缓存 (含 keyring 中的密码)."""
        ok = self._registry.remove(server_id)
        if ok:
            self._password_cache.pop(server_id, None)
            backend = self._backend_cache.pop(server_id, None)
            if backend is not None:
                try:
                    backend.close()
                except Exception:  # noqa: BLE001
                    pass
            # P4 F5.2: 同步清理 keyring, 避免遗留条目
            self._delete_password_from_keyring(server_id)
            # P4 W2 F3: detach 同样交由 ResourceMonitorService 订阅 server_removed 处理
            logger.info(f"已删除服务器档案: id={server_id}", LogType.NETWORK, LogSource.CORE)
            self.server_removed.emit(server_id)
        return ok

    # ==================== keyring 集成 (P4 F5.2) ====================
    def credential_store(self) -> CredentialStore:
        """返回内部 ``CredentialStore`` (主要供 ServerEditDialog 探测可用性)."""
        return self._credential_store

    def has_remembered_password(self, server_id: str) -> bool:
        """server_id 在 keyring 中是否有持久化的密码."""
        if not self._credential_store.is_available():
            return False
        return self._credential_store.load_password(server_id) is not None

    def _preload_passwords_from_keyring(self) -> None:
        """启动时把 keyring 中所有已知服务器的密码预加载到内存缓存."""
        if not self._credential_store.is_available():
            return
        for profile in self._registry.list():
            if profile.credentials.auth_method != "password":
                continue
            stored = self._credential_store.load_password(profile.id)
            if stored:
                self._password_cache[profile.id] = stored
                logger.trace(
                    f"已从 keyring 预加载密码: id={profile.id}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )

    def _persist_password_to_keyring(self, server_id: str, password: str) -> bool:
        """把密码写入 keyring; 失败时记录但不抛."""
        if not self._credential_store.is_available():
            return False
        ok = self._credential_store.store_password(server_id, password)
        if ok:
            logger.info(
                f"已把密码持久化到 keyring: id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        else:
            logger.warning(
                f"keyring 写入失败 (降级为仅内存缓存): id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        return ok

    def _delete_password_from_keyring(self, server_id: str) -> None:
        """从 keyring 删除密码; 不存在条目走 idempotent 成功路径."""
        if not self._credential_store.is_available():
            return
        self._credential_store.delete_password(server_id)

    # ==================== NapCat 完整性校验 (P5 F1.4) ====================
    def _lookup_napcat_expected_sha512(self) -> str | None:
        """同步获取远端 latest NapCat 的期望 SHA512.

        流程: 拉取上游 ``release.json`` (带缓存) -> 拉取 NapCat 远端 latest 版本号 ->
        在 hash 服务里 lookup. 任一步失败均返回 ``None``, 调用方会以"跳过校验"
        模式调用远端脚本 (脚本端会 ``log_warn``).

        在 worker 线程同步执行, 总耗时上限 ≈ 5s + 5s + 解析时间.
        """
        try:
            from src.core.versioning.release_hash_service import ReleaseHashService

            service = ReleaseHashService()
            service.fetch()
            version = self._fetch_napcat_remote_version()
            if not version:
                logger.warning(
                    "NapCat 完整性校验跳过: 拉取远端 latest 版本号失败",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return None
            entry = service.lookup(version)
            if entry is None:
                logger.warning(
                    f"NapCat 完整性校验跳过: 上游 release.json 中没有版本 {version}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return None
            logger.info(
                f"NapCat 完整性校验已启用: version={entry.version}, sha512_prefix={entry.shell_sha512[:16]}...",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return entry.shell_sha512
        except Exception as exc:  # noqa: BLE001 - 任何异常都退化为"跳过校验", 不阻塞部署
            logger.warning(
                f"NapCat 完整性校验跳过 (异常): {type(exc).__name__}: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return None

    @staticmethod
    def _fetch_napcat_remote_version() -> str | None:
        """同步获取 NapCat 远端 latest 版本号 (形如 ``v4.18.1``); 失败返回 None."""
        import httpx

        from src.core.network.urls import Urls

        candidates = (
            Urls.NAPCATQQ_REPO_API.value.toString(),
            Urls.NAPCATQQ_REPO_API_FALLBACK.value.toString(),
        )
        for url in candidates:
            try:
                with httpx.Client(timeout=5.0, follow_redirects=True) as client:
                    response = client.get(url)
                    response.raise_for_status()
                    payload = response.json()
            except Exception:  # noqa: BLE001 - 任意网络错误都视为该源失败
                continue
            tag = payload.get("tag_name") if isinstance(payload, dict) else None
            if isinstance(tag, str) and tag.strip():
                return tag.strip()
        return None

    # ==================== 资源监控钩子 (P4 W2 F3) ====================
    # 设计沉淀: ServerManager 不主动 attach/detach
    # [`ResourceMonitorService`](src/core/remote/resource_monitor.py); 该服务订阅
    # ``server_added`` / ``server_removed`` 后自行决定是否启动采样 worker,
    # 这样 W1 / 单元测试环境下只要不出现 ``it(ResourceMonitorService)``
    # 调用, 就不会启动背后轮询.

    def set_deployment_state(self, server_id: str, state: DeploymentState) -> None:
        """更新指定服务器的部署状态并持久化. """
        profile = self._registry.get(server_id)
        if profile is None:
            return
        profile.deployment_state = state
        self._registry.update(profile)
        self.server_state_changed.emit(server_id, state.value)

    # ==================== 凭据辅助 ====================
    def set_password(self, server_id: str, password: str | None) -> None:
        """设置或清除内存中的密码. """
        if password:
            self._password_cache[server_id] = password
        else:
            self._password_cache.pop(server_id, None)

    def has_password(self, server_id: str) -> bool:
        return server_id in self._password_cache

    def get_runtime_credentials(self, server_id: str) -> SSHCredentials | None:
        """返回注入了内存密码的运行期凭据副本; 服务器不存在时返回 None. """
        profile = self._registry.get(server_id)
        if profile is None:
            return None
        return self._inject_runtime_password(profile)

    # ==================== Backend 工厂 ====================
    def get_backend(self, server_id: str) -> "OperationBackend":
        """获取或惰性创建远端 backend 实例; 服务器不存在时抛 KeyError.

        W10b-Driver: 按 ``profile.backend_flavor`` 分发:

        - :attr:`BackendFlavor.NAPCAT` → :class:`RemoteBackend` (NC launcher 路径)
        - :attr:`BackendFlavor.SNOWLUMA` → :class:`RemoteSnowLumaBackend`
          (SL launcher + daemon 路径)

        两种 backend 都满足 :class:`OperationBackend` 协议, 上层
        (:class:`BotProcessManager` / :func:`resolve_backend_for_bot`) 不需要按
        flavor 做特殊处理, 协议方法语义自动对齐.
        """
        cached = self._backend_cache.get(server_id)
        if cached is not None:
            return cached

        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        cred = self._inject_runtime_password(profile)

        # W10b-Driver: SL flavor 走独立 backend 实现
        if profile.backend_flavor == BackendFlavor.SNOWLUMA:
            from src.core.operation.remote_snowluma_backend import RemoteSnowLumaBackend

            sl_paths = profile.snowluma_paths
            if sl_paths is None:
                # SL flavor 但 snowluma_paths 缺失 (旧 servers.json 异常态);
                # 走默认布局让 backend 仍可构造, 错误等部署时再暴露
                from src.core.remote.snowluma import SnowLumaRemotePaths

                sl_paths = SnowLumaRemotePaths.from_base()
                logger.warning(
                    f"服务器 {server_id} backend_flavor=SNOWLUMA 但 snowluma_paths 缺失, "
                    "已退化到默认布局",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
            # W10b-WebUI: 把 per-server WebUI 密码 override 注入 backend, 让
            # ``_ensure_remote_webui_password`` 优先级与本地 daemon 对齐.
            backend = RemoteSnowLumaBackend(
                cred,
                sl_paths,
                webui_password_override=profile.snowluma_webui_password_override,
            )
        else:
            # 延迟导入避免与 [`core.operation`](src/core/operation/__init__.py) 之间的循环
            from src.core.operation.remote_backend import RemoteBackend

            backend = RemoteBackend(cred, profile.paths)

        self._backend_cache[server_id] = backend
        return backend

    # ==================== 连接测试 ====================
    def test_connection(
        self,
        profile: ServerProfile,
        *,
        password: str | None = None,
    ) -> tuple[bool, str]:
        """**同步**测试 SSH 连接(应在后台线程调用). 

        Args:
            profile: 服务器档案; 不会修改其内部凭据
            password: 临时密码; 若 None 且为密码认证, 则尝试 ``password_cache``

        Returns:
            (成功标志, 用户可读消息)
        """
        cred = profile.credentials
        if cred.auth_method == "password":
            effective_password = password or self._password_cache.get(profile.id)
            if not effective_password:
                return False, "密码认证模式下必须提供密码"
            cred = replace(cred, password=effective_password)

        # P4 F5.4: 失败路径走 ``to_friendly`` 把 paramiko / SSH 异常转中文文案,
        # 避免 ``AuthenticationException`` 这类原始字眼直接出现在用户 InfoBar.
        from .friendly_errors import to_friendly

        client = SSHClient(cred)
        try:
            client.connect()
            try:
                result = client.run('echo "ok"')
            finally:
                client.close()
        except RemoteError as exc:
            return False, to_friendly(exc)
        except Exception as exc:  # noqa: BLE001 - 任何意外异常都要给用户友好反馈
            return False, to_friendly(exc)

        if not result.ok:
            return False, f"远端命令返回非零状态: {result.stderr or result.stdout}"
        return True, "SSH 连接测试成功"

    # ==================== 自动密钥下发 (ssh-copy-id 等价物) ====================
    def auto_setup_ssh_key(
        self,
        server_id: str,
        *,
        password: str | None = None,
    ) -> tuple[bool, str]:
        """**同步**用密码登录后把本地公钥下发到远端, 并把档案切到密钥认证(应在后台线程调用).

        语义对齐 ``ssh-copy-id``: 复用 ``~/.ssh/id_ed25519`` (不存在则生成),
        通过 [`SSHClient.install_authorized_key`](src/core/remote/ssh_client.py)
        幂等写入远端 ``authorized_keys``. 成功后:

        - 档案 ``credentials.auth_method`` 切到 ``"key"``, ``private_key_path`` 写入本地私钥
        - 档案 ``credentials.password`` 清空(配合 dataclass 不落盘内存密码)
        - ``_password_cache`` 中的内存密码移除
        - keyring 中的持久化密码删除

        Args:
            server_id: 服务器档案 ID(必须当前为密码认证模式)
            password: 临时密码; 若 None 则尝试 ``_password_cache``

        Returns:
            ``(成功标志, 用户可读消息)``; 失败时档案保持密码认证不变, UI 可让用户重试.
        """
        from .friendly_errors import to_friendly
        from .ssh_keys import ensure_local_keypair

        profile = self._registry.get(server_id)
        if profile is None:
            return False, "服务器档案不存在"
        if profile.credentials.auth_method != "password":
            return False, "仅密码认证模式支持自动配置 SSH 密钥"

        effective_password = password or self._password_cache.get(server_id)
        if not effective_password:
            return False, "缺少密码, 无法登录远端配置密钥"

        # 1) 准备本地密钥对; 任何文件系统/cryptography 异常都属可恢复, 不影响密码认证现状
        try:
            priv_path, pub_line = ensure_local_keypair()
        except Exception as exc:  # noqa: BLE001 - 任何异常都退回友好消息
            logger.warning(
                f"auto_setup_ssh_key 本地密钥准备失败: id={server_id}, exc={exc!r}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return False, f"本地密钥生成失败: {exc}"

        # 2) 用一次性带密码的凭据建立 SSH, 仅用于推送公钥; 不修改 profile 内的凭据对象
        cred = replace(profile.credentials, password=effective_password)
        client = SSHClient(cred)
        try:
            client.connect()
            try:
                client.install_authorized_key(pub_line)
            finally:
                client.close()
        except RemoteError as exc:
            return False, to_friendly(exc)
        except Exception as exc:  # noqa: BLE001 - 兜底, 走友好文案桥
            return False, to_friendly(exc)

        # 3) 切档案到密钥认证 + 清密码缓存 + 清 keyring
        new_cred = replace(
            profile.credentials,
            auth_method="key",
            password=None,
            private_key_path=str(priv_path),
        )
        # ServerProfile 是 ``@dataclass`` (无 slots), 这里直接 replace 出新对象写回 registry.
        new_profile = replace(profile, credentials=new_cred)
        self._registry.update(new_profile)
        self._backend_cache.pop(server_id, None)
        self._password_cache.pop(server_id, None)
        self._delete_password_from_keyring(server_id)
        self.server_updated.emit(server_id)

        logger.info(
            f"已自动配置 SSH 密钥并切换到密钥认证: id={server_id}, key={priv_path}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return True, "已配置免密登录, 服务器已切换为密钥认证"

    # ==================== 部署编排 (P1)  ====================
    def is_deploying(self, server_id: str) -> bool:
        """是否有正在进行中的部署任务. """
        return server_id in self._deploying

    def request_cancel(self, server_id: str) -> bool:
        """请求取消正在进行中的部署任务.

        协作式取消: set 一个 :class:`threading.Event`, 后台线程在多个埋点检查并主动抛
        :class:`RemoteDeploymentCancelledError` 退出. **不**强制 kill 线程, 因此:

        - 用户感知到的"立即停"取决于当前在哪一步. SSH 命令 / SFTP 上传等阻塞 IO 必须
          等当前调用返回 (受 ``script_timeout`` / ``connect_timeout`` 限制); 但本机
          httpx 下载循环 / 源迭代等会**几乎立即**响应
        - 重复调用幂等: Event 已 set 时直接返回 ``True``

        Args:
            server_id: 待取消的服务器档案 ID

        Returns:
            ``True``: 已成功标记取消 (任务在跑); ``False``: 没有正在跑的部署任务
        """
        event = self._cancel_events.get(server_id)
        if event is None:
            logger.info(
                f"request_cancel: 服务器无正在进行的部署任务, 忽略: id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return False
        if not event.is_set():
            logger.warning(
                f"request_cancel: 用户请求取消部署: id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            self.deployment_log.emit(
                server_id, "[WARN] 收到用户取消请求, 正在中止部署 (当前 SSH 命令完成后退出)..."
            )
            event.set()
        return True

    def is_cancel_requested(self, server_id: str) -> bool:
        """部署任务是否已被请求取消 (Event 是否 set). """
        event = self._cancel_events.get(server_id)
        return event is not None and event.is_set()

    def _check_cancelled(self, server_id: str) -> None:
        """埋点检查: 若用户已请求取消则抛 :class:`RemoteDeploymentCancelledError`.

        deploy_server 内部在每个 stage 切换 / 长 IO 之前调用; 让取消请求能尽快生效.
        """
        if self.is_cancel_requested(server_id):
            raise RemoteDeploymentCancelledError()

    def deploy_server(
        self,
        server_id: str,
        *,
        progress_callback: DeploymentProgressCallback | None = None,
        force_napcat_update: bool = False,
        force_linuxqq_reinstall: bool = False,
    ) -> DeploymentResult:
        """**同步**执行远端部署(应在后台线程调用). 

        根据 ``profile.backend_flavor`` 分支:

        - ``NAPCAT``: 编排 install_qq + install_napcat 两步, 进度 0-100 (现有路径)
        - ``SNOWLUMA``: 编排 install_linuxqq + install_snowluma_framework + 上传
          launcher 脚本 + verify (W8 新增路径); ``force_napcat_update`` 在 SL flavor
          下被忽略 (SL 不装 NapCat).

        Raises:
            KeyError: 服务器档案不存在
            RemoteDeploymentInProgressError: 已有部署任务在跑
            RemoteDeploymentError: 部署中途失败(stage 字段标记失败步骤)
        """
        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        if server_id in self._deploying:
            raise RemoteDeploymentInProgressError(
                f"服务器 {profile.name} 正在部署中, 请等待当前任务完成"
            )

        # W8: SnowLuma flavor 走独立 deploy 路径
        if profile.backend_flavor == BackendFlavor.SNOWLUMA:
            return self._deploy_snowluma_flavor(
                server_id,
                profile,
                progress_callback=progress_callback,
                force_linuxqq_reinstall=force_linuxqq_reinstall,
            )

        self._deploying.add(server_id)
        # 注册取消 Event (在加入 _deploying 之后立刻创建, 让 request_cancel 立刻可用)
        cancel_event = threading.Event()
        self._cancel_events[server_id] = cancel_event
        # 进入 DEPLOYING 状态(写盘 + 信号)
        self.set_deployment_state(server_id, DeploymentState.DEPLOYING)

        def _emit_progress(message: str, percent: int) -> None:
            self.deployment_progress.emit(server_id, message, percent)
            if progress_callback is not None:
                try:
                    progress_callback(message, percent)
                except Exception as exc:  # noqa: BLE001 - 回调失败不阻断
                    logger.warning(
                        f"deploy_server 外部 progress_callback 抛错: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )

        # P1.5: 把每行远端日志 emit 为 deployment_log 信号
        def _emit_log_line(line: str) -> None:
            self.deployment_log.emit(server_id, line)

        # ``\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条) 走单独的信号,
        # UI 端做"原地覆盖上一行"渲染, 避免控制台被进度条刷屏. 
        def _emit_log_progress(line: str) -> None:
            self.deployment_log_progress.emit(server_id, line)

        backend = None
        try:
            _emit_progress("准备 SSH 连接", 0)
            backend = self.get_backend(server_id)
            backend.connect()

            # ----- Stage 0: 兼容性体检 (preflight) -----
            # SSH 连通后先探测远端环境, 把发行版 / 架构 / 包管理器写到部署日志,
            # 不支持的组合立刻以 ``stage="preflight"`` 抛出, 避免后续 install_qq
            # 走到脚本里再报奇怪的退出码 (脚本侧的 30/31 错误对用户不友好).
            _emit_progress("远端环境体检", 1)
            self.deployment_log.emit(server_id, "[PREFLIGHT] 正在探测远端环境...")
            probe = backend.deployment.probe_environment()
            report = probe.evaluate_compatibility()
            family_text = report.family or "-"
            display_name = (
                report.distro_entry.display_name
                if report.distro_entry is not None
                else (probe.distro_id or "unknown")
            )
            self.deployment_log.emit(
                server_id,
                (
                    f"[PREFLIGHT] distro={display_name} "
                    f"version={probe.distro_version or '-'} "
                    f"arch={probe.normalized_arch or probe.architecture} "
                    f"family={family_text} "
                    f"installer={'dpkg' if probe.has_dpkg else ('rpm2cpio' if probe.has_rpm2cpio else 'none')} "
                    f"status={report.compat_status}"
                ),
            )
            for reason in report.reasons:
                self.deployment_log.emit(server_id, f"[PREFLIGHT] {reason}")

            if report.compat_status == "unsupported":
                detail = "; ".join(report.reasons) or "远端环境不满足部署条件"
                raise RemoteDeploymentError(
                    "preflight",
                    f"远端环境兼容性体检未通过: {detail}",
                )
            if report.compat_status == "unknown_but_runnable":
                self.deployment_log.emit(
                    server_id,
                    "[PREFLIGHT] 警告: 未识别的发行版, 但探测到可用的包管理器, "
                    "将以通用流程尝试部署 (失败概率较高)",
                )

            # 取消埋点 1: preflight 完成后, install_qq 之前
            self._check_cancelled(server_id)

            # ----- Stage 1: install_qq, 0-50 -----
            def _qq_progress(message: str, percent: int) -> None:
                # 0-100 -> 0-50
                _emit_progress(f"[LinuxQQ] {message}", int(percent / 2))

            try:
                backend.install_qq(
                    progress=_qq_progress,
                    log_callback=_emit_log_line,
                    progress_log_callback=_emit_log_progress,
                    force_reinstall=force_linuxqq_reinstall,
                )
            except Exception as exc:  # noqa: BLE001 - 统一封装为 RemoteDeploymentError
                # 用户在 install_qq 期间点了取消 -> 让取消异常透出, 不被包成 install_qq 失败
                if self.is_cancel_requested(server_id):
                    raise RemoteDeploymentCancelledError(cause=exc) from exc
                # 远端脚本退出码 37 -> LinuxQQ 包多次下载后仍未通过 dpkg-deb / rpm 完整性校验,
                # 单独 stage='install_qq_verify' 让用户能看到 "完整性校验失败" 而非通用安装失败
                stage_label = "install_qq"
                exit_status = getattr(exc, "exit_status", None)
                if exit_status == 37:
                    stage_label = "install_qq_verify"
                    raise RemoteDeploymentError(
                        stage_label,
                        "LinuxQQ 安装包完整性校验失败 (多次下载后仍损坏); "
                        "可能原因: 网络中途中断 / 代理截断 / 镜像源返回不完整内容. "
                        "建议检查远端服务器出方向网络, 或重试部署.",
                        cause=exc,
                    ) from exc
                raise RemoteDeploymentError(
                    stage_label,
                    f"LinuxQQ 安装失败: {exc}",
                    cause=exc,
                ) from exc

            # 取消埋点 2: install_qq 完成后, install_napcat 之前
            self._check_cancelled(server_id)

            # ----- Stage 2: install_napcat, 50-100 -----
            # P5 F1.4: 在调用 install_napcat 之前先查询上游 SHA512;
            # 取不到时静默跳过 (远端脚本端 fallback 为 warn skip), 不阻断部署
            # - 远端 worker 线程不便弹用户对话框, 这里走"网络不稳时允许继续"策略.
            expected_sha512 = self._lookup_napcat_expected_sha512()

            def _napcat_progress(message: str, percent: int) -> None:
                _emit_progress(f"[NapCat] {message}", 50 + int(percent / 2))

            # 本机下载兜底缓存路径: 复用 Desktop 自身管理 NapCat 资产的 tmp 目录,
            # 与 [`napcat_page._start_download`](src/ui/page/component_page/sub_page/napcat_page.py)
            # 共享同一份 archive (本机已经装过 NapCat 时可零下载直接 SFTP 上传).
            # PathFunc 不可用 (极早期 / 纯单测) 时退化为 None, 走旧的"远端自下载"路径.
            local_archive_cache: Path | None = None
            try:
                from creart import it as _it

                from src.core.runtime.paths import PathFunc as _PathFunc

                local_archive_cache = (
                    Path(_it(_PathFunc).tmp_path) / "NapCat.Shell.zip"
                )
            except Exception:  # noqa: BLE001 - 取不到 tmp 路径时静默关闭兜底
                local_archive_cache = None
            try:
                backend.install_napcat(
                    progress=_napcat_progress,
                    log_callback=_emit_log_line,
                    progress_log_callback=_emit_log_progress,
                    force_update=force_napcat_update,
                    expected_sha512=expected_sha512,
                    local_archive_cache=local_archive_cache,
                    should_cancel=cancel_event.is_set,
                )
            except RemoteDeploymentCancelledError:
                # prefetch 内部主动抛的取消异常, 直接透出, 不要包成 install_napcat 失败
                raise
            except Exception as exc:  # noqa: BLE001
                # 用户在 install_napcat 阻塞 IO 中点了取消 -> 透出取消异常
                if self.is_cancel_requested(server_id):
                    raise RemoteDeploymentCancelledError(cause=exc) from exc
                # P5 F1.4: 远端脚本退出码 36 -> SHA512 不匹配, 单独 stage 标签便于
                # friendly_errors 与 UI 提示走"完整性校验"专属文案
                stage_label = "install_napcat"
                stderr_text = getattr(exc, "stderr", "") or ""
                exit_status = getattr(exc, "exit_status", None)
                if exit_status == 36 or "sha512 mismatch" in stderr_text.lower():
                    stage_label = "install_napcat_verify"
                raise RemoteDeploymentError(
                    stage_label,
                    f"NapCat 安装失败: {exc}",
                    cause=exc,
                ) from exc

            # 取消埋点 3: install_napcat 完成后, 探测版本之前
            self._check_cancelled(server_id)

            # ----- 探测安装信息并写回档案 -----
            _emit_progress("探测远端版本", 98)
            installation = backend.detect_installation()
            updated_profile = self._registry.get(server_id)
            if updated_profile is not None:
                updated_profile.napcat_version = installation.napcat_version
                updated_profile.qq_version = installation.qq_version
                updated_profile.deployment_state = DeploymentState.DEPLOYED
                self._registry.update(updated_profile)
                self.server_state_changed.emit(server_id, DeploymentState.DEPLOYED.value)
                self.server_updated.emit(server_id)

            _emit_progress("部署完成", 100)
            success_message = (
                f"部署成功: NapCat={installation.napcat_version or '未探测到'}, "
                f"QQ={installation.qq_version or '未探测到'}"
            )
            self.deployment_finished.emit(server_id, True, success_message)
            return DeploymentResult(
                server_id=server_id,
                ok=True,
                message=success_message,
                napcat_version=installation.napcat_version,
                qq_version=installation.qq_version,
            )

        except RemoteDeploymentCancelledError as exc:
            # 用户主动取消 != 失败: 状态机回到 UNDEPLOYED, 让用户下次点部署是干净起点
            self.set_deployment_state(server_id, DeploymentState.UNDEPLOYED)
            self.deployment_log.emit(server_id, "[INFO] 部署已被用户取消")
            self.deployment_finished.emit(server_id, False, str(exc))
            logger.info(
                f"远端部署被用户取消: id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise
        except RemoteDeploymentError as exc:
            self.set_deployment_state(server_id, DeploymentState.FAILED)
            self.deployment_finished.emit(server_id, False, str(exc))
            logger.warning(
                f"远端部署失败: id={server_id}, stage={exc.stage}, msg={exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise
        except Exception as exc:  # noqa: BLE001
            self.set_deployment_state(server_id, DeploymentState.FAILED)
            wrapped = RemoteDeploymentError("unknown", f"部署阶段异常: {exc}", cause=exc)
            self.deployment_finished.emit(server_id, False, str(wrapped))
            logger.exception(
                f"远端部署阶段未捕获异常: id={server_id}",
                exc,
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise wrapped from exc
        finally:
            self._deploying.discard(server_id)
            self._cancel_events.pop(server_id, None)
            # 失败时不主动 close, 调用方决定是否复用; 成功时也保留连接给后续操作
            _ = backend  # noqa: F841 - keep alive reference comment

    # ==================== SnowLuma 部署路径 (W8) ====================
    def _deploy_snowluma_flavor(
        self,
        server_id: str,
        profile: ServerProfile,
        *,
        progress_callback: DeploymentProgressCallback | None,
        force_linuxqq_reinstall: bool,
    ) -> DeploymentResult:
        """SnowLuma flavor 部署主流程 (W8).

        阶段:
        - Stage 0 (0-1%): preflight 兼容性体检 (复用 NC ``LinuxCoreDeploymentProbe``)
        - Stage 1 (1-40%): install_linuxqq (复用 NC, 安装到 SL workspace)
        - Stage 2 (40-90%): install_snowluma_framework (SFTP lite tarball + 图形栈)
        - Stage 3 (90-95%): upload daemon/bot launcher 脚本
        - Stage 4 (95-100%): verify_deployment + 写回档案 framework_version

        Raises:
            RemoteDeploymentError / RemoteDeploymentCancelledError: 与 NC 路径同语义
        """
        from src.core.remote.snowluma import (
            SnowLumaDeployment,
            SnowLumaFrameworkNotBundledError,
            read_bundled_version,
        )

        assert profile.snowluma_paths is not None, "SL flavor 必须持有 snowluma_paths"
        sl_paths = profile.snowluma_paths

        self._deploying.add(server_id)
        cancel_event = threading.Event()
        self._cancel_events[server_id] = cancel_event
        self.set_deployment_state(server_id, DeploymentState.DEPLOYING)

        def _emit_progress(message: str, percent: int) -> None:
            self.deployment_progress.emit(server_id, message, percent)
            if progress_callback is not None:
                try:
                    progress_callback(message, percent)
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"deploy_server[SL] 外部 progress_callback 抛错: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )

        def _emit_log_line(line: str) -> None:
            self.deployment_log.emit(server_id, line)

        def _emit_log_progress(line: str) -> None:
            self.deployment_log_progress.emit(server_id, line)

        ssh_client: SSHClient | None = None
        try:
            _emit_progress("准备 SSH 连接", 0)
            cred = self._inject_runtime_password(profile)
            ssh_client = SSHClient(cred)
            ssh_client.connect()

            exec_backend = RemoteExecutionBackend(ssh_client)
            sl_deployer = SnowLumaDeployment(exec_backend, sl_paths)

            # ----- Stage 0: preflight -----
            _emit_progress("远端环境体检", 1)
            self.deployment_log.emit(server_id, "[PREFLIGHT] 正在探测远端环境...")
            probe = sl_deployer.probe_environment()
            report = probe.evaluate_compatibility()
            family_text = report.family or "-"
            display_name = (
                report.distro_entry.display_name
                if report.distro_entry is not None
                else (probe.distro_id or "unknown")
            )
            self.deployment_log.emit(
                server_id,
                (
                    f"[PREFLIGHT] distro={display_name} "
                    f"version={probe.distro_version or '-'} "
                    f"arch={probe.normalized_arch or probe.architecture} "
                    f"family={family_text} "
                    f"installer={'dpkg' if probe.has_dpkg else ('rpm2cpio' if probe.has_rpm2cpio else 'none')} "
                    f"status={report.compat_status}"
                ),
            )
            for reason in report.reasons:
                self.deployment_log.emit(server_id, f"[PREFLIGHT] {reason}")
            if report.compat_status == "unsupported":
                detail = "; ".join(report.reasons) or "远端环境不满足部署条件"
                raise RemoteDeploymentError(
                    "preflight",
                    f"远端环境兼容性体检未通过: {detail}",
                )
            # SL 当前只支持 dpkg (install_snowluma.sh 内 apt-get); RHEL 系报错
            if not probe.has_dpkg:
                raise RemoteDeploymentError(
                    "preflight",
                    "SnowLuma 部署当前仅支持 Debian/Ubuntu (apt-get); "
                    "远端缺少 dpkg",
                )
            self._check_cancelled(server_id)

            # ----- Stage 1: install_linuxqq (1-40%) -----
            def _qq_progress(message: str, percent: int) -> None:
                # 0-100 -> 1-40
                _emit_progress(f"[LinuxQQ] {message}", 1 + int(percent * 39 / 100))

            try:
                sl_deployer.install_linuxqq(
                    progress=_qq_progress,
                    log_callback=_emit_log_line,
                    progress_log_callback=_emit_log_progress,
                    force_reinstall=force_linuxqq_reinstall,
                )
            except Exception as exc:  # noqa: BLE001
                if self.is_cancel_requested(server_id):
                    raise RemoteDeploymentCancelledError(cause=exc) from exc
                # 与 NC 分支对齐: 退出码 37 -> LinuxQQ 包完整性校验失败
                stage_label = "install_qq"
                exit_status = getattr(exc, "exit_status", None)
                if exit_status == 37:
                    stage_label = "install_qq_verify"
                    raise RemoteDeploymentError(
                        stage_label,
                        "LinuxQQ 安装包完整性校验失败 (多次下载后仍损坏); "
                        "可能原因: 网络中途中断 / 代理截断 / 镜像源返回不完整内容. "
                        "建议检查远端服务器出方向网络, 或重试部署.",
                        cause=exc,
                    ) from exc
                raise RemoteDeploymentError(
                    stage_label,
                    f"LinuxQQ 安装失败: {exc}",
                    cause=exc,
                ) from exc

            self._check_cancelled(server_id)

            # ----- Stage 2: install_snowluma_framework (40-90%) -----
            def _sl_progress(message: str, percent: int) -> None:
                # 0-100 -> 40-90
                _emit_progress(f"[SnowLuma] {message}", 40 + int(percent * 50 / 100))

            try:
                sl_deployer.install_snowluma_framework(
                    progress=_sl_progress,
                    log_callback=_emit_log_line,
                    progress_log_callback=_emit_log_progress,
                )
            except SnowLumaFrameworkNotBundledError as exc:
                raise RemoteDeploymentError(
                    "install_snowluma_framework",
                    f"Desktop 未捆绑 SnowLuma.Framework: {exc}",
                    cause=exc,
                ) from exc
            except Exception as exc:  # noqa: BLE001
                if self.is_cancel_requested(server_id):
                    raise RemoteDeploymentCancelledError(cause=exc) from exc
                raise RemoteDeploymentError(
                    "install_snowluma_framework",
                    f"SnowLuma.Framework 安装失败: {exc}",
                    cause=exc,
                ) from exc

            self._check_cancelled(server_id)

            # ----- Stage 3: 上传 launcher 脚本 (90-95%) -----
            _emit_progress("上传 launcher 脚本", 90)
            try:
                sl_deployer.upload_daemon_launcher_script()
                sl_deployer.upload_bot_launcher_script()
            except Exception as exc:  # noqa: BLE001
                raise RemoteDeploymentError(
                    "upload_launcher",
                    f"launcher 脚本上传失败: {exc}",
                    cause=exc,
                ) from exc

            self._check_cancelled(server_id)

            # ----- Stage 4: verify + 写回 (95-100%) -----
            _emit_progress("校验远端文件", 95)
            ok, missing = sl_deployer.verify_deployment()
            if not ok:
                raise RemoteDeploymentError(
                    "verify",
                    f"远端关键文件缺失: {', '.join(missing)}",
                )

            framework_version = read_bundled_version()
            updated_profile = self._registry.get(server_id)
            if updated_profile is not None:
                updated_profile.snowluma_framework_version = framework_version
                updated_profile.deployment_state = DeploymentState.DEPLOYED
                self._registry.update(updated_profile)
                self.server_state_changed.emit(
                    server_id, DeploymentState.DEPLOYED.value
                )
                self.server_updated.emit(server_id)

            _emit_progress("部署完成", 100)
            success_message = (
                f"SnowLuma 部署成功 (framework={framework_version or '未知'})"
            )
            self.deployment_finished.emit(server_id, True, success_message)
            return DeploymentResult(
                server_id=server_id,
                ok=True,
                message=success_message,
            )

        except RemoteDeploymentCancelledError as exc:
            self.set_deployment_state(server_id, DeploymentState.UNDEPLOYED)
            self.deployment_log.emit(server_id, "[INFO] 部署已被用户取消")
            self.deployment_finished.emit(server_id, False, str(exc))
            logger.info(
                f"SnowLuma 部署被用户取消: id={server_id}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise
        except RemoteDeploymentError as exc:
            self.set_deployment_state(server_id, DeploymentState.FAILED)
            self.deployment_finished.emit(server_id, False, str(exc))
            logger.warning(
                f"SnowLuma 部署失败: id={server_id}, stage={exc.stage}, msg={exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise
        except Exception as exc:  # noqa: BLE001
            self.set_deployment_state(server_id, DeploymentState.FAILED)
            wrapped = RemoteDeploymentError(
                "unknown", f"SnowLuma 部署阶段异常: {exc}", cause=exc
            )
            self.deployment_finished.emit(server_id, False, str(wrapped))
            logger.exception(
                f"SnowLuma 部署阶段未捕获异常: id={server_id}",
                exc,
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise wrapped from exc
        finally:
            self._deploying.discard(server_id)
            self._cancel_events.pop(server_id, None)
            # SL deploy 用独立 SSH 连接, 部署结束后立即关闭 (与 NC 路径行为不同;
            # NC 复用 RemoteBackend 缓存的连接, SL 暂无对应缓存机制 - W9 优化项).
            if ssh_client is not None and ssh_client.is_connected:
                try:
                    ssh_client.close()
                except Exception:  # noqa: BLE001
                    pass

    # ==================== 仅版本探测 (轻量, 不部署) ====================
    def redetect_versions(self, server_id: str) -> tuple[str | None, str | None]:
        """**同步** 重新探测远端 NapCat / QQ 版本号并写回档案. 

        相比 [`deploy_server`](src/core/remote/server_manager.py) 不会重新执行
        安装脚本, 仅跑 ``detect_installation`` 即可, 用于:
        - 部署完成后的"刷新"按钮
        - 启动时对已部署服务器自动同步版本

        Returns:
            ``(napcat_version, qq_version)`` 元组, 任一字段可能为 None. 

        Raises:
            KeyError: 服务器档案不存在
        """
        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        backend = self.get_backend(server_id)
        backend.connect()
        installation = backend.detect_installation()

        # 写回档案 (即使返回 None 也写, 避免旧值误导)
        updated = self._registry.get(server_id)
        if updated is not None:
            updated.napcat_version = installation.napcat_version
            updated.qq_version = installation.qq_version
            self._registry.update(updated)
            self.server_updated.emit(server_id)
            logger.info(
                f"远端版本探测完成: id={server_id}, "
                f"napcat={installation.napcat_version}, qq={installation.qq_version}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        return installation.napcat_version, installation.qq_version

    # ==================== 回滚安装 (开发者调试用) ====================
    def rollback_server(
        self,
        server_id: str,
        *,
        include_qq: bool = True,
        log_callback: Callable[[str], None] | None = None,
    ) -> None:
        """**同步**清空远端 NapCat (可选含 QQ) 安装, 重置部署状态为 UNDEPLOYED. 

        主要面向开发者反复测试部署场景. **会破坏远端文件**, 调用前需用户确认. 

        Args:
            server_id: 服务器档案 ID
            include_qq: 是否同时清理 QQ 安装与下载的安装包(默认 True)
            log_callback: 每条阶段日志的可选回调, 便于在控制台展示

        Raises:
            KeyError: 服务器档案不存在
            RemoteDeploymentInProgressError: 当前正在部署/回滚中
        """
        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        if server_id in self._deploying:
            raise RemoteDeploymentInProgressError(
                f"服务器 {profile.name} 正在部署/回滚中, 请等待完成"
            )

        def _log(line: str) -> None:
            self.deployment_log.emit(server_id, line)
            if log_callback is not None:
                try:
                    log_callback(line)
                except Exception:  # noqa: BLE001
                    pass

        self._deploying.add(server_id)
        try:
            _log(f"[INFO] 开始回滚远端安装: include_qq={include_qq}")
            self.deployment_progress.emit(server_id, "准备 SSH 连接", 5)
            backend = self.get_backend(server_id)
            backend.connect()
            _log("[INFO] SSH 已连接, 开始清理...")
            self.deployment_progress.emit(server_id, "清理远端文件", 30)

            # 直接复用 LinuxCoreDeployment.clean_environment
            backend.deployment.clean_environment(include_qq=include_qq)
            self.deployment_progress.emit(server_id, "重置档案状态", 90)

            # 重置档案状态
            updated = self._registry.get(server_id)
            if updated is not None:
                updated.napcat_version = None
                updated.qq_version = None
                updated.deployment_state = DeploymentState.UNDEPLOYED
                self._registry.update(updated)
                self.server_state_changed.emit(server_id, DeploymentState.UNDEPLOYED.value)
                self.server_updated.emit(server_id)

            _log("[OK] 回滚完成, 服务器已重置为未部署状态")
            self.deployment_progress.emit(server_id, "回滚完成", 100)
            self.deployment_finished.emit(server_id, True, "回滚完成: 远端环境已清空")
            logger.info(
                f"远端回滚完成: id={server_id}, include_qq={include_qq}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        except Exception as exc:  # noqa: BLE001
            _log(f"[ERROR] 回滚失败: {exc}")
            self.deployment_finished.emit(server_id, False, f"回滚失败: {exc}")
            logger.warning(
                f"远端回滚失败: id={server_id}, exc={exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            raise
        finally:
            self._deploying.discard(server_id)

    # ==================== 资源清理 ====================
    def shutdown(self) -> None:
        """关闭所有缓存的 backend 连接, 应用退出前调用. """
        for server_id, backend in list(self._backend_cache.items()):
            try:
                backend.close()
            except Exception:  # noqa: BLE001
                logger.warning(f"关闭 RemoteBackend 失败: id={server_id}", LogType.NETWORK, LogSource.CORE)
        self._backend_cache.clear()

    # ==================== 内部 ====================
    def _inject_runtime_password(self, profile: ServerProfile) -> SSHCredentials:
        """对密码认证模式, 从内存缓存注入密码生成凭据副本; 否则原样返回. """
        cred = profile.credentials
        if cred.auth_method != "password":
            return cred
        password = self._password_cache.get(profile.id)
        if not password:
            return cred  # 由调用方在 SSHClient.connect() 时报 SSHAuthenticationError
        return replace(cred, password=password)

    def _migrate_legacy_single_server_config(self) -> None:
        """从 ``cfg.remote_*`` 单服务器配置迁移到 ``servers.json``. 

        触发条件: 磁盘上原本不存在 ``servers.json``. 
        若 ``cfg.remote_host`` 为空, 视为干净安装, 直接跳过. 
        """
        try:
            from src.core.config import cfg
        except ImportError:
            return

        try:
            credentials = cfg.build_ssh_credentials()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"读取旧版 cfg.remote_* 配置失败, 跳过迁移: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return

        if not credentials.host.strip():
            return  # 用户从未配置过远程服务器, 无需迁移

        try:
            paths = cfg.build_linux_core_paths()
        except Exception:  # noqa: BLE001
            from .models import LinuxCorePaths

            paths = LinuxCorePaths()

        profile = ServerProfile.create(
            name=f"已迁移服务器 ({credentials.host})",
            credentials=credentials,
            notes="从旧版 v1 单服务器配置自动迁移; 如使用密码认证, 请重新输入密码",
            paths=paths,
        )
        try:
            self._registry.add(profile)
        except ValueError:
            return  # 极端情况下 UUID 冲突, 忽略
        logger.info(
            (
                "已从旧版单服务器配置迁移到 servers.json: "
                f"id={profile.id}, host={credentials.host}, auth={credentials.auth_method}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )


class ServerManagerCreator(AbstractCreator, ABC):
    """``ServerManager`` 单例创建器, 与现有 creart 风格保持一致. """

    targets = (
        CreateTargetInfo(
            module="src.core.remote.server_manager",
            identify="ServerManager",
            humanized_name="服务器管理器",
            description="多服务器档案管理与持久化",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.remote.server_manager")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(ServerManagerCreator)
