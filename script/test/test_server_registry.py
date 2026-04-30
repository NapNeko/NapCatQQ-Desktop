# -*- coding: utf-8 -*-
"""[`ServerProfile`](src/desktop/core/remote/servers.py) 与 [`ServerRegistry`](src/desktop/core/remote/servers.py) 单元测试。

P0 验收要点:
- 序列化往返保留所有字段(密码与 passphrase 除外)
- 密码与 passphrase **绝对不写盘**(参考 §6.2)
- JSON 损坏时不阻断启动, 静默初始化为空
- CRUD 触发原子写, 不会留下半成品文件
"""
from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.desktop.core.remote.models import LinuxCorePaths, SSHCredentials
from src.desktop.core.remote.servers import (
    DeploymentState,
    ServerProfile,
    ServerRegistry,
)


def _make_password_profile() -> ServerProfile:
    cred = SSHCredentials(
        host="example.com",
        port=2222,
        username="root",
        auth_method="password",
        password="s3cr3t!",
        connect_timeout=15.0,
        command_timeout=30.0,
        host_key_policy="reject",
    )
    return ServerProfile.create(
        name="生产服务器",
        credentials=cred,
        notes="主力机",
    )


def _make_key_profile() -> ServerProfile:
    cred = SSHCredentials(
        host="dev.example.com",
        port=22,
        username="ubuntu",
        auth_method="key",
        password=None,
        private_key_path="C:\\Users\\me\\.ssh\\id_rsa",
        private_key_passphrase="passphrase!!!",
        connect_timeout=10.0,
        command_timeout=20.0,
        host_key_policy="warning",
    )
    return ServerProfile.create(name="开发服务器", credentials=cred)


# ==================== ServerProfile ====================
class TestServerProfile:
    def test_create_assigns_uuid_and_defaults(self) -> None:
        profile = _make_password_profile()
        assert profile.id  # 非空
        assert len(profile.id) >= 16
        assert profile.deployment_state is DeploymentState.UNDEPLOYED
        assert profile.napcat_version is None
        assert profile.qq_version is None
        assert profile.last_connected_at is None
        assert profile.created_at > 0

    def test_serialization_round_trip_drops_password(self) -> None:
        profile = _make_password_profile()
        payload = profile.to_dict()

        # 关键安全断言: 密码不在 payload 中
        cred_payload = payload["credentials"]
        assert "password" not in cred_payload
        # 私钥 passphrase 也不在
        assert "private_key_passphrase" not in cred_payload

        restored = ServerProfile.from_dict(payload)
        assert restored.id == profile.id
        assert restored.name == profile.name
        assert restored.notes == profile.notes
        assert restored.created_at == profile.created_at
        # 密码字段被剥离, 反序列化后为 None
        assert restored.credentials.password is None
        # 其他字段完整
        assert restored.credentials.host == profile.credentials.host
        assert restored.credentials.port == profile.credentials.port
        assert restored.credentials.auth_method == profile.credentials.auth_method
        assert restored.credentials.connect_timeout == profile.credentials.connect_timeout
        assert restored.credentials.host_key_policy == profile.credentials.host_key_policy

    def test_key_profile_passphrase_not_persisted(self) -> None:
        profile = _make_key_profile()
        payload = profile.to_dict()
        assert "private_key_passphrase" not in payload["credentials"]
        # 但**路径**应保留
        assert payload["credentials"]["private_key_path"] == profile.credentials.private_key_path

    def test_paths_round_trip(self) -> None:
        cred = SSHCredentials(host="x", username="u", auth_method="password", password="p")
        profile = ServerProfile.create(
            name="自定义路径",
            credentials=cred,
            paths=LinuxCorePaths(workspace_dir="$HOME/my_napcat"),
        )
        restored = ServerProfile.from_dict(profile.to_dict())
        assert restored.paths.workspace_dir == "$HOME/my_napcat"

    def test_from_dict_missing_fields_uses_defaults(self) -> None:
        # 仅 host/auth_method 的最小载荷
        minimal = {"credentials": {"host": "h", "auth_method": "key", "username": "u"}}
        restored = ServerProfile.from_dict(minimal)
        assert restored.credentials.host == "h"
        assert restored.credentials.port == 22  # 默认端口
        assert restored.deployment_state is DeploymentState.UNDEPLOYED

    def test_from_dict_invalid_state_falls_back(self) -> None:
        payload = _make_password_profile().to_dict()
        payload["deployment_state"] = "totally_invalid_state"
        restored = ServerProfile.from_dict(payload)
        assert restored.deployment_state is DeploymentState.UNDEPLOYED


# ==================== ServerRegistry ====================
class TestServerRegistry:
    def test_load_empty_when_file_missing(self, tmp_path: Path) -> None:
        registry = ServerRegistry(tmp_path / "servers.json")
        assert len(registry) == 0
        assert registry.list() == []

    def test_add_persists_to_disk(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        profile = _make_key_profile()
        registry.add(profile)

        # 文件已写入
        assert path.exists()
        # 重新加载等价于
        registry2 = ServerRegistry(path)
        assert profile.id in registry2
        assert registry2.get(profile.id) is not None
        assert registry2.get(profile.id).name == profile.name

    def test_password_never_persisted(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        profile = _make_password_profile()
        registry.add(profile)

        raw = path.read_text(encoding="utf-8")
        # 关键安全断言: 写入磁盘的内容里不含密码字面量
        assert "s3cr3t!" not in raw
        payload = json.loads(raw)
        cred = payload["servers"][0]["credentials"]
        assert "password" not in cred

    def test_passphrase_never_persisted(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        profile = _make_key_profile()
        registry.add(profile)
        raw = path.read_text(encoding="utf-8")
        assert "passphrase!!!" not in raw

    def test_add_duplicate_id_raises(self, tmp_path: Path) -> None:
        registry = ServerRegistry(tmp_path / "servers.json")
        profile = _make_key_profile()
        registry.add(profile)
        with pytest.raises(ValueError):
            registry.add(profile)

    def test_update_unknown_raises(self, tmp_path: Path) -> None:
        registry = ServerRegistry(tmp_path / "servers.json")
        with pytest.raises(KeyError):
            registry.update(_make_key_profile())

    def test_update_persists_changes(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        profile = _make_key_profile()
        registry.add(profile)

        profile.name = "重命名后的服务器"
        profile.deployment_state = DeploymentState.DEPLOYED
        registry.update(profile)

        registry2 = ServerRegistry(path)
        restored = registry2.get(profile.id)
        assert restored.name == "重命名后的服务器"
        assert restored.deployment_state is DeploymentState.DEPLOYED

    def test_remove(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        profile = _make_key_profile()
        registry.add(profile)
        assert registry.remove(profile.id) is True
        assert registry.get(profile.id) is None
        # 重复删除返回 False
        assert registry.remove(profile.id) is False

        # 持久化反映删除
        registry2 = ServerRegistry(path)
        assert len(registry2) == 0

    def test_list_sorted_by_created_at(self, tmp_path: Path) -> None:
        registry = ServerRegistry(tmp_path / "servers.json")
        first = _make_key_profile()
        first.created_at = 1000.0
        second = _make_password_profile()
        second.created_at = 2000.0
        registry.add(second)
        registry.add(first)

        listed = registry.list()
        assert listed[0].id == first.id
        assert listed[1].id == second.id

    def test_load_corrupt_json_does_not_raise(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        path.write_text("{ this is not valid json", encoding="utf-8")
        registry = ServerRegistry(path)
        # 静默初始化为空, 不阻断启动
        assert len(registry) == 0

    def test_load_non_object_payload_does_not_raise(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        path.write_text('["unexpected", "list"]', encoding="utf-8")
        registry = ServerRegistry(path)
        assert len(registry) == 0

    def test_load_skips_invalid_entries_keeps_valid(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        valid = _make_key_profile()
        path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "servers": [
                        {"not_a_real_profile": True},
                        valid.to_dict(),
                        "string-instead-of-object",
                    ],
                }
            ),
            encoding="utf-8",
        )
        registry = ServerRegistry(path)
        assert len(registry) >= 1
        assert valid.id in registry

    def test_atomic_write_no_leftover_tmp(self, tmp_path: Path) -> None:
        path = tmp_path / "servers.json"
        registry = ServerRegistry(path)
        registry.add(_make_key_profile())
        # 原子写完成后不应残留 .tmp
        leftover = list(tmp_path.glob("servers.json.tmp"))
        assert leftover == []

    def test_iter_and_contains(self, tmp_path: Path) -> None:
        registry = ServerRegistry(tmp_path / "servers.json")
        profile = _make_key_profile()
        registry.add(profile)

        ids = [p.id for p in registry]
        assert profile.id in ids
        assert profile.id in registry
        assert "not-an-id" not in registry


# ==================== 本地 SSH 密钥扫描 ====================
class TestScanLocalSSHKeys:
    """覆盖 [`scan_local_ssh_keys`](src/desktop/core/remote/ssh_keys.py)
    的关键行为, 注意此辅助函数与计划 §6.2 安全基线兼容: 仅作 UI 候选, 不会自动建立连接。"""

    def test_returns_empty_when_no_ssh_dir(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        # 模拟 home 目录下没有 .ssh
        fake_home = tmp_path / "home"
        fake_home.mkdir()
        monkeypatch.setattr(Path, "home", lambda: fake_home)

        from src.desktop.core.remote.ssh_keys import scan_local_ssh_keys

        assert scan_local_ssh_keys() == []

    def test_picks_existing_keys_in_priority_order(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        fake_home = tmp_path / "home"
        ssh_dir = fake_home / ".ssh"
        ssh_dir.mkdir(parents=True)
        # 故意按相反顺序创建文件, 验证函数仍按优先级排序
        (ssh_dir / "id_rsa").write_text("rsa-key", encoding="utf-8")
        (ssh_dir / "id_ed25519").write_text("ed25519-key", encoding="utf-8")
        # 干扰文件
        (ssh_dir / "id_rsa.pub").write_text("public-key-should-be-skipped", encoding="utf-8")
        (ssh_dir / "config").write_text("Host *", encoding="utf-8")
        (ssh_dir / "known_hosts").write_text("", encoding="utf-8")

        monkeypatch.setattr(Path, "home", lambda: fake_home)
        from src.desktop.core.remote.ssh_keys import scan_local_ssh_keys

        keys = scan_local_ssh_keys()
        assert len(keys) == 2
        # ed25519 优先级高于 rsa
        assert keys[0].endswith("id_ed25519")
        assert keys[1].endswith("id_rsa")
        # 公钥与配置文件不应被收录
        assert not any(k.endswith(".pub") for k in keys)
        assert not any(k.endswith("config") for k in keys)
        assert not any(k.endswith("known_hosts") for k in keys)

    def test_skips_non_existing_standard_names(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        fake_home = tmp_path / "home"
        ssh_dir = fake_home / ".ssh"
        ssh_dir.mkdir(parents=True)
        # 只有 id_ecdsa 存在
        (ssh_dir / "id_ecdsa").write_text("ecdsa-key", encoding="utf-8")

        monkeypatch.setattr(Path, "home", lambda: fake_home)
        from src.desktop.core.remote.ssh_keys import scan_local_ssh_keys

        keys = scan_local_ssh_keys()
        assert len(keys) == 1
        assert keys[0].endswith("id_ecdsa")
