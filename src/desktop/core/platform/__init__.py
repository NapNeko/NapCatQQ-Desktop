# -*- coding: utf-8 -*-
"""平台与进程启动相关能力。"""

from src.desktop.core.platform.app_paths import APP_DATA_DIR_NAME, resolve_app_base_path, resolve_app_data_path
from src.desktop.core.platform.runtime_args import (
    RuntimeLaunchOptions,
    apply_runtime_launch_options,
    get_runtime_launch_options,
    is_developer_mode_enabled,
    parse_runtime_launch_options,
)
