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


class DeploymentState(str, Enum):
    """远端 NapCat 部署状态. """

    UNDEPLOYED = "undeployed"
    DEPLOYING = "deploying"
    DEPLOYED = "deployed"
    FAILED = "failed"


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

    @classmethod
    def create(
        cls,
        *,
        name: str,
        credentials: SSHCredentials,
        notes: str = "",
        paths: LinuxCorePaths | None = None,
    ) -> "ServerProfile":
        """构造新档案, 自动生成 UUID. """
        display_name = name.strip() or credentials.host or "未命名服务器"
        return cls(
            id=str(uuid4()),
            name=display_name,
            credentials=credentials,
            paths=paths or LinuxCorePaths(),
            notes=notes.strip(),
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
