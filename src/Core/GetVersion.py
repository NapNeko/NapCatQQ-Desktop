# -*- coding: utf-8 -*-
import json
from abc import ABC
from typing import Optional

from creart import it, AbstractCreator, CreateTargetInfo, exists_module, add_creator

from src.Core.PathFunc import PathFunc


class GetVersion:
    """
    ## 提供两个方法, 分别获取本地的 NapCat 和 QQ 的版本
    """

    @staticmethod
    def getNapCatVersion():
        """
        ## 获取 NapCat 的版本信息
        """
        try:
            # 获取 package.json 路径并读取
            package_file_path = it(PathFunc).getNapCatPath() / "package.json"
            with open(str(package_file_path), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"

        except FileNotFoundError:
            # 文件不存在则返回 None
            return None

    @staticmethod
    def getQQVersion():
        """
        ## 获取 QQ 的版本信息
        """
        try:
            # 获取 package.json 路径并读取
            package_file_path = it(PathFunc).getQQPath() / "resources/app/package.json"
            with open(str(package_file_path), "r", encoding="utf-8") as f:
                # 读取参数并返回版本信息
                package = json.loads(f.read())
            # 拼接字符串返回版本信息
            platform = "Windows" if package["platform"] == "win32" else "Linux"
            return f"{platform} {package['version']}"

        except FileNotFoundError:
            # 文件不存在则返回 None
            return None


class GetVersionClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Core.GetVersion", "GetVersion"),)

    # 静态方法available()，用于检查模块"PathFunc"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Core.GetVersion")

    # 静态方法create()，用于创建PathFunc类的实例，返回值为PathFunc对象。
    @staticmethod
    def create(create_type: [GetVersion]) -> GetVersion:
        return GetVersion()


add_creator(GetVersionClassCreator)
