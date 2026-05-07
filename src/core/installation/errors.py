# -*- coding: utf-8 -*-
"""安装域异常 (P5 安全收尾 F1.2).

`NapCatHashMismatchError` 在本地与远端两条安装路径都会被抛出, 故放在 installation
共享层, 不再放到 ``src/core/remote/errors.py`` 避免污染远程层.
"""
from __future__ import annotations


class InstallationError(RuntimeError):
    """安装域基础异常."""


class NapCatHashMismatchError(InstallationError):
    """NapCat 安装包 SHA512 校验失败.

    Attributes:
        version: 期望版本号 (不含 ``v`` 前缀)
        expected: 期望 SHA512 (128 位 hex 小写)
        actual: 实际计算所得 SHA512
        archive_path: 触发校验的本地或远端 archive 路径 (用于日志/诊断)
    """

    def __init__(
        self,
        version: str,
        expected: str,
        actual: str,
        archive_path: str,
    ) -> None:
        self.version = version
        self.expected = expected
        self.actual = actual
        self.archive_path = archive_path
        super().__init__(
            f"NapCat 安装包完整性校验失败 (version={version}, archive={archive_path})"
        )


__all__: tuple[str, ...] = (
    "InstallationError",
    "NapCatHashMismatchError",
)
