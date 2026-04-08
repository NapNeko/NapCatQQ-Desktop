# -*- coding: utf-8 -*-
"""远端部署脚本模板。"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[2] / "resource" / "script"
LINUX_DEPLOY_SCRIPT_FILENAME = "remote_deploy_napcat.sh"


def load_linux_deploy_script() -> str:
    """读取 Linux 远端部署脚本模板。"""
    return (SCRIPT_DIR / LINUX_DEPLOY_SCRIPT_FILENAME).read_text(encoding="utf-8")


def inject_script_variables(script_content: str, variables: Mapping[str, str | int]) -> str:
    """在脚本头部注入变量定义。"""
    injected_lines = [f'{key}="{str(value).replace(chr(34), r"\"")}"' for key, value in variables.items()]
    lines = script_content.splitlines()
    if lines and lines[0].startswith("#!"):
        return "\n".join([lines[0], *injected_lines, *lines[1:]]) + "\n"
    return "\n".join([*injected_lines, *lines]) + "\n"


def build_linux_deploy_script(variables: Mapping[str, str | int]) -> str:
    """构建带变量注入的 Linux 部署脚本。"""
    return inject_script_variables(load_linux_deploy_script(), variables)
