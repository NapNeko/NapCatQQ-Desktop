# -*- coding: utf-8 -*-
""":class:`ServerProfile` SnowLuma flavor 扩展单测 (W7).

覆盖:

- ``BackendFlavor`` 枚举 + ``ServerProfile.create`` 双 flavor 路径
- ``to_dict`` / ``from_dict`` round-trip (NC + SL 双 flavor)
- 向后兼容: 旧 servers.json (无 backend_flavor 字段) 反序列化为 NAPCAT
- SL flavor 但缺 snowluma_paths payload → 用默认值
- NC flavor 不应持有 snowluma_paths (强制 None)
- 未知 backend_flavor 字符串降级到 NAPCAT
- 非法 SnowLumaRemotePaths payload 降级到默认
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from src.core.remote import (
    BackendFlavor,
    DeploymentState,
    ServerProfile,
    ServerRegistry,
    SSHCredentials,
)
from src.core.remote.snowluma import SnowLumaRemotePaths


@pytest.fixture
def basic_credentials() -> SSHCredentials:
    return SSHCredentials(
        host="example.com",
        username="ubuntu",
        port=22,
        auth_method="password",
        password="x",
    )


# ==================== create 双 flavor ====================
class TestCreate:
    def test_default_napcat_flavor(self, basic_credentials: SSHCredentials) -> None:
        p = ServerProfile.create(name="np", credentials=basic_credentials)
        assert p.backend_flavor == BackendFlavor.NAPCAT
        assert p.snowluma_paths is None
        assert p.snowluma_webui_password_override == ""
        assert p.snowluma_framework_version is None

    def test_explicit_snowluma_flavor_default_paths(
        self, basic_credentials: SSHCredentials
    ) -> None:
        p = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
        )
        assert p.backend_flavor == BackendFlavor.SNOWLUMA
        assert p.snowluma_paths is not None
        assert p.snowluma_paths.base_dir == "$HOME/snowluma-remote"

    def test_explicit_snowluma_with_custom_paths(
        self, basic_credentials: SSHCredentials
    ) -> None:
        custom = SnowLumaRemotePaths.from_base("/opt/sl-custom")
        p = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
            snowluma_paths=custom,
            snowluma_webui_password_override="  abc  ",
        )
        assert p.snowluma_paths is custom
        # 密码 strip 处理
        assert p.snowluma_webui_password_override == "abc"

    def test_napcat_flavor_ignores_snowluma_paths_arg(
        self, basic_credentials: SSHCredentials
    ) -> None:
        """NC flavor 即使误传 snowluma_paths 也强制 None."""
        custom = SnowLumaRemotePaths.from_base("/opt/sl")
        p = ServerProfile.create(
            name="np",
            credentials=basic_credentials,
            snowluma_paths=custom,  # 误传
        )
        assert p.backend_flavor == BackendFlavor.NAPCAT
        assert p.snowluma_paths is None


# ==================== to_dict / from_dict round-trip ====================
class TestSerialization:
    def test_napcat_round_trip(self, basic_credentials: SSHCredentials) -> None:
        original = ServerProfile.create(name="np", credentials=basic_credentials)
        original.deployment_state = DeploymentState.DEPLOYED
        original.napcat_version = "v4.18.1"
        original.qq_version = "3.2.13"

        payload = original.to_dict()
        # 关键字段都序列化了
        assert payload["backend_flavor"] == "napcat"
        assert payload["snowluma_paths"] is None
        assert payload["snowluma_webui_password_override"] == ""
        assert payload["snowluma_framework_version"] is None

        recovered = ServerProfile.from_dict(payload)
        assert recovered.backend_flavor == BackendFlavor.NAPCAT
        assert recovered.snowluma_paths is None
        assert recovered.napcat_version == "v4.18.1"
        assert recovered.deployment_state == DeploymentState.DEPLOYED

    def test_snowluma_round_trip(self, basic_credentials: SSHCredentials) -> None:
        sl_paths = SnowLumaRemotePaths.from_base("/opt/sl")
        original = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
            snowluma_paths=sl_paths,
            snowluma_webui_password_override="secret123",
        )
        original.snowluma_framework_version = "0.1.0"

        payload = original.to_dict()
        assert payload["backend_flavor"] == "snowluma"
        assert payload["snowluma_paths"] is not None
        assert payload["snowluma_paths"]["base_dir"] == "/opt/sl"
        assert payload["snowluma_paths"]["workspace_dir"] == "/opt/sl/workspace"
        assert payload["snowluma_webui_password_override"] == "secret123"
        assert payload["snowluma_framework_version"] == "0.1.0"

        recovered = ServerProfile.from_dict(payload)
        assert recovered.backend_flavor == BackendFlavor.SNOWLUMA
        assert recovered.snowluma_paths is not None
        assert recovered.snowluma_paths.base_dir == "/opt/sl"
        assert recovered.snowluma_paths.workspace_dir == "/opt/sl/workspace"
        assert recovered.snowluma_framework_version == "0.1.0"


# ==================== 向后兼容 ====================
class TestBackwardCompat:
    def test_legacy_payload_without_flavor_defaults_napcat(
        self, basic_credentials: SSHCredentials
    ) -> None:
        """旧 Desktop 写的 servers.json 没有 backend_flavor 字段."""
        legacy_payload = {
            "id": "abc-123",
            "name": "legacy",
            "credentials": {
                "host": "example.com",
                "username": "ubuntu",
                "auth_method": "password",
                "port": 22,
            },
            "paths": {
                "workspace_dir": "$HOME/Napcat",
                "runtime_dir": "$HOME/Napcat/run",
                "config_dir": "$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/config",
                "log_dir": "$HOME/Napcat/log",
                "tmp_dir": "$HOME/Napcat/tmp",
                "package_dir": "$HOME/Napcat/packages",
            },
            "deployment_state": "deployed",
            # 故意不带 backend_flavor / snowluma_paths
        }
        recovered = ServerProfile.from_dict(legacy_payload)
        assert recovered.backend_flavor == BackendFlavor.NAPCAT
        assert recovered.snowluma_paths is None

    def test_unknown_flavor_falls_back_napcat(
        self, basic_credentials: SSHCredentials
    ) -> None:
        payload = ServerProfile.create(
            name="x", credentials=basic_credentials
        ).to_dict()
        payload["backend_flavor"] = "totally_unknown"
        recovered = ServerProfile.from_dict(payload)
        assert recovered.backend_flavor == BackendFlavor.NAPCAT

    def test_snowluma_flavor_missing_paths_uses_default(
        self, basic_credentials: SSHCredentials
    ) -> None:
        """SL flavor 但 payload 没 snowluma_paths → 用默认 $HOME/snowluma-remote."""
        payload = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
        ).to_dict()
        payload["snowluma_paths"] = None  # 模拟缺失
        recovered = ServerProfile.from_dict(payload)
        assert recovered.backend_flavor == BackendFlavor.SNOWLUMA
        assert recovered.snowluma_paths is not None
        assert recovered.snowluma_paths.base_dir == "$HOME/snowluma-remote"

    def test_invalid_snowluma_paths_payload_degrades(
        self, basic_credentials: SSHCredentials
    ) -> None:
        payload = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
        ).to_dict()
        # 注入非法 shell 元字符到 base_dir, 触发 P5 F2.3 校验失败
        payload["snowluma_paths"]["base_dir"] = "/opt/$(rm -rf /)"
        recovered = ServerProfile.from_dict(payload)
        # 降级到默认
        assert recovered.snowluma_paths is not None
        assert recovered.snowluma_paths.base_dir == "$HOME/snowluma-remote"

    def test_napcat_flavor_forces_snowluma_paths_none(
        self, basic_credentials: SSHCredentials
    ) -> None:
        """即使 payload 错误地给 NC flavor 同时塞了 snowluma_paths, 也强制 None."""
        payload = ServerProfile.create(
            name="np", credentials=basic_credentials
        ).to_dict()
        payload["backend_flavor"] = "napcat"
        # 误塞 SL paths
        payload["snowluma_paths"] = {
            "base_dir": "/opt/sl",
            "workspace_dir": "/opt/sl/workspace",
            "snowluma_framework_dir": "",
            "config_dir": "",
            "runtime_dir": "",
            "log_dir": "",
            "vnc_secret": "",
            "webui_secret": "",
            "daemon_launcher_script": "",
            "bot_launcher_script": "",
        }
        recovered = ServerProfile.from_dict(payload)
        assert recovered.snowluma_paths is None


# ==================== ServerRegistry 集成 ====================
class TestServerRegistry:
    def test_save_load_mixed_flavor(
        self,
        tmp_path: Path,
        basic_credentials: SSHCredentials,
    ) -> None:
        storage = tmp_path / "servers.json"
        registry = ServerRegistry(storage)

        np_profile = ServerProfile.create(name="np-server", credentials=basic_credentials)
        sl_profile = ServerProfile.create(
            name="sl-server",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
        )
        registry.add(np_profile)
        registry.add(sl_profile)

        # 重新加载
        registry2 = ServerRegistry(storage)
        loaded_np = registry2.get(np_profile.id)
        loaded_sl = registry2.get(sl_profile.id)
        assert loaded_np is not None
        assert loaded_sl is not None
        assert loaded_np.backend_flavor == BackendFlavor.NAPCAT
        assert loaded_sl.backend_flavor == BackendFlavor.SNOWLUMA
        assert loaded_sl.snowluma_paths is not None

    def test_json_disk_format(
        self, tmp_path: Path, basic_credentials: SSHCredentials
    ) -> None:
        storage = tmp_path / "servers.json"
        registry = ServerRegistry(storage)
        sl_profile = ServerProfile.create(
            name="sl",
            credentials=basic_credentials,
            backend_flavor=BackendFlavor.SNOWLUMA,
        )
        registry.add(sl_profile)

        disk = json.loads(storage.read_text(encoding="utf-8"))
        assert disk["schema_version"] == ServerRegistry.SCHEMA_VERSION
        assert len(disk["servers"]) == 1
        server_entry = disk["servers"][0]
        assert server_entry["backend_flavor"] == "snowluma"
        assert server_entry["snowluma_paths"] is not None
