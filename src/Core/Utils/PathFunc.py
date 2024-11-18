# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

# 项目内模块导入
from src.Core.Utils.singleton import Singleton


class PathFunc(metaclass=Singleton):

    def __init__(self):
        """
        ## 初始化
        """
        self.qq_path = None
        self.base_path = Path.cwd()
        self.napcat_path = self.base_path / "NapCat"
        self.config_dir_path = self.base_path / "config"
        self.config_path = self.config_dir_path / "config.json"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.tmp_path = self.base_path / "tmp"

        self.log_path = self.base_path / "log"
        self.log_info_path = self.log_path / "info"
        self.log_debug_path = self.log_path / "debug"
        self.log_warn_path = self.log_path / "warn"
        self.log_error_path = self.log_path / "error"

        self.pathValidator()

    def pathValidator(self) -> None:
        """
        ## 验证一系列路径
        """

        paths_to_validate = [
            (self.config_dir_path, "Config"),
            (self.tmp_path, "Tmp"),
            (self.napcat_path, "NapCat"),
            (self.log_path, "Log"),
            (self.log_info_path, "Log Info"),
            (self.log_debug_path, "Log Debug"),
            (self.log_warn_path, "Log Warn"),
            (self.log_error_path, "Log Error"),
        ]

        for path, name in paths_to_validate:
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)

    @staticmethod
    def getQQPath() -> Path | None:
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
