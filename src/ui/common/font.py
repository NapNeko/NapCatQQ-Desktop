# -*- coding: utf-8 -*-
# font_manager.py
from PySide6.QtGui import QFontDatabase

# 项目内模块导入
from src.core.utils.logger import logger


class FontManager:
    _font_loaded = False
    _font_family = None

    @classmethod
    def initialize_fonts(cls) -> None:
        """在应用程序启动时调用，初始化所有字体"""

        font_paths = [
            ":/font/font/JB-MONO.ttf",
            ":/font/font/ALMM.ttf",
        ]

        if not cls._font_loaded:
            cls._load_fonts(font_paths)
            cls._font_loaded = True

    @classmethod
    def _load_fonts(cls, font_paths: list) -> None:
        """加载多个字体"""
        [cls._load_font(font_path) for font_path in font_paths]

    @classmethod
    def _load_font(cls, font_path: str) -> None:
        """加载字体"""
        if (font_id := QFontDatabase.addApplicationFont(font_path)) == -1:
            logger.error("字体加载失败")
            return

        if font_families := QFontDatabase.applicationFontFamilies(font_id):
            cls._font_family = font_families[0]
            logger.info(f"字体加载成功: {cls._font_family}")
            return

        logger.error("未找到字体家族")
