# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

# 项目内模块导入
from src.core.utils.logger import LogType, LogSource, logger
from src.core.utils.singleton import Singleton


class PathFunc(metaclass=Singleton):

    def __init__(self):

        # QQ路径(未定义,从注册表中获取)
        self.qq_path = None

        # NCD运行所用到的路径
        self.base_path = Path.cwd() / ".NapCat Desktop"
        self.napcat_path = self.base_path / "NapCat"
        self.config_dir_path = self.base_path / "config"
        self.tmp_path = self.base_path / "tmp"

        # 文件路径
        self.config_path = self.config_dir_path / "config.json"

    def path_validator(self) -> None:
        """验证路径是否存在"""
        for path in [path for path in self.__dict__.values() if isinstance(path, Path)]:
            # 遍历属性提取出 Path 进行验证
            if path.suffix:
                # 此时有文件后缀,认为是文件,则跳过
                continue
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)
                logger.info(f"创建路径{path.name.center(12)}成功", LogType.FILE_FUNC, LogSource.CORE)
            else:
                logger.info(f"路径{path.name.center(12)}已存在", LogType.FILE_FUNC, LogSource.CORE)

    def get_qq_path(self) -> Path | None:
        """读取注册表获取QQ路径"""
        try:
            key = winreg.OpenKey(key=winreg.HKEY_LOCAL_MACHINE, sub_key=r"SOFTWARE\WOW6432Node\Tencent\QQNT")
            self.qq_path = Path(winreg.QueryValueEx(key, "InstallPath")[0])
            return self.qq_path
        except FileNotFoundError:
            return None


__all__ = ["PathFunc"]
