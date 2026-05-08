# -*- coding: utf-8 -*-
"""本地 SSH 密钥扫描与生成辅助. 

- [`scan_local_ssh_keys`] 仅做 UI 候选项扫描, **不会自动用于建立连接**(§6.2 安全基线).
- [`ensure_local_keypair`] 用于 "密码登录后自动配置免密" 场景: 复用或生成
  ``~/.ssh/id_ed25519``, 行为对齐 ``ssh-copy-id``. 私钥 0o600, ``~/.ssh`` 目录 0o700.

独立于 UI 层, 便于单元测试与脚本复用. 
"""

from __future__ import annotations

import os
import socket
from pathlib import Path

# OpenSSH 客户端默认私钥文件名(不含 .pub 公钥), 按现代算法优先级排序
_STANDARD_SSH_KEY_NAMES: tuple[str, ...] = (
    "id_ed25519",
    "id_ecdsa",
    "id_rsa",
    "id_dsa",
)

# 自动生成的密钥文件名, 与 OpenSSH 客户端默认一致, 便于 ``ssh user@host`` 直接复用
_DEFAULT_KEY_NAME: str = "id_ed25519"


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


def _key_comment() -> str:
    """生成 OpenSSH 公钥末尾的注释段, 形如 ``napcat-desktop@<hostname>``.

    主机名抓不到时退回 ``napcat-desktop`` 字面量, 避免空注释或异常.
    """
    try:
        host = socket.gethostname() or ""
    except Exception:  # noqa: BLE001 - 任何环境异常都退回安全默认
        host = ""
    host = host.strip() or "host"
    return f"napcat-desktop@{host}"


def _chmod_silent(path: Path, mode: int) -> None:
    """``os.chmod`` 在 Windows 上对 POSIX 模式位语义有限, 失败容忍."""
    try:
        os.chmod(path, mode)
    except OSError:
        pass


def ensure_local_keypair() -> tuple[Path, str]:
    """确保本地存在 ed25519 密钥对(OpenSSH 格式), 返回私钥路径与公钥单行字符串.

    行为细节(对齐 ``ssh-copy-id`` 心智模型):
    - 复用 ``~/.ssh/id_ed25519``; 该文件存在时**不重新生成**, 公钥从 ``id_ed25519.pub``
      读取(缺失则从私钥派生并补写 .pub).
    - 不存在时生成新密钥对; 私钥 OpenSSH PEM 容器, 无密码; 公钥单行 ``ssh-ed25519
      <base64> napcat-desktop@<hostname>``.
    - 目录 ``~/.ssh`` 自动创建并归一权限 0o700; 私钥 0o600; 公钥 0o644.
    - 私钥写入采用 ``临时文件 + os.replace`` 原子语义, 避免半成品.

    Returns:
        ``(私钥绝对路径, OpenSSH 单行公钥字符串)``

    Raises:
        OSError: 文件系统不可写
        ImportError: 运行环境缺少 ``cryptography``(理论上不会发生, 已在依赖清单)
    """
    # 局部 import: 避免在仅做候选项扫描时也强制加载 cryptography(虽然它本是必装项).
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric import ed25519

    ssh_dir = Path.home() / ".ssh"
    ssh_dir.mkdir(mode=0o700, exist_ok=True)
    _chmod_silent(ssh_dir, 0o700)

    priv_path = ssh_dir / _DEFAULT_KEY_NAME
    pub_path = ssh_dir / f"{_DEFAULT_KEY_NAME}.pub"
    comment = _key_comment()

    # 复用既有私钥
    if priv_path.is_file():
        if pub_path.is_file():
            existing = pub_path.read_text(encoding="utf-8").strip()
            if existing:
                return priv_path, existing
        # 公钥文件丢失或为空 -> 从私钥派生并补写
        with priv_path.open("rb") as fp:
            data = fp.read()
        priv_key = serialization.load_ssh_private_key(data, password=None)
        public_bytes = priv_key.public_key().public_bytes(
            encoding=serialization.Encoding.OpenSSH,
            format=serialization.PublicFormat.OpenSSH,
        )
        pub_line = f"{public_bytes.decode('ascii')} {comment}"
        pub_path.write_text(pub_line + "\n", encoding="utf-8")
        _chmod_silent(pub_path, 0o644)
        return priv_path, pub_line

    # 新建密钥对
    priv_key = ed25519.Ed25519PrivateKey.generate()
    private_bytes = priv_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.OpenSSH,
        encryption_algorithm=serialization.NoEncryption(),
    )
    public_bytes = priv_key.public_key().public_bytes(
        encoding=serialization.Encoding.OpenSSH,
        format=serialization.PublicFormat.OpenSSH,
    )
    pub_line = f"{public_bytes.decode('ascii')} {comment}"

    # 私钥原子落盘
    tmp_priv = priv_path.with_name(priv_path.name + ".tmp")
    tmp_priv.write_bytes(private_bytes)
    _chmod_silent(tmp_priv, 0o600)
    os.replace(tmp_priv, priv_path)
    _chmod_silent(priv_path, 0o600)

    pub_path.write_text(pub_line + "\n", encoding="utf-8")
    _chmod_silent(pub_path, 0o644)

    return priv_path, pub_line


__all__ = ("scan_local_ssh_keys", "ensure_local_keypair")
