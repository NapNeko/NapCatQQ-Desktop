# -*- coding: utf-8 -*-
"""服务器档案模型与多服务器持久化注册表. 

对应 [`docs/general/remote_ssh_plan.md`](../../../../docs/general/remote_ssh_plan.md) §3.2 设计. 

安全基线(参考 §6.2):
- SSH 密码不写入磁盘, 仅在内存中保留
- 私钥 passphrase 不写入磁盘
- 私钥**路径**(非内容)可写入磁盘

存储路径: ``{data_path}/runtime/config/servers.json``. 
JSON 文件损坏或缺失时静默初始化为空, 不阻断 Desktop 启动. 
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from time import time
from typing import Any
from uuid import uuid4

from .models import LinuxCorePaths, SSHCredentials
from .snowluma.paths import SnowLumaRemotePaths


class DeploymentState(str, Enum):
    """远端 NapCat 部署状态. """

    UNDEPLOYED = "undeployed"
    DEPLOYING = "deploying"
    DEPLOYED = "deployed"
    FAILED = "failed"


class BackendFlavor(str, Enum):
    """ServerProfile 后端类型 (W7 D8 决策, 不新建 ProfileType, 扩字段).

    一旦创建即不可变 (per-server flavor 互斥); UI 在 ``AddServerDialog`` 让用户选,
    选定后不允许编辑 (重选需删除档案重建).
    """

    NAPCAT = "napcat"
    SNOWLUMA = "snowluma"


@dataclass
class ServerProfile:
    """单台远程服务器的配置档案. 

    Attributes:
        id: 服务器唯一标识(UUID4 字符串)
        name: 用户可读显示名
        credentials: SSH 凭据; 密码与 passphrase 不参与磁盘持久化
        paths: 远端目录布局
        deployment_state: 部署状态机
        napcat_version: 上次探测到的 NapCat 版本(缓存展示)
        qq_version: 上次探测到的 QQ 版本(缓存展示)
        notes: 备注
        created_at: 创建时间(Unix 秒)
        last_connected_at: 最近一次成功连接的时间, 未连接时为 None
    """

    id: str
    name: str
    credentials: SSHCredentials
    paths: LinuxCorePaths = field(default_factory=LinuxCorePaths)
    deployment_state: DeploymentState = DeploymentState.UNDEPLOYED
    napcat_version: str | None = None
    qq_version: str | None = None
    notes: str = ""
    created_at: float = field(default_factory=time)
    last_connected_at: float | None = None
    # W7 (D8 决策): 后端类型 + SL 专用字段; ``backend_flavor=NAPCAT`` 时下方 SL
    # 字段保持默认值 (None / "") 不参与运行时逻辑
    backend_flavor: BackendFlavor = BackendFlavor.NAPCAT
    snowluma_paths: SnowLumaRemotePaths | None = None
    """flavor=snowluma 时使用; flavor=napcat 时强制 None"""
    snowluma_webui_password_override: str = ""
    """per-server WebUI 密码 (OQ4 决策); 空串表示走 App 级 fallback"""
    snowluma_framework_version: str | None = None
    """远端已部署的 SnowLuma.Framework 版本; 与 Desktop 内置 bundled_version
    不一致时 UI 提示 "可升级" (deploy 期写入)"""

    @classmethod
    def create(
        cls,
        *,
        name: str,
        credentials: SSHCredentials,
        notes: str = "",
        paths: LinuxCorePaths | None = None,
        backend_flavor: BackendFlavor = BackendFlavor.NAPCAT,
        snowluma_paths: SnowLumaRemotePaths | None = None,
        snowluma_webui_password_override: str = "",
    ) -> "ServerProfile":
        """构造新档案, 自动生成 UUID.

        Args:
            backend_flavor: ``NAPCAT`` (默认) 或 ``SNOWLUMA``; 一经设定不可变
            snowluma_paths: 仅 flavor=snowluma 时使用; flavor=napcat 时静默 ``None``;
                flavor=snowluma 而 ``snowluma_paths`` 缺省时使用
                :meth:`SnowLumaRemotePaths.from_base` 默认布局 (``$HOME/snowluma-remote``)
            snowluma_webui_password_override: per-server WebUI 密码; 空串走 App fallback

        Notes:
            P13 (review): 早期文档曾写 "flavor=snowluma 而未提供 ``snowluma_paths`` 时
            raise ``ValueError``"; 当前实现是回退到 :meth:`SnowLumaRemotePaths.from_base`
            默认布局 — docstring 已与代码对齐. 若调用方希望强制提供, 应在 UI 层校验.
        """
        display_name = name.strip() or credentials.host or "未命名服务器"
        if backend_flavor == BackendFlavor.SNOWLUMA:
            sl_paths = snowluma_paths or SnowLumaRemotePaths.from_base()
            nc_paths = LinuxCorePaths()  # 占位; SL flavor 下不参与运行时逻辑
        else:
            sl_paths = None  # NC flavor 不持有 SL paths
            nc_paths = paths or LinuxCorePaths()
        return cls(
            id=str(uuid4()),
            name=display_name,
            credentials=credentials,
            paths=nc_paths,
            notes=notes.strip(),
            backend_flavor=backend_flavor,
            snowluma_paths=sl_paths,
            snowluma_webui_password_override=snowluma_webui_password_override.strip(),
        )

    def to_dict(self) -> dict[str, Any]:
        """序列化为 JSON 兼容结构, 跳过敏感字段. """
        cred = self.credentials
        cred_payload: dict[str, Any] = {
            "host": cred.host,
            "port": cred.port,
            "username": cred.username,
            "auth_method": cred.auth_method,
            # 密码与 passphrase 故意不写盘
            "private_key_path": cred.private_key_path,
            "connect_timeout": cred.connect_timeout,
            "command_timeout": cred.command_timeout,
            "script_timeout": cred.script_timeout,
            "host_key_policy": cred.host_key_policy,
            "allow_agent": cred.allow_agent,
            "look_for_keys": cred.look_for_keys,
        }
        paths_payload: dict[str, Any] = {
            "workspace_dir": self.paths.workspace_dir,
            "runtime_dir": self.paths.runtime_dir,
            "config_dir": self.paths.config_dir,
            "log_dir": self.paths.log_dir,
            "tmp_dir": self.paths.tmp_dir,
            "package_dir": self.paths.package_dir,
        }
        # W7: SL paths 字段; SL flavor 时序列化全字段, NC flavor 时省略 (默认 None)
        snowluma_paths_payload: dict[str, Any] | None = None
        if self.snowluma_paths is not None:
            snowluma_paths_payload = {
                "base_dir": self.snowluma_paths.base_dir,
                "workspace_dir": self.snowluma_paths.workspace_dir,
                "snowluma_framework_dir": self.snowluma_paths.snowluma_framework_dir,
                "config_dir": self.snowluma_paths.config_dir,
                "runtime_dir": self.snowluma_paths.runtime_dir,
                "log_dir": self.snowluma_paths.log_dir,
                "vnc_secret": self.snowluma_paths.vnc_secret,
                "webui_secret": self.snowluma_paths.webui_secret,
                "daemon_launcher_script": self.snowluma_paths.daemon_launcher_script,
                "bot_launcher_script": self.snowluma_paths.bot_launcher_script,
            }
        return {
            "id": self.id,
            "name": self.name,
            "credentials": cred_payload,
            "paths": paths_payload,
            "deployment_state": self.deployment_state.value,
            "napcat_version": self.napcat_version,
            "qq_version": self.qq_version,
            "notes": self.notes,
            "created_at": self.created_at,
            "last_connected_at": self.last_connected_at,
            # W7 字段; 缺失时旧 Desktop 反序列化静默走默认 NAPCAT
            "backend_flavor": self.backend_flavor.value,
            "snowluma_paths": snowluma_paths_payload,
            "snowluma_webui_password_override": self.snowluma_webui_password_override,
            "snowluma_framework_version": self.snowluma_framework_version,
        }

    @classmethod
    def from_dict(cls, payload: dict[str, Any]) -> "ServerProfile":
        """从 JSON 结构反序列化, 字段缺失时使用安全默认值. """
        cred_payload = payload.get("credentials") or {}
        defaults_paths = LinuxCorePaths()
        credentials = SSHCredentials(
            host=str(cred_payload.get("host", "")),
            port=int(cred_payload.get("port", 22)),
            username=str(cred_payload.get("username", "")),
            auth_method=cred_payload.get("auth_method", "key"),
            password=None,
            private_key_path=cred_payload.get("private_key_path"),
            private_key_passphrase=None,
            connect_timeout=float(cred_payload.get("connect_timeout", 10.0)),
            command_timeout=float(cred_payload.get("command_timeout", 20.0)),
            script_timeout=float(cred_payload.get("script_timeout", 1800.0)),
            host_key_policy=cred_payload.get("host_key_policy", "reject"),
            allow_agent=bool(cred_payload.get("allow_agent", False)),
            look_for_keys=bool(cred_payload.get("look_for_keys", False)),
        )
        paths_payload = payload.get("paths") or {}
        try:
            paths = LinuxCorePaths(
                workspace_dir=str(paths_payload.get("workspace_dir", defaults_paths.workspace_dir)),
                runtime_dir=str(paths_payload.get("runtime_dir", defaults_paths.runtime_dir)),
                config_dir=str(paths_payload.get("config_dir", defaults_paths.config_dir)),
                log_dir=str(paths_payload.get("log_dir", defaults_paths.log_dir)),
                tmp_dir=str(paths_payload.get("tmp_dir", defaults_paths.tmp_dir)),
                package_dir=str(paths_payload.get("package_dir", defaults_paths.package_dir)),
            )
        except ValueError as exc:
            # P5 F2.3: ``servers.json`` 内非法路径退化到默认值, 不阻断 Desktop 启动.
            # 下次 ``servers.json`` save 会用默认值覆盖原非法值, 起到自愈效果.
            try:
                from src.core.logging import LogSource, LogType, logger

                logger.warning(
                    f"servers.json 中存在非法 LinuxCorePaths, 已退化到默认: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
            except Exception:  # noqa: BLE001 - 日志失败不应再抛
                pass
            paths = LinuxCorePaths()
        try:
            deployment_state = DeploymentState(payload.get("deployment_state", "undeployed"))
        except ValueError:
            deployment_state = DeploymentState.UNDEPLOYED
        last_connected_raw = payload.get("last_connected_at")
        last_connected = float(last_connected_raw) if isinstance(last_connected_raw, (int, float)) else None

        # W7: backend_flavor 反序列化; 未知值降级为 NAPCAT (向后兼容旧 servers.json)
        try:
            backend_flavor = BackendFlavor(payload.get("backend_flavor", "napcat"))
        except ValueError:
            backend_flavor = BackendFlavor.NAPCAT

        # W7: snowluma_paths 反序列化 (仅 SL flavor 期望非空; 校验失败降级 None)
        snowluma_paths_value: SnowLumaRemotePaths | None = None
        sl_payload = payload.get("snowluma_paths")
        if isinstance(sl_payload, dict):
            try:
                snowluma_paths_value = SnowLumaRemotePaths(
                    base_dir=str(sl_payload.get("base_dir", "$HOME/snowluma-remote")),
                    workspace_dir=str(sl_payload.get("workspace_dir", "")),
                    snowluma_framework_dir=str(sl_payload.get("snowluma_framework_dir", "")),
                    config_dir=str(sl_payload.get("config_dir", "")),
                    runtime_dir=str(sl_payload.get("runtime_dir", "")),
                    log_dir=str(sl_payload.get("log_dir", "")),
                    vnc_secret=str(sl_payload.get("vnc_secret", "")),
                    webui_secret=str(sl_payload.get("webui_secret", "")),
                    daemon_launcher_script=str(sl_payload.get("daemon_launcher_script", "")),
                    bot_launcher_script=str(sl_payload.get("bot_launcher_script", "")),
                )
            except ValueError as exc:
                try:
                    from src.core.logging import LogSource, LogType, logger

                    logger.warning(
                        f"servers.json 中存在非法 SnowLumaRemotePaths, 降级到默认: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
                except Exception:  # noqa: BLE001
                    pass
                snowluma_paths_value = SnowLumaRemotePaths.from_base()
        elif backend_flavor == BackendFlavor.SNOWLUMA:
            # SL flavor 但 payload 缺 snowluma_paths: 用默认值
            snowluma_paths_value = SnowLumaRemotePaths.from_base()

        # NC flavor 时强制 snowluma_paths = None (避免 schema 误用)
        if backend_flavor == BackendFlavor.NAPCAT:
            snowluma_paths_value = None

        return cls(
            id=str(payload.get("id") or uuid4()),
            name=str(payload.get("name") or credentials.host or "未命名服务器"),
            credentials=credentials,
            paths=paths,
            deployment_state=deployment_state,
            napcat_version=payload.get("napcat_version"),
            qq_version=payload.get("qq_version"),
            notes=str(payload.get("notes") or ""),
            created_at=float(payload.get("created_at") or time()),
            last_connected_at=last_connected,
            backend_flavor=backend_flavor,
            snowluma_paths=snowluma_paths_value,
            snowluma_webui_password_override=str(
                payload.get("snowluma_webui_password_override") or ""
            ),
            snowluma_framework_version=payload.get("snowluma_framework_version"),
        )


class ServerRegistry:
    """多服务器档案的内存索引 + JSON 持久化. 

    采用 "临时文件 + replace" 原子写入, 避免崩溃损坏 servers.json. 
    线程安全性: 当前实现非线程安全, 调用方应在 Qt 主线程使用,
    或通过 [`ServerManager`](src/core/remote/server_manager.py) 提供的 Qt 信号桥接. 
    """

    SCHEMA_VERSION = 1

    def __init__(self, storage_path: str | Path) -> None:
        self._storage_path = Path(storage_path)
        self._profiles: dict[str, ServerProfile] = {}
        self.load()

    @property
    def storage_path(self) -> Path:
        return self._storage_path

    # ---------- 持久化 ----------
    def load(self) -> None:
        """从磁盘加载档案; 文件缺失或损坏时静默初始化为空. """
        self._profiles.clear()
        if not self._storage_path.exists():
            return

        try:
            raw = self._storage_path.read_text(encoding="utf-8")
            payload = json.loads(raw)
        except (OSError, json.JSONDecodeError):
            return

        if not isinstance(payload, dict):
            return

        servers = payload.get("servers")
        if not isinstance(servers, list):
            return

        for entry in servers:
            if not isinstance(entry, dict):
                continue
            try:
                profile = ServerProfile.from_dict(entry)
            except Exception:  # noqa: BLE001 - 单条损坏不应影响其他档案
                continue
            self._profiles[profile.id] = profile

    def save(self) -> None:
        """原子写入磁盘, 父目录不存在时自动创建. """
        self._storage_path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "schema_version": self.SCHEMA_VERSION,
            "servers": [profile.to_dict() for profile in self._profiles.values()],
        }
        tmp_path = self._storage_path.with_name(self._storage_path.name + ".tmp")
        tmp_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        tmp_path.replace(self._storage_path)

    # ---------- CRUD ----------
    def list(self) -> list[ServerProfile]:
        """返回当前所有档案的快照(按创建时间排序). """
        return sorted(self._profiles.values(), key=lambda p: p.created_at)

    def get(self, server_id: str) -> ServerProfile | None:
        return self._profiles.get(server_id)

    def add(self, profile: ServerProfile) -> None:
        """添加新档案; ID 已存在时抛 ValueError. """
        if profile.id in self._profiles:
            raise ValueError(f"服务器档案已存在: {profile.id}")
        self._profiles[profile.id] = profile
        self.save()

    def update(self, profile: ServerProfile) -> None:
        """覆盖现有档案; ID 不存在时抛 KeyError. """
        if profile.id not in self._profiles:
            raise KeyError(f"服务器档案不存在: {profile.id}")
        self._profiles[profile.id] = profile
        self.save()

    def remove(self, server_id: str) -> bool:
        """删除档案, 返回是否存在并删除成功. """
        if server_id not in self._profiles:
            return False
        del self._profiles[server_id]
        self.save()
        return True

    # ---------- 集合协议 ----------
    def __len__(self) -> int:
        return len(self._profiles)

    def __contains__(self, server_id: object) -> bool:
        return isinstance(server_id, str) and server_id in self._profiles

    def __iter__(self):
        return iter(self.list())
