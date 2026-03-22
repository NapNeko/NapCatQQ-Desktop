# -*- coding: utf-8 -*-
"""Desktop 更新脚本辅助函数。"""

import httpx

from src.core.network.urls import Urls


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


def fetch_remote_update_script(script_url: str) -> str:
    """下载远端 updater 脚本，失败时自动尝试镜像。"""

    errors: list[str] = []
    for candidate_url in _iter_update_script_urls(script_url):
        try:
            with httpx.Client(timeout=8, follow_redirects=True) as client:
                response = client.get(candidate_url)
                response.raise_for_status()
                script_content = response.text.strip()
                if not script_content:
                    raise ValueError("脚本内容为空")
                return script_content + "\n"
        except (httpx.RequestError, httpx.HTTPStatusError, ValueError) as exc:
            errors.append(f"{candidate_url}: {exc}")

    raise ValueError("无法获取远端更新脚本: " + " | ".join(errors))


def _iter_update_script_urls(script_url: str) -> list[str]:
    """生成远端更新脚本候选地址列表。"""

    candidate_urls = [script_url]
    candidate_urls.extend(f"{mirror.toString().rstrip('/')}/{script_url}" for mirror in Urls.MIRROR_SITE.value)
    return candidate_urls
