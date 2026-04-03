# -*- coding: utf-8 -*-
"""Desktop 更新领域导出。"""

from src.core.desktop_update.constants import (
    MINIMUM_MSI_SIZE_BYTES,
    MSI_LOG_FILE,
    MSI_UPDATE_FILENAME,
    MSI_UPDATE_SCRIPT_FILENAME,
)
from src.core.desktop_update.manager import (
    MsiUpdateStrategy,
    UpdateLaunchResult,
    UpdateManager,
)
from src.core.desktop_update.scripts import inject_target_pid
