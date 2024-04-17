# -*- coding: utf-8 -*-
import winreg
from abc import ABC
from pathlib import Path

from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from loguru import logger

from PySide6.QtCore import Qt
from qfluentwidgets.components import InfoBar


class PathFunc:

    def __init__(self):
        """
        初始化
        """
        # 软件路径
        self.base_path = Path.cwd()
        self.config_dir_path = self.base_path / "config"
        self.config_path = self.config_dir_path / "config.json"
        self.tmp_path = self.base_path / "tmp"

        self.path_velidator()

    def path_velidator(self) -> None:
        """
        验证路径
        """
        logger.info(f"{'-' * 10}开始验证路径{'-' * 10}")

        if not self.config_dir_path.exists():
            self.config_dir_path.mkdir(parents=True, exist_ok=True)
            logger.success("Config Path 路径不存在, 已创建")
        elif not self.config_dir_path.is_dir():
            self.config_dir_path.mkdir(parents=True, exist_ok=True)
            logger.warning("存在一个名为 config 的文件, 请检查")
        logger.success("Config Path 验证完成")

        if not self.tmp_path.exists():
            self.tmp_path.mkdir(parents=True, exist_ok=True)
            logger.success("Tmp Path 路径不存在, 已创建")
        elif not self.tmp_path.is_dir():
            self.tmp_path.mkdir(parents=True, exist_ok=True)
            logger.warning("存在一个名为 tmp 的文件, 请检查")
        logger.success("Tmp Path 验证完成")

        logger.success(f"{'-' * 10}路径验证完成{'-' * 10}")

    def get_qq_path(self) -> Path:
        """
        获取QQ路径
        :return: Path
        """
        from src.core.config import cfg
        try:
            if Path(cfg.get(cfg.qq_path)) == Path.cwd():
                key = winreg.OpenKey(
                    key=winreg.HKEY_LOCAL_MACHINE,
                    sub_key=r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\QQ"
                )
                self.qq_path = Path(winreg.QueryValueEx(key, "UninstallString")[0]).parent
                cfg.set(item=cfg.qq_path, value=str(path), save=True)
                return self.qq_path
            else:
                self.qq_path = cfg.get(cfg.qq_path)
                return self.qq_path
        except FileNotFoundError:
            return False


class PathFuncClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.core.path_func", "PathFunc"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.core.path_func")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: [PathFunc]) -> PathFunc:
        return PathFunc()


add_creator(PathFuncClassCreator)
