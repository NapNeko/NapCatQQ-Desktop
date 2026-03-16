# coding:utf-8

# 标准库导入
from pathlib import Path
from typing import Self

# 第三方库导入
from PySide6.QtCore import QFile


class QFluentFile(QFile):
    """支持使用 with 语法的 QFile (明确仅对读取操作进行了测试)"""

    def __init__(self, file: str | QFile | Path, mode: QFile.OpenModeFlag) -> None:
        super().__init__(file if isinstance(file, QFile) else str(file))
        self._mode: QFile.OpenModeFlag = mode

    def __enter__(self) -> Self:
        """打开文件"""
        if not self.open(self._mode):
            raise IOError(self.errorString())
        return self

    def __exit__(self, exc_type: type | None, exc_val: BaseException | None, exc_tb: object | None) -> bool:
        """关闭文件"""
        self.close()
        return False
