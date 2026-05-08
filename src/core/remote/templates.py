# -*- coding: utf-8 -*-
"""远端部署脚本模板. 

P1 拆分后, Desktop 侧上传给远端的脚本有 3 份:

| 脚本 | 用途 | 构建器 |
| --- | --- | --- |
| `remote_install_linuxqq.sh` | LinuxQQ rootless 安装 | [`build_install_linuxqq_script`](src/core/remote/templates.py) |
| `remote_install_napcat.sh` | NapCat 注入 + launcher 部署 | [`build_install_napcat_script`](src/core/remote/templates.py) |
| `remote_napcat_launcher.sh` | 启停/状态查询 (P2 调用) | [`build_napcat_launcher_script`](src/core/remote/templates.py) |

每个构建器都会读取对应模板, 在脚本头部注入路径变量, 返回完整的可执行脚本文本. 

历史脚本 `remote_deploy_napcat.sh` 已被拆分替代, 仅在历史归档中保留作为参考. 
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
    """在脚本头部注入变量定义. 

    始终在第一行 ``#!`` shebang 之后插入变量赋值, 保持 ``set -euo pipefail`` 等设置完好. 

    P5 安全收尾 F2.1 - 单引号注入语法
    ------------------------------------
    历史实现把值塞在双引号里, 仅转义 ``"``: 这导致 ``$()`` / 反引号 / ``$VAR``
    仍被远端 bash 二次展开, 用户可写入的 [`LinuxCorePaths.workspace_dir`]
    (src/core/remote/models.py) 字段成了命令注入入口.

    新语法采用 **POSIX 单引号字面量**, 单引号内一切字面;
    单引号自身用 ``'\\''`` 模式插入(关闭->转义->重开). 同时保留远端 ``$HOME``
    展开能力: 当值以 ``$HOME`` 起头时, 拆成 ``"$HOME"`` (双引号让 bash 展开)
    + 余下后缀 (单引号字面量), 用 bash 字符串拼接组装.

    结果对比:

    - 旧: ``workspace_dir="$HOME/Napcat$(rm)"`` -> 远端会执行 ``rm`` (漏洞)
    - 新: ``workspace_dir="$HOME"'/Napcat$(rm)'`` -> 仅 $HOME 展开, ``$(rm)`` 字面保留
    """
    injected_lines = [f"{key}={_safe_shell_value(str(value))}" for key, value in variables.items()]
    lines = script_content.splitlines()
    if lines and lines[0].startswith("#!"):
        return "\n".join([lines[0], *injected_lines, *lines[1:]]) + "\n"
    return "\n".join([*injected_lines, *lines]) + "\n"


def _safe_shell_value(value: str) -> str:
    """渲染单个变量值为 bash 安全的右值表达式 (P5 F2.1).

    - ``$HOME`` / ``$HOME/...`` 起头: 拆成 ``"$HOME"`` + 单引号包后缀
    - 其他: 整体单引号包裹
    - 单引号字符通过 ``'\\''`` 闭合-转义-重开模式嵌入
    """
    if value == "$HOME":
        return '"$HOME"'
    if value.startswith("$HOME/"):
        suffix = value[len("$HOME") :]
        return '"$HOME"' + _single_quote(suffix)
    return _single_quote(value)


def _single_quote(value: str) -> str:
    """POSIX 单引号字面量, 单引号自身用 ``'\\''`` 关闭-转义-重开."""
    return "'" + value.replace("'", "'\\''") + "'"


# ==================== 构建器 ====================
def build_install_linuxqq_script(variables: Mapping[str, str | int]) -> str:
    """构建 LinuxQQ 安装脚本(注入路径变量后). """
    return inject_script_variables(_read_template(INSTALL_LINUXQQ_SCRIPT_FILENAME), variables)


def build_install_napcat_script(variables: Mapping[str, str | int]) -> str:
    """构建 NapCat 安装脚本(注入路径变量后). """
    return inject_script_variables(_read_template(INSTALL_NAPCAT_SCRIPT_FILENAME), variables)


def build_napcat_launcher_script(variables: Mapping[str, str | int]) -> str:
    """构建 NapCat launcher 脚本(注入路径变量后). """
    return inject_script_variables(_read_template(NAPCAT_LAUNCHER_SCRIPT_FILENAME), variables)


# ==================== 历史一站式脚本 (保留兼容入口)  ====================
def load_linux_deploy_script() -> str:
    """读取历史 Linux 远端部署脚本模板. 

    .. deprecated:: P1
        请改用 [`build_install_linuxqq_script`](src/core/remote/templates.py)
        与 [`build_install_napcat_script`](src/core/remote/templates.py). 
    """
    return _read_template(LINUX_DEPLOY_SCRIPT_FILENAME)


def build_linux_deploy_script(variables: Mapping[str, str | int]) -> str:
    """构建历史一站式部署脚本. 

    .. deprecated:: P1
        见 [`load_linux_deploy_script`](src/core/remote/templates.py). 
    """
    return inject_script_variables(load_linux_deploy_script(), variables)
