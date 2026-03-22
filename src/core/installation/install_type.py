# -*- coding: utf-8 -*-
"""安装类型检测工具。

用于区分 MSI 安装版和便携版（ZIP 解压）
"""

# 标准库导入
import enum
import logging
from pathlib import Path

# 第三方库导入
try:
    import winreg

    HAS_WINREG = True
except ImportError:
    HAS_WINREG = False  # 非 Windows 环境

logger = logging.getLogger(__name__)


class InstallType(enum.Enum):
    """安装类型枚举。"""

    PORTABLE = "portable"
    """便携版（ZIP 解压，无注册表项）"""

    MSI = "msi"
    """MSI 安装版（有注册表项和卸载程序）"""

    UNKNOWN = "unknown"
    """无法确定类型"""


# MSI 注册表常量
MSI_REGISTRY_KEY = r"Software\NapCatQQ-Desktop"
MSI_REGISTRY_VALUE = "InstallDir"


def detect_install_type(app_path: Path | None = None) -> InstallType:
    r"""检测当前安装类型。

    检测逻辑：
    1. 检查注册表 HKLM\Software\NapCatQQ-Desktop\InstallDir
    2. 如果注册表存在且路径匹配，判定为 MSI
    3. 否则判定为便携版（存在 _internal 目录）

    Note:
        MSI 安装不会生成 Uninstall.exe，仅通过注册表区分。
        便携版解压后也没有 Uninstall.exe。

    Args:
        app_path: 应用根目录，默认使用当前运行目录

    Returns:
        InstallType: 检测到的安装类型
    """
    if app_path is None:
        # 冻结运行时从 exe 所在目录获取
        if getattr(__import__("sys"), "frozen", False):
            app_path = Path(__import__("sys").executable).parent
        else:
            # 源码运行时从项目根目录获取
            app_path = Path(__file__).parent.parent.parent.parent

    app_path = app_path.resolve()

    # 优先检查 MSI 注册表项
    if HAS_WINREG:
        msi_install_dir = _get_msi_install_dir_from_registry()
        if msi_install_dir is not None:
            if Path(msi_install_dir).resolve() == app_path:
                logger.debug(f"检测到 MSI 安装版: {app_path}")
                return InstallType.MSI
            else:
                logger.debug(f"注册表路径与当前路径不匹配: " f"registry={msi_install_dir}, current={app_path}")

    # 检查是否为便携版：存在 _internal 目录（PyInstaller 特征）
    if (app_path / "_internal").is_dir():
        logger.debug(f"检测到便携版: {app_path}")
        return InstallType.PORTABLE

    logger.warning(f"无法确定安装类型: {app_path}")
    return InstallType.UNKNOWN


def _get_msi_install_dir_from_registry() -> str | None:
    """从注册表获取 MSI 安装目录。

    Returns:
        str | None: 安装目录路径，未找到时返回 None
    """
    if not HAS_WINREG:
        return None

    # 延迟导入避免类型检查问题
    import winreg as _winreg

    # 尝试 64 位注册表
    try:
        with _winreg.OpenKey(
            _winreg.HKEY_LOCAL_MACHINE, MSI_REGISTRY_KEY, 0, _winreg.KEY_READ | _winreg.KEY_WOW64_64KEY
        ) as key:
            install_dir, _ = _winreg.QueryValueEx(key, MSI_REGISTRY_VALUE)
            return str(install_dir) if install_dir else None
    except FileNotFoundError:
        pass
    except OSError as e:
        logger.debug(f"读取 64 位注册表失败: {e}")

    # 尝试 32 位注册表（兼容旧系统）
    try:
        with _winreg.OpenKey(
            _winreg.HKEY_LOCAL_MACHINE, MSI_REGISTRY_KEY, 0, _winreg.KEY_READ | _winreg.KEY_WOW64_32KEY
        ) as key:
            install_dir, _ = _winreg.QueryValueEx(key, MSI_REGISTRY_VALUE)
            return str(install_dir) if install_dir else None
    except FileNotFoundError:
        pass
    except OSError as e:
        logger.debug(f"读取 32 位注册表失败: {e}")

    return None


def _is_portable_installation(app_path: Path) -> bool:
    """检查是否为便携版安装。

    便携版特征：
    - 存在 _internal 目录（PyInstaller 打包特征）

    Note:
        MSI 安装和便携版都没有 Uninstall.exe，
        所以仅通过注册表（MSI）和 _internal 目录（便携版）区分。

    Args:
        app_path: 应用根目录

    Returns:
        bool: 是否为便携版
    """
    return (app_path / "_internal").is_dir()


def get_update_file_pattern(install_type: InstallType, version: str) -> str:
    """获取更新文件名模式。

    统一命名格式：
    - 便携版: NapCatQQ-Desktop-{version}-portable-x64.zip
    - MSI: NapCatQQ-Desktop-{version}-x64.msi

    Args:
        install_type: 安装类型
        version: 版本号（不含 v 前缀）

    Returns:
        str: 更新文件名
    """
    if install_type == InstallType.MSI:
        return f"NapCatQQ-Desktop-{version}-x64.msi"
    else:
        return f"NapCatQQ-Desktop-{version}-portable-x64.zip"


def is_msi_installed_version(version: str) -> bool:
    """检查指定版本是否为 MSI 安装。

    用于版本回滚检测。

    Args:
        version: 版本号

    Returns:
        bool: 是否为 MSI 安装版本
    """
    # 简化实现：当前仅检查安装类型
    # 未来可扩展为检查注册表中记录的版本号
    return detect_install_type() == InstallType.MSI
