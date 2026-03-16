# -*- coding: utf-8 -*-

# 标准库导入
import stat
import zipfile
from pathlib import Path

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.utils.desktop_update import (
    UPDATE_APP_DIR_NAME,
    UPDATE_EXE_NAME,
    prepare_desktop_update,
)
from src.ui.page.unit_page.napcat_desktop_page import NCDPage


def test_prepare_desktop_update_extracts_valid_package(tmp_path: Path) -> None:
    """合法整包应被解压并返回应用目录。"""

    zip_path = tmp_path / "NapCatQQ-Desktop.zip"
    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/{UPDATE_EXE_NAME}", "exe")
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/_internal/bootstrap.txt", "ok")
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/lib/foo.dll", "dll")

    app_dir = prepare_desktop_update(zip_path, tmp_path)

    assert app_dir == tmp_path / "_update_staging" / "package" / UPDATE_APP_DIR_NAME
    assert (app_dir / UPDATE_EXE_NAME).is_file()


def test_prepare_desktop_update_rejects_invalid_layout(tmp_path: Path) -> None:
    """缺少顶层目录或 exe 时应中止更新。"""

    zip_path = tmp_path / "NapCatQQ-Desktop.zip"
    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr("NapCatQQ-Desktop/readme.txt", "missing exe")

    with pytest.raises(ValueError, match="更新包结构不正确"):
        prepare_desktop_update(zip_path, tmp_path)


def test_prepare_desktop_update_requires_internal_runtime_dir(tmp_path: Path) -> None:
    """目录版更新包缺少 _internal 时必须被拒绝。"""

    zip_path = tmp_path / "NapCatQQ-Desktop.zip"
    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/{UPDATE_EXE_NAME}", "exe")

    with pytest.raises(ValueError, match="_internal"):
        prepare_desktop_update(zip_path, tmp_path)


def test_prepare_desktop_update_rejects_traversal_member(tmp_path: Path) -> None:
    """目录穿越条目必须被拒绝。"""

    zip_path = tmp_path / "NapCatQQ-Desktop.zip"
    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr("../bad.txt", "bad")

    with pytest.raises(ValueError, match="非法相对路径"):
        prepare_desktop_update(zip_path, tmp_path)


def test_prepare_desktop_update_removes_readonly_staging_dir(tmp_path: Path) -> None:
    """旧 staging 中的只读文件应能在 Windows 风格场景下被清理。"""

    staging_root = tmp_path / "_update_staging"
    readonly_file = staging_root / "package" / "stale.txt"
    readonly_file.parent.mkdir(parents=True, exist_ok=True)
    readonly_file.write_text("stale", encoding="utf-8")
    readonly_file.chmod(stat.S_IREAD)

    zip_path = tmp_path / "NapCatQQ-Desktop.zip"
    with zipfile.ZipFile(zip_path, "w") as archive:
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/{UPDATE_EXE_NAME}", "exe")
        archive.writestr(f"{UPDATE_APP_DIR_NAME}/_internal/bootstrap.txt", "ok")

    app_dir = prepare_desktop_update(zip_path, tmp_path)

    assert app_dir == tmp_path / "_update_staging" / "package" / UPDATE_APP_DIR_NAME
    assert not readonly_file.exists()


def test_update_script_template_contains_backup_and_rollback() -> None:
    """正式 updater 模板必须包含备份和回滚分支。"""

    script_path = Path(__file__).resolve().parents[2] / "src" / "resource" / "script" / "update.bat"
    script_content = script_path.read_text(encoding="utf-8")

    assert 'set "backup_root=%app_root%\\_update_staging\\backup"' in script_content
    assert 'set "backup_app_dir=%backup_root%\\NapCatQQ-Desktop"' in script_content
    assert 'robocopy "%app_root%" "%backup_app_dir%" /MIR' in script_content
    assert ":rollback" in script_content
    assert 'robocopy "%backup_app_dir%" "%app_root%" /MIR' in script_content


def test_load_update_script_contains_backup_and_rollback() -> None:
    """页面模块读取到的 updater 模板必须包含备份和回滚分支。"""

    script_content = NCDPage._load_update_script()

    assert 'set "backup_root=%app_root%\\_update_staging\\backup"' in script_content
    assert 'set "backup_app_dir=%backup_root%\\NapCatQQ-Desktop"' in script_content
    assert 'robocopy "%app_root%" "%backup_app_dir%" /MIR' in script_content
    assert ":rollback" in script_content
    assert 'robocopy "%backup_app_dir%" "%app_root%" /MIR' in script_content
