# -*- coding: utf-8 -*-
"""加载打包进 Qt 资源系统的更新脚本。"""

from pathlib import Path

from PySide6.QtCore import QFile, QIODevice

from src.core.desktop_update.constants import MSI_UPDATE_SCRIPT_FILENAME


SCRIPT_DIR = Path(__file__).resolve().parents[2] / "resource" / "script"


def load_msi_update_script() -> str:
    """读取 MSI 更新脚本模板。"""

    return _load_script(
        resource_path=":/script/script/update_msi.bat",
        fallback_path=SCRIPT_DIR / MSI_UPDATE_SCRIPT_FILENAME,
    )


def _load_script(resource_path: str, fallback_path: Path) -> str:
    resource_file = QFile(resource_path)
    if resource_file.open(QIODevice.OpenModeFlag.ReadOnly | QIODevice.OpenModeFlag.Text):
        try:
            return bytes(resource_file.readAll()).decode("utf-8")  # type: ignore[arg-type]
        finally:
            resource_file.close()

    return fallback_path.read_text(encoding="utf-8")
