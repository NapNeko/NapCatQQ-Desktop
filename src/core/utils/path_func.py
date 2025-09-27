# -*- coding: utf-8 -*-
# 标准库导入
import winreg
from pathlib import Path

# 项目内模块导入
from src.core.utils.logger import LogSource, LogType, logger
from src.core.utils.singleton import Singleton


class PathFunc(metaclass=Singleton):
    """路径处理类

    NapCatQQ Desktop 的路径处理类, 负责管理和验证应用程序所需的各种路径

    Attributes:
        qq_path (Path | None): QQ安装路径, 如果未找到则为None
        napcat_path (Path): NapCat目录路径
        config_dir_path (Path): 配置文件目录路径
        tmp_path (Path): 临时文件目录路径
        config_path (Path): 主配置文件路径
        bot_config_path (Path): 机器人配置文件路径
    """

    qq_path: Path | None
    napcat_path: Path
    config_dir_path: Path
    tmp_path: Path
    config_path: Path
    bot_config_path: Path

    def __init__(self) -> None:
        """初始化"""

        # 路径字段
        self.qq_path = None
        self.base_path = Path.cwd()
        self.napcat_path = self.base_path / "NapCat"
        self.config_dir_path = self.base_path / "config"
        self.tmp_path = self.base_path / "tmp"

        # 文件字段
        self.config_path = self.config_dir_path / "config.json"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.napcat_config_path = self.napcat_path / "config.json"

    def path_validator(self) -> None:
        """验证一系列路径"""

        paths_to_validate = [(self.tmp_path, "Tmp"), (self.config_dir_path, "config"), (self.napcat_path, "NapCat")]

        for path, name in paths_to_validate:
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)
                logger.info(f"创建路径 {name.center(8)} 成功", LogType.FILE_FUNC, LogSource.CORE)
            else:
                logger.info(f"路径 {name.center(8)} 已存在", LogType.FILE_FUNC, LogSource.CORE)

    @staticmethod
    def get_qq_path() -> Path | None:
        """获取QQ路径"""
        try:
            key = winreg.OpenKey(
                key=winreg.HKEY_LOCAL_MACHINE,
                sub_key=r"SOFTWARE\WOW6432Node\Tencent\QQNT",
            )
            return Path(winreg.QueryValueEx(key, "Install")[0])
        except FileNotFoundError:
            return None
