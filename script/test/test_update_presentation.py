# -*- coding: utf-8 -*-

import importlib.util
from pathlib import Path

from src.desktop.core.installation.install_type import InstallType


_PRESENTATION_PATH = Path(__file__).resolve().parents[2] / "src" / "ui" / "page" / "component_page" / "utils" / "presentation.py"
_PRESENTATION_SPEC = importlib.util.spec_from_file_location("test_presentation_module", _PRESENTATION_PATH)
assert _PRESENTATION_SPEC is not None and _PRESENTATION_SPEC.loader is not None
_PRESENTATION_MODULE = importlib.util.module_from_spec(_PRESENTATION_SPEC)
_PRESENTATION_SPEC.loader.exec_module(_PRESENTATION_MODULE)

build_install_type_details = _PRESENTATION_MODULE.build_install_type_details
build_update_confirmation_message = _PRESENTATION_MODULE.build_update_confirmation_message
get_download_message = _PRESENTATION_MODULE.get_download_message
get_install_type_log_prefix = _PRESENTATION_MODULE.get_install_type_log_prefix


def _translate(text: str) -> str:
    return text


def test_get_download_message_switches_portable_copy_to_msi_migration() -> None:
    assert get_download_message(_translate, InstallType.MSI) == "正在下载 NapCat Desktop MSI 安装包..."
    assert (
        get_download_message(_translate, InstallType.PORTABLE)
        == "正在下载 NapCat Desktop MSI 安装包，准备将当前便携版迁移为 MSI 安装..."
    )
    assert get_download_message(_translate, InstallType.UNKNOWN) == "正在下载 NapCat Desktop MSI 安装包..."


def test_get_install_type_log_prefix_marks_portable_migration() -> None:
    assert get_install_type_log_prefix(InstallType.MSI) == "[MSI 安装版]\n"
    assert get_install_type_log_prefix(InstallType.PORTABLE) == "[便携版→MSI 迁移]\n"
    assert get_install_type_log_prefix(InstallType.UNKNOWN) == "[未知安装类型→MSI 安装]\n"


def test_build_install_type_details_for_portable_mentions_msi_migration(tmp_path: Path) -> None:
    details = build_install_type_details(_translate, InstallType.PORTABLE, tmp_path)

    assert details[0] == "检测到的安装类型: portable"
    assert any("迁移到 MSI 安装版" in item for item in details)


def test_build_update_confirmation_message_for_portable_mentions_zip_disable() -> None:
    message = build_update_confirmation_message(
        translate=_translate,
        install_type=InstallType.PORTABLE,
        local_version="v1.9.0",
        remote_version="v2.0.0",
        has_running_bot=False,
    )

    assert "便携版（将迁移到 MSI 安装版）" in message
    assert "ZIP 覆盖更新已停用" in message
