# -*- coding: utf-8 -*-
"""Desktop 更新脚本辅助函数。"""

from collections.abc import Mapping


def inject_script_variables(script_content: str, variables: Mapping[str, str | int]) -> str:
    """在 `setlocal enabledelayedexpansion` 后注入脚本变量。"""

    script_lines = script_content.splitlines()
    injected_lines = [f'set "{key}={value}"' for key, value in variables.items()]

    for index, line in enumerate(script_lines):
        if line.strip().lower() == "setlocal enabledelayedexpansion":
            script_lines[index + 1 : index + 1] = injected_lines
            break
    else:
        script_lines[0:0] = injected_lines

    return "\n".join(script_lines) + "\n"


def inject_target_pid(script_content: str, target_pid: int) -> str:
    """为更新脚本注入目标进程 PID。"""

    return inject_script_variables(script_content, {"target_pid": target_pid})
