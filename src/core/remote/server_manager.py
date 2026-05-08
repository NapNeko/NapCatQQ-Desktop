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

from abc import ABC
from collections.abc import Callable
from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QObject, Signal

from src.core.logging import LogSource, LogType, logger

from .credential_store import CredentialStore
from .errors import RemoteDeploymentError, RemoteDeploymentInProgressError, RemoteError
from .models import SSHCredentials
from .servers import DeploymentState, ServerProfile, ServerRegistry
from .ssh_client import SSHClient

if TYPE_CHECKING:
    from src.core.operation.remote_backend import RemoteBackend


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
        # 服务器 ID -> RemoteBackend 实例(惰性创建并缓存)
        self._backend_cache: dict[str, "RemoteBackend"] = {}
        # P1: 服务器 ID -> 是否正在部署(防并发)
        self._deploying: set[str] = set()
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
    def get_backend(self, server_id: str) -> "RemoteBackend":
        """获取或惰性创建 ``RemoteBackend`` 实例; 服务器不存在时抛 KeyError. """
        cached = self._backend_cache.get(server_id)
        if cached is not None:
            return cached

        profile = self._registry.get(server_id)
        if profile is None:
            raise KeyError(f"服务器档案不存在: {server_id}")

        # 延迟导入避免与 [`core.operation`](src/core/operation/__init__.py) 之间的循环
        from src.core.operation.remote_backend import RemoteBackend

        cred = self._inject_runtime_password(profile)
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

    # ==================== 部署编排 (P1)  ====================
    def is_deploying(self, server_id: str) -> bool:
        """是否有正在进行中的部署任务. """
        return server_id in self._deploying

    def deploy_server(
        self,
        server_id: str,
        *,
        progress_callback: DeploymentProgressCallback | None = None,
        force_napcat_update: bool = False,
        force_linuxqq_reinstall: bool = False,
    ) -> DeploymentResult:
        """**同步**执行远端部署(应在后台线程调用). 

        编排 install_qq + install_napcat 两步, 把进度区间映射到统一的 0-100. 

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

        self._deploying.add(server_id)
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

        backend = None
        try:
            _emit_progress("准备 SSH 连接", 0)
            backend = self.get_backend(server_id)
            backend.connect()

            # ----- Stage 1: install_qq, 0-50 -----
            def _qq_progress(message: str, percent: int) -> None:
                # 0-100 -> 0-50
                _emit_progress(f"[LinuxQQ] {message}", int(percent / 2))

            try:
                backend.install_qq(
                    progress=_qq_progress,
                    log_callback=_emit_log_line,
                    force_reinstall=force_linuxqq_reinstall,
                )
            except Exception as exc:  # noqa: BLE001 - 统一封装为 RemoteDeploymentError
                raise RemoteDeploymentError(
                    "install_qq",
                    f"LinuxQQ 安装失败: {exc}",
                    cause=exc,
                ) from exc

            # ----- Stage 2: install_napcat, 50-100 -----
            # P5 F1.4: 在调用 install_napcat 之前先查询上游 SHA512;
            # 取不到时静默跳过 (远端脚本端 fallback 为 warn skip), 不阻断部署
            # - 远端 worker 线程不便弹用户对话框, 这里走"网络不稳时允许继续"策略.
            expected_sha512 = self._lookup_napcat_expected_sha512()

            def _napcat_progress(message: str, percent: int) -> None:
                _emit_progress(f"[NapCat] {message}", 50 + int(percent / 2))

            try:
                backend.install_napcat(
                    progress=_napcat_progress,
                    log_callback=_emit_log_line,
                    force_update=force_napcat_update,
                    expected_sha512=expected_sha512,
                )
            except Exception as exc:  # noqa: BLE001
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
            # 失败时不主动 close, 调用方决定是否复用; 成功时也保留连接给后续操作
            _ = backend  # noqa: F841 - keep alive reference comment

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
