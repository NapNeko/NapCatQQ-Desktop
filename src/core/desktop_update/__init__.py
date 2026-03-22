# -*- coding: utf-8 -*-
"""Desktop 更新领域导出。"""

from src.core.desktop_update.constants import (
    MINIMUM_MSI_SIZE_BYTES,
    MSI_LOG_FILE,
    MSI_UPDATE_FILENAME,
    MSI_UPDATE_SCRIPT_FILENAME,
    PORTABLE_UPDATE_SCRIPT_FILENAME,
    REQUIRED_UPDATE_ENTRIES,
    UPDATE_APP_DIR_NAME,
    UPDATE_ARCHIVE_NAME,
    UPDATE_EXE_NAME,
    UPDATE_STAGING_DIR_NAME,
    UPDATE_STAGING_PACKAGE_DIR,
)
from src.core.desktop_update.manager import (
    MsiUpdateStrategy,
    PortableUpdateStrategy,
    UpdateManager,
    UpdateStrategy,
    get_staged_app_dir,
    get_update_stage_package_root,
    get_update_staging_root,
    prepare_desktop_update,
)
from src.core.desktop_update.models import DesktopUpdateManifest, DesktopUpdateMigration, DesktopUpdatePlan
from src.core.desktop_update.planner import resolve_desktop_update_plan
from src.core.desktop_update.scripts import fetch_remote_update_script, inject_target_pid
