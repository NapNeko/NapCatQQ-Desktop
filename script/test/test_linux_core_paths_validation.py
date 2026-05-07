# -*- coding: utf-8 -*-
"""[`LinuxCorePaths.__post_init__`](src/core/remote/models.py) 严格白名单校验
单测 (P5 安全收尾 F2.3).

防御目的: 即使攻击者通过修改 ``servers.json`` / ``config.json`` 绕过 UI 校验直接
注入恶意 ``workspace_dir`` 等字段, 模型加载阶段 ``ValueError`` 会立刻拒绝它,
不让恶意值流到 ``inject_script_variables`` / ``_quote_remote_argument``.
"""
from __future__ import annotations

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.remote.models import LinuxCorePaths


# ==================== 合法值 ====================
def test_default_paths_are_accepted() -> None:
    """默认值必须能构造成功."""
    LinuxCorePaths()  # 不抛即通过


@pytest.mark.parametrize(
    "value",
    [
        "$HOME/Napcat",
        "$HOME/Napcat/run",
        "$HOME/foo-bar_v1.2",
        "/opt/napcat",
        "/opt/napcat/runtime",
        "/var/lib/napcat-1.0",
    ],
)
def test_valid_paths_are_accepted(value: str) -> None:
    """常见合法路径都应被接受."""
    LinuxCorePaths(workspace_dir=value)  # workspace_dir 是关键, 其他字段默认值 OK


# ==================== 非法值 (注入攻击) ====================
@pytest.mark.parametrize(
    "value",
    [
        "$HOME/$(whoami)",          # 命令替换
        "$HOME/Napcat$(rm -rf /)",  # 嵌入命令替换
        "$HOME/`whoami`",           # 反引号替换
        "/opt;rm -rf /",            # ; 分隔符
        "/opt/napcat&true",         # & 后台
        "/opt/napcat|cat",          # | 管道
        '$HOME"x',                  # 双引号
        "$HOME/Napcat>$(date)",     # 重定向 + 命令替换
        "$HOME/Nap\\cat",           # 反斜杠
        "$HOME\nrm -rf /",          # 换行注入
        "$HOMEX/foo",               # 不是 $HOME 而是 $HOMEX (含 $ 符号)
        "$HOME$USER/foo",           # 后跟 $USER
    ],
)
def test_malicious_paths_are_rejected(value: str) -> None:
    """含 shell 元字符的路径必须拒绝."""
    with pytest.raises(ValueError):
        LinuxCorePaths(workspace_dir=value)


# ==================== 空值 / 边界 ====================
def test_empty_workspace_is_rejected() -> None:
    with pytest.raises(ValueError):
        LinuxCorePaths(workspace_dir="")


def test_relative_path_is_rejected() -> None:
    """禁止相对路径 (不以 ``/`` 或 ``$HOME`` 起头), 防止远端路径解析歧义."""
    with pytest.raises(ValueError):
        LinuxCorePaths(workspace_dir="napcat")


# ==================== 多字段同时校验 ====================
def test_each_path_field_is_validated() -> None:
    """所有路径字段都要走校验, 不光 workspace_dir."""
    with pytest.raises(ValueError):
        LinuxCorePaths(runtime_dir="$HOME/$(rm)")
    with pytest.raises(ValueError):
        LinuxCorePaths(config_dir="/opt/napcat;rm")
    with pytest.raises(ValueError):
        LinuxCorePaths(log_dir="$HOME/`id`")
    with pytest.raises(ValueError):
        LinuxCorePaths(tmp_dir="$HOME|cat")
    with pytest.raises(ValueError):
        LinuxCorePaths(package_dir="/opt/$(curl evil)")
