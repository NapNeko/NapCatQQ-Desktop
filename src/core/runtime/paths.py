# -*- coding: utf-8 -*-
# 标准库导入
import shutil
import tempfile
import winreg
from pathlib import Path
from abc import ABC

# 项目内模块导入
from src.core.platform.app_paths import resolve_app_base_path, resolve_app_data_path
from src.core.logging import LogSource, LogType, logger
from creart import exists_module, AbstractCreator, CreateTargetInfo, add_creator


# 更新流程运行结束后可能遗留的文件名模式, 由 path_validator 清理
_UPDATE_LEFTOVER_PATTERNS: tuple[str, ...] = (
    "NapCatQQ-Desktop*.msi",
    "msi_update.log",
    "update_msi.bat",
)


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
        self.snowluma_path = self.runtime_path / "SnowLuma"
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

        paths_to_validate = [
            (self.tmp_path, "Tmp"),
            (self.config_dir_path, "config"),
            (self.napcat_path, "NapCat"),
            (self.snowluma_path, "SnowLuma"),
        ]

        for path, name in paths_to_validate:
            if not path.exists():
                path.mkdir(parents=True, exist_ok=True)
                logger.info(f"创建路径 {name.center(8)} 成功", LogType.FILE_FUNC, LogSource.CORE)
            else:
                logger.info(f"路径 {name.center(8)} 已存在", LogType.FILE_FUNC, LogSource.CORE)

        self.cleanup_stale_update_artifacts()

    def cleanup_stale_update_artifacts(self) -> None:
        """清理 Desktop 更新流程遗留的安装包等临时文件.

        msiexec 在新版应用启动前已经退出, 此时残留在 ``runtime/tmp`` 与系统临时目录
        ``%TEMP%/NapCatQQ-Desktop/update`` 下的 MSI 安装包不再被使用, 应当主动清理,
        否则会持续占用 ``C:\\ProgramData\\NapCatQQ Desktop\\runtime\\tmp`` 空间.

        本方法只清理已知的更新产物 (MSI 文件、更新日志、旧版批处理脚本),
        不会触碰 ``.msi.part`` 等仍在下载的中间文件, 以保留断点续传能力.
        """
        candidate_dirs: list[Path] = [self.tmp_path]
        try:
            candidate_dirs.append(Path(tempfile.gettempdir()) / "NapCatQQ-Desktop" / "update")
        except OSError as exc:
            logger.warning(
                f"获取系统临时目录失败, 跳过备用更新目录清理: {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

        for directory in candidate_dirs:
            try:
                if not directory.is_dir():
                    continue
            except OSError as exc:
                logger.warning(
                    f"检查更新临时目录失败 {directory}: {exc}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
                continue

            for pattern in _UPDATE_LEFTOVER_PATTERNS:
                for leftover in directory.glob(pattern):
                    if not leftover.is_file():
                        continue
                    try:
                        leftover.unlink()
                        logger.info(
                            f"已清理更新残留文件: {leftover}",
                            LogType.FILE_FUNC,
                            LogSource.CORE,
                        )
                    except OSError as exc:
                        logger.warning(
                            f"清理更新残留文件失败 {leftover}: {exc}",
                            LogType.FILE_FUNC,
                            LogSource.CORE,
                        )

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

    # ==================== SnowLuma 路径 ====================
    def get_snowluma_node_executable(self) -> Path | None:
        """返回 SnowLuma 发布包自带的 ``node.exe``路径.

        不存在时返回 ``None``, 调用方据此判断是否需要提示用户先安装
        SnowLuma (走组件页 SnowLuma tab).

        上游发布包例: ``SnowLuma-v1.7.5-win-x64/node.exe`` (Node.js v22.22.2 portable).
        """
        node_exe = self.snowluma_path / "node.exe"
        return node_exe if node_exe.exists() else None

    def get_snowluma_entry(self) -> Path:
        """返回 SnowLuma 主入口 ``index.mjs`` 路径.

        该路径不会检查是否存在; 仅提供给 QProcess.setArguments 使用.
        是否可启动请配合 :meth:`get_snowluma_node_executable` 使用.
        """
        return self.snowluma_path / "index.mjs"

    def get_snowluma_config_dir(self) -> Path:
        """返回 SnowLuma 配置目录, 该目录包含 ``runtime.json``、``webui.json``、
        ``onebot_<uin>.json`` 等 SnowLuma 本地配置文件.

        Desktop 在启动 SnowLuma 进程前会向该目录渲染 ``onebot_<QQID>.json`` (参见
        :mod:`src.core.runtime.snowluma_config_renderer`).
        """
        return self.snowluma_path / "config"

    def get_snowluma_data_dir(self) -> Path:
        """返回 SnowLuma 运行时数据目录, 其下以登录 ``uin`` 为子目录,
        包含 SnowLuma sqlite (``messages.db``、``media.db``) 等.

        Desktop 不直接写入该目录, 仅在覆盖安装 SnowLuma 时保留该子树.
        """
        return self.snowluma_path / "data"

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
            module="src.core.runtime.paths",
            identify="PathFunc",
            humanized_name="路径处理类",
            description="NapCatQQ Desktop 路径处理类",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断路径处理类模块是否可用"""
        return exists_module("src.core.runtime.paths")

    @staticmethod
    def create(create_type):
        """创建路径处理类实例"""
        return create_type()


add_creator(PathFuncCreator)

