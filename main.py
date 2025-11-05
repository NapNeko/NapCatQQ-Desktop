# -*- coding: utf-8 -*-
# 标准库导入
import json
import os
import sys

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.utils.mutex import SingleInstanceApplication
from src.core.utils.path_func import PathFunc

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    # 执行路径验证
    PathFunc().path_validator()

    # 设置DPI缩放 - 直接读取配置文件以避免加载qfluentwidgets
    # 这样可以在QApplication创建前应用DPI设置，同时避免加载大型UI库
    config_path = PathFunc().config_path
    dpi_scale = "Auto"  # 默认值
    try:
        if config_path.exists():
            with open(config_path, "r", encoding="utf-8") as f:
                config_data = json.load(f)
                # 配置文件使用嵌套结构: Personalized.DpiScale
                dpi_scale = config_data.get("Personalized", {}).get("DpiScale", "Auto")
    except (json.JSONDecodeError, OSError):
        pass  # 使用默认值
    
    if dpi_scale == "Auto":
        QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    else:
        os.environ["QT_SCALE_FACTOR"] = str(dpi_scale)

    app = QApplication(sys.argv)

    # 延迟加载配置模块（包含qfluentwidgets），在QApplication创建后加载
    # qfluentwidgets是一个大型UI库，延迟加载可显著提升启动速度
    from src.core.config import cfg

    # 延迟加载资源文件（202,000行的编译资源），在QApplication创建后加载
    # 这样可以显著提升启动速度
    from src.resource import resource

    # 延迟加载字体，在窗口显示之前加载以提升启动速度
    # 字体加载移至窗口初始化时进行

    if cfg.get(cfg.main_window) and cfg.get(cfg.elua_accepted):
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        MainWindow().initialize()
    else:
        # 项目内模块导入
        from src.ui.window.guide_window import GuideWindow

        GuideWindow().initialize()

    sys.exit(app.exec())
