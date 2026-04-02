# -*- coding: utf-8 -*-
"""Desktop 更新脚本辅助函数。"""


def inject_target_pid(script_content: str, target_pid: int) -> str:
    """为更新脚本注入目标进程 PID。"""

    target_pid_line = f'set "target_pid={target_pid}"'
    script_lines = script_content.splitlines()

    for index, line in enumerate(script_lines):
        if line.strip().lower() == "setlocal enabledelayedexpansion":
            script_lines.insert(index + 1, target_pid_line)
            break
    else:
        script_lines.insert(0, target_pid_line)

    return "\n".join(script_lines) + "\n"
