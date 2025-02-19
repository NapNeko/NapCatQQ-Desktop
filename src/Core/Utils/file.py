# -*- coding: utf-8 -*-
"""
文件相关操作工具
"""


# 标准库导入
import json
from pathlib import Path

from PySide6.QtCore import QObject

# 项目内模块导入
from src.ui.common.info_bar import error_bar, success_bar
from src.core.utils.singleton import singleton


@singleton
class JsonFunc(QObject):

    def dict2json(self, data: dict, path: Path | str) -> None:
        """字典转 json 文件"""
        try:
            if not data or not path:
                return

            if isinstance(path, str):
                path = Path(path)

            path.parent.mkdir(parents=True, exist_ok=True)
            with open(str(path), "w", encoding="utf-8") as f:
                json.dump(data, f, ensure_ascii=False, indent=4)
            success_bar(self.tr("成功!"))

        except PermissionError:
            error_bar(self.tr("没有权限写入文件"))

    def json2dict(self, path: Path | str) -> dict:
        """json 文件转字典"""
        try:
            if not path:
                return

            if isinstance(path, str):
                path = Path(path)

            if not path.exists():
                return

            with open(str(path), "r", encoding="utf-8") as f:
                return json.load(f)

        except PermissionError:
            error_bar(self.tr("没有权限读取文件"))
            return {}
