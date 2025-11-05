# -*- coding: utf-8 -*-
# 标准库导入
import shutil
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

    class OldVersionPath:
        """旧版本路径类

        用于存储旧版本的路径信息, 以便进行版本迁移
        """

        @staticmethod
        def _v1613() -> dict[str, Path]:
            """NapCatQQ Desktop v1.6.13 及更早版本的路径, 仅包含文件夹变化"""
            return {
                "napcat_path": Path.cwd() / "NapCat",
                "config_dir_path": Path.cwd() / "config",
                "tmp_path": Path.cwd() / "tmp",
            }

    def __init__(self) -> None:
        """初始化"""

        # 基础路径字段
        self.base_path = Path.cwd()
        self.runtime_path = self.base_path / "runtime"

        # 运行时路径字段
        self.qq_path = None
        self.napcat_path = self.runtime_path / "NapCatQQ"
        self.config_dir_path = self.runtime_path / "config"
        self.tmp_path = self.runtime_path / "tmp"

        # 文件字段
        self.config_path = self.config_dir_path / "config.json"
        self.bot_config_path = self.config_dir_path / "bot.json"
        self.napcat_config_path = self.napcat_path / "config"

        # 延迟执行迁移检查，提升启动速度
        # 迁移检查将在 path_validator 首次调用时执行
        self._migration_checked = False

    def path_validator(self) -> None:
        """验证一系列路径"""

        # 首次调用时执行迁移检查
        if not self._migration_checked:
            self.path_migration()
            self._migration_checked = True

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
        # 获取旧版文件夹路径大全
        old_paths = self.OldVersionPath._v1613()

        # 检查是否需要迁移
        if not Path(old_paths["config_dir_path"]).exists():
            logger.debug("无需进行路径迁移", LogType.FILE_FUNC, LogSource.CORE)
            return

        # 进行迁移
        for path_name, old_path in old_paths.items():
            if not old_path.exists():
                # 不存在旧版文件则跳过
                continue

            # 通过 getattr 获取新的路径
            new_path = getattr(self, path_name)

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
