# -*- coding: utf-8 -*-
"""Desktop 更新 UI 文案与提示拼装。"""

from collections.abc import Callable
from pathlib import Path

from src.core.installation.install_type import InstallType


def build_update_confirmation_message(
    translate: Callable[[str], str],
    install_type: InstallType,
    local_version: str | None,
    remote_version: str | None,
    has_running_bot: bool,
    test_mode: bool = False,
) -> str:
    """构建 Desktop 更新确认弹窗文案。"""

    install_type_text, process_text = _get_install_type_copy(translate, install_type)
    warning_text = translate("⚠️ 所有运行中的 Bot 将被强制关闭\n\n") if has_running_bot else ""

    if test_mode:
        version_left = translate("当前版本")
        version_right = translate("新版本")
        prefix = "[开发者模式测试]\n\n"
    else:
        version_left = local_version or translate("未安装")
        version_right = remote_version or translate("未知版本")
        prefix = ""

    version_text = translate("版本: {} → {}\n\n").format(version_left, version_right)
    return (
        f"{prefix}"
        f"{version_text}"
        f"安装类型: {install_type_text}\n\n"
        f"{warning_text}"
        f"{process_text}\n\n"
        f"是否继续更新？"
    )


def get_download_message(translate: Callable[[str], str], install_type: InstallType) -> str:
    """返回当前安装类型的下载提示。"""

    if install_type == InstallType.MSI:
        return translate("正在下载 NapCat Desktop MSI 安装包...")
    return translate("正在下载 NapCat Desktop 便携版压缩包...")


def get_install_type_log_prefix(install_type: InstallType) -> str:
    """返回更新日志前缀。"""

    if install_type == InstallType.MSI:
        return "[MSI 安装版]\n"
    if install_type == InstallType.PORTABLE:
        return "[便携版]\n"
    return ""


def build_install_type_details(
    translate: Callable[[str], str],
    install_type: InstallType,
    base_path: Path,
) -> list[str]:
    """构建安装类型诊断详情。"""

    details = [
        f"检测到的安装类型: {install_type.value}",
        f"应用路径: {base_path}",
    ]

    if install_type == InstallType.MSI:
        details.append(translate("注册表项: HKLM\\Software\\NapCatQQ-Desktop\\InstallDir"))
        details.append(translate("更新将使用 MSI 安装包 (.msi)"))
    elif install_type == InstallType.PORTABLE:
        details.append(f"_internal 目录存在: {(base_path / '_internal').exists()}")
        details.append(translate("更新将使用便携版压缩包 (.zip)"))
    else:
        details.append(translate("无法确定安装类型，将使用便携版更新"))

    return details


def _get_install_type_copy(translate: Callable[[str], str], install_type: InstallType) -> tuple[str, str]:
    if install_type == InstallType.MSI:
        return (
            translate("MSI 安装版"),
            translate(
                "更新流程:\n"
                "1. 下载新版本 MSI 安装包\n"
                "2. 关闭当前程序并等待完全退出\n"
                "3. 以管理员权限运行 MSI 升级安装\n"
                "4. 安装完成后自动启动新版本\n\n"
                '注意: 安装过程中会弹出 UAC 权限请求，请点击"是"继续。'
            ),
        )

    return (
        translate("便携版"),
        translate(
            "更新流程:\n"
            "1. 下载新版本压缩包\n"
            "2. 关闭当前程序并等待完全退出\n"
            "3. 解压并替换程序文件\n"
            "4. 自动启动新版本\n\n"
            "注意: 更新过程可能需要管理员权限。"
        ),
    )

