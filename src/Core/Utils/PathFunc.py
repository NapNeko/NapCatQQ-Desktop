# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

# 项目内模块导入
from src.Core.Utils.logger import LogType, LogSource, logger
from src.Core.Utils.singleton import Singleton


class PathFunc(metaclass=Singleton):

    def __init__(self):
        """
        ## 初始化
        """

        # 路径字段
        self.qq_path = None
        self.base_path = Path.cwd()
        self.napcat_path = self.base_path / "NapCat"
        self.config_dir_path = self.base_path / "config"
        self.tmp_path = self.base_path / "tmp"

        # 文件字段
        self.config_path = self.config_dir_path / "config.json"
        self.bot_config_path = self.config_dir_path / "bot.json"

    def path_validator(self) -> None:
        """
        ## 验证一系列路径
        """

        paths_to_validate = [
            (self.config_dir_path, "Config"),
            (self.tmp_path, "Tmp"),
            (self.napcat_path, "NapCat")
        ]

        for path, name in paths_to_validate:
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)
                logger.info(f"创建路径 {name} 成功", LogType.FILE_FUNC, LogSource.CORE)
            else:
                logger.info(f"路径 {name} 已存在", LogType.FILE_FUNC, LogSource.CORE)

    @staticmethod
    def get_qq_path() -> Path | None:
        """
        获取QQ路径
        """
        try:
            key = winreg.OpenKey(
                key=winreg.HKEY_LOCAL_MACHINE,
                sub_key=r"SOFTWARE\WOW6432Node\Tencent\QQNT",
            )
            return Path(winreg.QueryValueEx(key, "Install")[0])
        except FileNotFoundError:
            return None
