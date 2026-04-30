# -*- coding: utf-8 -*-
"""远端部署脚本模板。

P1 拆分后, Desktop 侧上传给远端的脚本有 3 份:

| 脚本 | 用途 | 构建器 |
| --- | --- | --- |
| `remote_install_linuxqq.sh` | LinuxQQ rootless 安装 | [`build_install_linuxqq_script`](src/core/remote/templates.py) |
| `remote_install_napcat.sh` | NapCat 注入 + launcher 部署 | [`build_install_napcat_script`](src/core/remote/templates.py) |
| `remote_napcat_launcher.sh` | 启停/状态查询 (P2 调用) | [`build_napcat_launcher_script`](src/core/remote/templates.py) |

每个构建器都会读取对应模板, 在脚本头部注入路径变量, 返回完整的可执行脚本文本。

历史脚本 `remote_deploy_napcat.sh` 已被拆分替代, 仅在历史归档中保留作为参考。
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[2] / "resource" / "script"

INSTALL_LINUXQQ_SCRIPT_FILENAME = "remote_install_linuxqq.sh"
INSTALL_NAPCAT_SCRIPT_FILENAME = "remote_install_napcat.sh"
NAPCAT_LAUNCHER_SCRIPT_FILENAME = "remote_napcat_launcher.sh"

# 旧版一站式脚本, 仅保留为兼容入口
LINUX_DEPLOY_SCRIPT_FILENAME = "remote_deploy_napcat.sh"


def _read_template(filename: str) -> str:
    return (SCRIPT_DIR / filename).read_text(encoding="utf-8")


def inject_script_variables(script_content: str, variables: Mapping[str, str | int]) -> str:
    """在脚本头部注入变量定义。

    始终在第一行 ``#!`` shebang 之后插入变量赋值, 保持 ``set -euo pipefail`` 等设置完好。
    """
    injected_lines = [f'{key}="{str(value).replace(chr(34), r"\"")}"' for key, value in variables.items()]
    lines = script_content.splitlines()
    if lines and lines[0].startswith("#!"):
        return "\n".join([lines[0], *injected_lines, *lines[1:]]) + "\n"
    return "\n".join([*injected_lines, *lines]) + "\n"


# ==================== 构建器 ====================
def build_install_linuxqq_script(variables: Mapping[str, str | int]) -> str:
    """构建 LinuxQQ 安装脚本(注入路径变量后)。"""
    return inject_script_variables(_read_template(INSTALL_LINUXQQ_SCRIPT_FILENAME), variables)


def build_install_napcat_script(variables: Mapping[str, str | int]) -> str:
    """构建 NapCat 安装脚本(注入路径变量后)。"""
    return inject_script_variables(_read_template(INSTALL_NAPCAT_SCRIPT_FILENAME), variables)


def build_napcat_launcher_script(variables: Mapping[str, str | int]) -> str:
    """构建 NapCat launcher 脚本(注入路径变量后)。"""
    return inject_script_variables(_read_template(NAPCAT_LAUNCHER_SCRIPT_FILENAME), variables)


# ==================== 历史一站式脚本（保留兼容入口） ====================
def load_linux_deploy_script() -> str:
    """读取历史 Linux 远端部署脚本模板。

    .. deprecated:: P1
        请改用 [`build_install_linuxqq_script`](src/core/remote/templates.py)
        与 [`build_install_napcat_script`](src/core/remote/templates.py)。
    """
    return _read_template(LINUX_DEPLOY_SCRIPT_FILENAME)


def build_linux_deploy_script(variables: Mapping[str, str | int]) -> str:
    """构建历史一站式部署脚本。

    .. deprecated:: P1
        见 [`load_linux_deploy_script`](src/core/remote/templates.py)。
    """
    return inject_script_variables(load_linux_deploy_script(), variables)
