# -*- coding: utf-8 -*-
"""``remote_workspace_dir`` 配置校验集成测试 (P5 安全收尾 F2.4).

覆盖三层校验链:
1. ``is_valid_linux_path`` 公共 API (UI 同源校验入口)
2. ``_LinuxPathValidator`` 注入到 ``cfg.remote_workspace_dir`` 后, ``cfg.set``
   传非法值时 qfluentwidgets 会调 ``correct`` 回退到默认 ``$HOME/Napcat``,
   防止恶意 payload 流到 ``LinuxCorePaths``.
3. ``Config.linux_core_paths()`` 即使 ``servers.json`` / 配置文件被改也能拿到
   合法的 ``LinuxCorePaths`` (W2.3 兜底).

UI 层显式 ``error_bar`` 拦截不在本文件覆盖, 由 ``remote.py:_on_save`` 集成测试
(若有) 与手工抽测验证.
"""
from __future__ import annotations

# 第三方库导入
import pytest


# ==================== is_valid_linux_path 直查 ====================
@pytest.mark.parametrize(
    "value",
    [
        "$HOME/Napcat",
        "$HOME/Napcat/run",
        "$HOME/Napcat/foo-bar_v1.2",
        "/opt/napcat",
        "/var/lib/napcat-1.0",
    ],
)
def test_is_valid_linux_path_accepts_valid(value: str) -> None:
    from src.core.remote.models import is_valid_linux_path

    assert is_valid_linux_path(value) is True


@pytest.mark.parametrize(
    "value",
    [
        "",
        "napcat",  # 相对
        "$HOME/$(rm)",
        "$HOME/Napcat$(touch)",
        "$HOME/`whoami`",
        "/opt;rm",
        "/opt|cat",
        "$HOME/$USER",
        "$HOME\nrm",
    ],
)
def test_is_valid_linux_path_rejects_invalid(value: str) -> None:
    from src.core.remote.models import is_valid_linux_path

    assert is_valid_linux_path(value) is False


def test_is_valid_linux_path_rejects_non_string() -> None:
    from src.core.remote.models import is_valid_linux_path

    assert is_valid_linux_path(None) is False
    assert is_valid_linux_path(123) is False
    assert is_valid_linux_path([]) is False


# ==================== Validator 集成: correct 回退 ====================
def test_linux_path_validator_correct_returns_default_for_invalid() -> None:
    """qfluentwidgets ``Config`` 在 ``set`` 非法值时会通过 ``correct`` 回退到默认.

    此测试验证 ``_LinuxPathValidator.correct`` 行为正确; 不直接走 ``cfg.set``
    避免污染全局单例.
    """
    from src.core.config import _LinuxPathValidator

    validator = _LinuxPathValidator()
    # 非法 -> 默认
    assert validator.correct("$HOME/$(rm)") == "$HOME/Napcat"
    assert validator.correct("/opt;rm") == "$HOME/Napcat"
    assert validator.correct("") == "$HOME/Napcat"
    # 合法 -> 原样
    assert validator.correct("$HOME/Napcat") == "$HOME/Napcat"
    assert validator.correct("/opt/napcat") == "/opt/napcat"


def test_linux_path_validator_validate_matches_is_valid_linux_path() -> None:
    """``Validator.validate`` 行为必须与 ``is_valid_linux_path`` 完全一致."""
    from src.core.config import _LinuxPathValidator
    from src.core.remote.models import is_valid_linux_path

    validator = _LinuxPathValidator()
    test_cases = [
        "$HOME/Napcat",
        "$HOME/$(rm)",
        "/opt/napcat",
        "/opt;rm",
        "",
        "napcat",
    ]
    for value in test_cases:
        assert validator.validate(value) == is_valid_linux_path(value), f"分歧 value={value!r}"


# ==================== LinuxCorePaths 兜底 ====================
def test_servers_json_with_invalid_path_falls_back_to_default(tmp_path) -> None:
    """``servers.json`` 含非法 ``workspace_dir`` 时, ``ServerProfile.from_dict``
    应退化到默认 ``LinuxCorePaths()``, 不阻断 Desktop 启动 (W2.3 容错)."""
    import json

    from src.core.remote.servers import ServerProfile

    payload = {
        "id": "abc",
        "name": "test",
        "credentials": {
            "host": "example.com",
            "port": 22,
            "username": "root",
            "auth_method": "key",
            "private_key_path": "/some/key",
        },
        "paths": {
            "workspace_dir": "$HOME/$(curl evil)",  # 非法
            "runtime_dir": "$HOME/Napcat/run",
            "config_dir": "$HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/config",
            "log_dir": "$HOME/Napcat/log",
            "tmp_dir": "$HOME/Napcat/tmp",
            "package_dir": "$HOME/Napcat/packages",
        },
    }
    profile = ServerProfile.from_dict(payload)

    # 非法路径不应保留, 应退化为默认
    assert profile.paths.workspace_dir == "$HOME/Napcat"
