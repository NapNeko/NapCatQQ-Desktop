# -*- coding: utf-8 -*-
# 标准库导入
import shutil
import winreg
from pathlib import Path
from abc import ABC

# 项目内模块导入
from src.core.utils.app_path import resolve_app_base_path, resolve_app_data_path
from src.core.utils.logger import LogSource, LogType, logger
from creart import exists_module, AbstractCreator, CreateTargetInfo, add_creator


class OldVersionPath:
    """旧版本路径类

    用于存储旧版本的路径信息, 以便进行版本迁移
    """

    @staticmethod
    def v1613(base_path: Path) -> dict[str, Path]:
        """NapCatQQ Desktop v1.6.13 及更早版本的路径, 仅包含文件夹变化"""
        return {
            "napcat_path": base_path / "NapCat",
            "config_dir_path": base_path / "config",
            "tmp_path": base_path / "tmp",
        }

    @staticmethod
    def install_runtime_layout(base_path: Path) -> dict[str, Path]:
        """旧版将可写运行时数据放在安装目录 runtime/ 下的路径布局。"""
        runtime_root = base_path / "runtime"
        return {
            "napcat_path": runtime_root / "NapCatQQ",
            "config_dir_path": runtime_root / "config",
            "tmp_path": runtime_root / "tmp",
        }


class PathFunc:
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

    def __init__(self) -> None:
        """初始化"""

        # 基础路径字段
        self.base_path = resolve_app_base_path()
        self.data_path = resolve_app_data_path()
        self.runtime_path = self.data_path / "runtime"

        # 运行时路径字段
        self.qq_path = None
        self.napcat_path = self.runtime_path / "NapCatQQ"
        self.config_dir_path = self.runtime_path / "config"
        self.tmp_path = self.runtime_path / "tmp"

        # 文件字段
        self.config_path = self.config_dir_path / "config.json"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.napcat_config_path = self.napcat_path / "config"

        # 检查迁移
        self.path_migration()

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

    def path_migration(self) -> None:
        """路径迁移

        检查并迁移旧版本的路径到当前版本(目前只有v1.6.13及更早版本与当前版本不兼容)
        """
        legacy_path_sets = [OldVersionPath.v1613(self.base_path)]
        if self.data_path != self.base_path:
            legacy_path_sets.append(OldVersionPath.install_runtime_layout(self.base_path))

        # 检查是否需要迁移
        if not any(path.exists() for old_paths in legacy_path_sets for path in old_paths.values()):
            logger.debug("无需进行路径迁移", LogType.FILE_FUNC, LogSource.CORE)
            return

        # 进行迁移
        for old_paths in legacy_path_sets:
            for path_name, old_path in old_paths.items():
                if not old_path.exists():
                    # 不存在旧版文件则跳过
                    continue

                # 通过 getattr 获取新的路径
                new_path = getattr(self, path_name)

                if old_path.resolve() == new_path.resolve():
                    continue

                new_path.parent.mkdir(parents=True, exist_ok=True)

                # 检查文件夹名称是否有改变
                if old_path.name != new_path.name:
                    # 如果文件夹名称改变, 则直接移动整个文件夹
                    shutil.move(str(old_path), str(new_path))
                    logger.debug(f"已将旧版路径 {old_path} 整体迁移至 {new_path}", LogType.FILE_FUNC, LogSource.CORE)

                else:
                    # 如果文件夹名称未改变, 则逐个移动文件
                    for item in old_path.iterdir():
                        target_path: Path = new_path / item.name
                        target_path.parent.mkdir(parents=True, exist_ok=True)
                        shutil.move(str(item), str(target_path))
                        logger.debug(f"已将旧版路径 {item} 迁移至 {target_path}", LogType.FILE_FUNC, LogSource.CORE)

                    # 删除旧版空文件夹
                    try:
                        old_path.rmdir()
                        logger.debug(f"已删除旧版空文件夹 {old_path}", LogType.FILE_FUNC, LogSource.CORE)
                    except OSError:
                        logger.warning(f"无法删除旧版文件夹 {old_path}, 请手动删除", LogType.FILE_FUNC, LogSource.CORE)

        logger.debug("路径迁移完成", LogType.FILE_FUNC, LogSource.CORE)


class PathFuncCreator(AbstractCreator, ABC):
    """路径处理类创建器"""

    targets = (
        CreateTargetInfo(
            module="src.core.utils.path_func",
            identify="PathFunc",
            humanized_name="路径处理类",
            description="NapCatQQ Desktop 路径处理类",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断路径处理类模块是否可用"""
        return exists_module("src.core.utils.path_func")

    @staticmethod
    def create(create_type):
        """创建路径处理类实例"""
        return create_type()


add_creator(PathFuncCreator)
