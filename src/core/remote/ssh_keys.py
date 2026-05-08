# -*- coding: utf-8 -*-
"""本地 SSH 密钥扫描辅助. 

仅作为 UI 候选项使用, **不会自动用于建立连接**(参考计划 §6.2 安全基线). 
独立于 UI 层, 便于单元测试与脚本复用. 
"""

from __future__ import annotations

from pathlib import Path

# OpenSSH 客户端默认私钥文件名(不含 .pub 公钥), 按现代算法优先级排序
_STANDARD_SSH_KEY_NAMES: tuple[str, ...] = (
    "id_ed25519",
    "id_ecdsa",
    "id_rsa",
    "id_dsa",
)


def scan_local_ssh_keys() -> list[str]:
    """扫描用户 ``~/.ssh/`` 下标准命名的私钥, 返回绝对路径列表(已存在的文件). 

    按现代算法优先级排序: ed25519 > ecdsa > rsa > dsa. 
    """
    ssh_dir = Path.home() / ".ssh"
    if not ssh_dir.is_dir():
        return []
    found: list[str] = []
    for name in _STANDARD_SSH_KEY_NAMES:
        candidate = ssh_dir / name
        if candidate.is_file():
            found.append(str(candidate))
    return found
